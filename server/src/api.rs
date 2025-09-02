use std::{collections::HashMap, sync::Arc};
use axum::{extract::{State, Path}, Json};
use axum::http::HeaderMap;
use serde::{Deserialize, Serialize};
use tokio::sync::{RwLock, broadcast};

use crate::{auth, models::*, error::{AppError, Result}};

#[derive(Clone)]
pub struct AppState {
    pub pool: sqlx::SqlitePool,
    pub jwt_secret: String,
    // broadcast per conversation
    pub channels: Arc<RwLock<HashMap<i64, broadcast::Sender<serde_json::Value>>>>,
}
impl AppState {
    pub fn new(pool: sqlx::SqlitePool, jwt_secret: String) -> Self {
        Self { pool, jwt_secret, channels: Arc::new(RwLock::new(HashMap::new())) }
    }
}

// --- helpers ---
fn bearer(headers:&HeaderMap) -> Result<String> {
    let v = headers.get(axum::http::header::AUTHORIZATION).ok_or(AppError::Unauthorized)?;
    let s = v.to_str().map_err(|_| AppError::Unauthorized)?;
    Ok(s.strip_prefix("Bearer ").ok_or(AppError::Unauthorized)?.to_string())
}

async fn user_id_from(headers:&HeaderMap, state:&AppState) -> Result<i64> {
    let t = bearer(headers)?;
    let c = auth::decode_jwt(&t, &state.jwt_secret).map_err(|_| AppError::Unauthorized)?;
    Ok(c.sub)
}

async fn ensure_channel(state:&AppState, cid:i64) -> broadcast::Sender<serde_json::Value> {
    let mut map = state.channels.write().await;
    map.entry(cid).or_insert_with(|| broadcast::channel(256).0).clone()
}

// --- payloads ---
#[derive(Deserialize)]
pub struct RegisterReq { pub username:String, pub password:String }
#[derive(Serialize)]
pub struct LoginResp { pub token:String }
#[derive(Deserialize)]
pub struct GroupReq { pub name:String }
#[derive(Deserialize)]
pub struct InviteReq { pub group_id:i64 }
#[derive(Serialize)]
pub struct InviteResp { pub token:String }
#[derive(Deserialize)]
pub struct JoinByTokenReq { pub token:String }
#[derive(Deserialize)]
pub struct DmOpenReq { pub peer_username:String }
#[derive(Deserialize)]
pub struct PostMsgReq { pub body:String }
#[derive(Deserialize)]
pub struct ReadReq { pub msg_id:i64 }

// --- handlers ---
pub async fn register(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RegisterReq>
) -> Result<Json<serde_json::Value>> {
    if req.username.is_empty() || req.password.len()<4 {
        return Err(AppError::BadRequest("invalid creds".into()));
    }
    let hash = auth::hash_password(&req.password)
        .map_err(|e| AppError::BadRequest(e.to_string()))?;

    let res = sqlx::query(
        "INSERT INTO users(username, pass_hash, created_at)
         VALUES(?, ?, strftime('%s','now'))"
    )
        .bind(&req.username)
        .bind(hash)
        .execute(&state.pool)
        .await?; // grazie all'impl From<sqlx::Error> per AppError

    let id = res.last_insert_rowid();
    Ok(Json(serde_json::json!({ "id": id })))
}

pub async fn login(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RegisterReq>
) -> Result<Json<LoginResp>> {
    let row = sqlx::query_as::<_, (i64,String)>(
        "SELECT id, pass_hash FROM users WHERE username=?"
    )
        .bind(&req.username)
        .fetch_optional(&state.pool)
        .await?;

    if let Some((uid, hash)) = row {
        if auth::verify_password(&req.password, &hash) {
            let token = auth::issue_jwt(uid, &state.jwt_secret, 8);
            return Ok(Json(LoginResp{ token }));
        }
    }
    Err(AppError::Unauthorized)
}

pub async fn create_group(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<GroupReq>
) -> Result<Json<serde_json::Value>> {
    let uid = user_id_from(&headers, &state).await?;

    let group_id = sqlx::query(
        "INSERT INTO groups(name, owner_id, created_at)
         VALUES(?, ?, strftime('%s','now'))"
    )
        .bind(&req.name)
        .bind(uid)
        .execute(&state.pool)
        .await?
        .last_insert_rowid();

    let cid = sqlx::query(
        "INSERT INTO conversations(kind, title, created_at)
         VALUES('group', ?, strftime('%s','now'))"
    )
        .bind(&req.name)
        .execute(&state.pool)
        .await?
        .last_insert_rowid();

    sqlx::query("INSERT INTO participants(conversation_id,user_id,role) VALUES(?, ?, 'admin')")
        .bind(cid).bind(uid).execute(&state.pool).await?;
    sqlx::query("INSERT INTO group_members(user_id,group_id,role) VALUES(?, ?, 'admin')")
        .bind(uid).bind(group_id).execute(&state.pool).await?;

    Ok(Json(serde_json::json!({ "group_id": group_id, "conversation_id": cid })))
}

pub async fn create_invite(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<InviteReq>
) -> Result<Json<InviteResp>> {
    let uid = user_id_from(&headers, &state).await?;

    let is_admin: Option<i64> = sqlx::query_scalar(
        "SELECT 1 FROM group_members
         WHERE user_id=? AND group_id=? AND role IN ('admin','owner')"
    )
        .bind(uid).bind(req.group_id)
        .fetch_optional(&state.pool)
        .await?;
    if is_admin.is_none() { return Err(AppError::Forbidden); }

    let token = uuid::Uuid::new_v4().to_string();

    sqlx::query(
        "INSERT INTO invites(group_id, token, expires_at, used)
         VALUES(?, ?, strftime('%s','now')+86400, 0)"
    )
        .bind(req.group_id)
        .bind(&token)
        .execute(&state.pool)
        .await?;

    Ok(Json(InviteResp{ token }))
}

pub async fn join_group_by_token(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<JoinByTokenReq>
) -> Result<Json<serde_json::Value>> {
    let uid = user_id_from(&headers, &state).await?;

    let row = sqlx::query_as::<_, (i64,i64)>(
        "SELECT id, group_id FROM invites
         WHERE token=? AND used=0 AND expires_at > strftime('%s','now')"
    )
        .bind(&req.token)
        .fetch_optional(&state.pool)
        .await?;
    let (inv_id, gid) = row.ok_or(AppError::BadRequest("invalid token".into()))?;

    sqlx::query("INSERT OR IGNORE INTO group_members(user_id,group_id,role) VALUES(?, ?, 'member')")
        .bind(uid).bind(gid).execute(&state.pool).await?;
    sqlx::query("UPDATE invites SET used=1 WHERE id=?")
        .bind(inv_id).execute(&state.pool).await?;

    // conversation di gruppo (titolo = nome gruppo)
    let cid_opt: Option<i64> = sqlx::query_scalar(
        "SELECT c.id FROM conversations c
         WHERE c.kind='group' AND c.title = (SELECT name FROM groups WHERE id=?)"
    )
        .bind(gid)
        .fetch_optional(&state.pool).await?;

    let cid = if let Some(id) = cid_opt {
        id
    } else {
        sqlx::query(
            "INSERT INTO conversations(kind, title, created_at)
             VALUES('group', (SELECT name FROM groups WHERE id=?), strftime('%s','now'))"
        )
            .bind(gid)
            .execute(&state.pool)
            .await?
            .last_insert_rowid()
    };

    sqlx::query("INSERT OR IGNORE INTO participants(conversation_id,user_id,role) VALUES(?, ?, 'member')")
        .bind(cid).bind(uid).execute(&state.pool).await?;

    Ok(Json(serde_json::json!({ "conversation_id": cid })))
}

pub async fn open_dm(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<DmOpenReq>
) -> Result<Json<serde_json::Value>> {
    let me = user_id_from(&headers, &state).await?;

    let peer: Option<i64> = sqlx::query_scalar("SELECT id FROM users WHERE username=?")
        .bind(&req.peer_username)
        .fetch_optional(&state.pool)
        .await?;
    let peer = peer.ok_or(AppError::BadRequest("peer not found".into()))?;

    if let Some(cid) = sqlx::query_scalar::<_, i64>(
        r#"SELECT c.id FROM conversations c
           JOIN participants p1 ON p1.conversation_id=c.id AND p1.user_id=?
           JOIN participants p2 ON p2.conversation_id=c.id AND p2.user_id=?
           WHERE c.kind='dm' LIMIT 1"#
    )
        .bind(me).bind(peer)
        .fetch_optional(&state.pool).await? {
        return Ok(Json(serde_json::json!({ "conversation_id": cid })));
    }

    let cid = sqlx::query("INSERT INTO conversations(kind, created_at) VALUES('dm', strftime('%s','now'))")
        .execute(&state.pool).await?.last_insert_rowid();

    sqlx::query("INSERT INTO participants(conversation_id,user_id) VALUES(?, ?)")
        .bind(cid).bind(me).execute(&state.pool).await?;
    sqlx::query("INSERT INTO participants(conversation_id,user_id) VALUES(?, ?)")
        .bind(cid).bind(peer).execute(&state.pool).await?;

    Ok(Json(serde_json::json!({ "conversation_id": cid })))
}

pub async fn get_messages(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(cid): Path<i64>
) -> Result<Json<Vec<Message>>> {
    let uid = user_id_from(&headers, &state).await?;
    let p: Option<i64> = sqlx::query_scalar(
        "SELECT 1 FROM participants WHERE conversation_id=? AND user_id=?"
    )
        .bind(cid).bind(uid)
        .fetch_optional(&state.pool).await?;
    if p.is_none() { return Err(AppError::Forbidden); }

    let rows = sqlx::query_as::<_, Message>(
        "SELECT id, conversation_id, author_id, body, created_at
         FROM messages WHERE conversation_id=?
         ORDER BY created_at ASC"
    )
        .bind(cid)
        .fetch_all(&state.pool)
        .await?;

    Ok(Json(rows))
}

pub async fn post_message(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(cid): Path<i64>,
    Json(req): Json<PostMsgReq>
) -> Result<Json<serde_json::Value>> {
    let uid = user_id_from(&headers, &state).await?;
    let p: Option<i64> = sqlx::query_scalar(
        "SELECT 1 FROM participants WHERE conversation_id=? AND user_id=?"
    )
        .bind(cid).bind(uid)
        .fetch_optional(&state.pool).await?;
    if p.is_none() { return Err(AppError::Forbidden); }

    let id = sqlx::query(
        "INSERT INTO messages(conversation_id, author_id, body, created_at)
         VALUES(?, ?, ?, strftime('%s','now'))"
    )
        .bind(cid).bind(uid).bind(&req.body)
        .execute(&state.pool).await?
        .last_insert_rowid();

    let evt = serde_json::json!({
        "type":"message",
        "cid": cid,
        "msg": { "id": id, "author_id": uid, "body": req.body }
    });
    let ch = ensure_channel(&state, cid).await;
    let _ = ch.send(evt);

    Ok(Json(serde_json::json!({ "id": id })))
}

pub async fn post_read(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(cid): Path<i64>,
    Json(req): Json<ReadReq>
) -> Result<Json<serde_json::Value>> {
    let uid = user_id_from(&headers, &state).await?;

    sqlx::query(
        "UPDATE participants SET last_read_msg=? WHERE conversation_id=? AND user_id=?"
    )
        .bind(req.msg_id).bind(cid).bind(uid)
        .execute(&state.pool).await?;

    let evt = serde_json::json!({ "type":"read", "cid": cid, "user": uid, "msg_id": req.msg_id });
    let ch = ensure_channel(&state, cid).await;
    let _ = ch.send(evt);

    Ok(Json(serde_json::json!({ "ok": true })))
}

