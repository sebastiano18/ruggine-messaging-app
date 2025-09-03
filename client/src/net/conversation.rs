use anyhow::Result;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use crate::state::ConversationOut;

#[derive(Serialize)]
pub struct GroupReq<'a> {
    pub name: &'a str
}

#[derive(Serialize)]
pub struct DmReq {
    pub user_id: i64
}

#[derive(Deserialize)]
pub struct CreatedId {
    pub id: i64
}

// Rimuovi ConversationOut da qui, ora è importata da state.rs

#[derive(Serialize)]
pub struct InviteReq {
    pub conversation_id: i64  // Cambiato da group_id
}

#[derive(Deserialize)]
pub struct InviteResp {
    pub token: String
}

#[derive(Serialize)]
pub struct JoinByTokenReq<'a> {
    pub token: &'a str
}

// Crea un nuovo gruppo
pub async fn create_group(base: &str, token: &str, name: &str) -> Result<i64> {
    let r = Client::new()
        .post(format!("{base}/api/conversations/groups"))
        .bearer_auth(token)
        .json(&GroupReq { name })
        .send().await?
        .error_for_status()?
        .json::<CreatedId>().await?;
    Ok(r.id)
}

// Crea o trova una DM
pub async fn create_dm(base: &str, token: &str, user_id: i64) -> Result<i64> {
    let r = Client::new()
        .post(format!("{base}/api/conversations/dm"))
        .bearer_auth(token)
        .json(&DmReq { user_id })
        .send().await?
        .error_for_status()?
        .json::<CreatedId>().await?;
    Ok(r.id)
}

// Ottieni le mie conversazioni
pub async fn get_conversations(base: &str, token: &str) -> Result<Vec<ConversationOut>> {
    let r = Client::new()
        .get(format!("{base}/api/conversations"))
        .bearer_auth(token)
        .send().await?
        .error_for_status()?
        .json::<Vec<ConversationOut>>().await?;
    Ok(r)
}

// Crea invito per una conversazione (solo gruppi)
pub async fn create_invite(base: &str, token: &str, conversation_id: i64) -> Result<String> {
    let r = Client::new()
        .post(format!("{base}/api/invites"))  // Endpoint da aggiungere
        .bearer_auth(token)
        .json(&InviteReq { conversation_id })
        .send().await?
        .error_for_status()?
        .json::<InviteResp>().await?;
    Ok(r.token)
}

// Join tramite token
pub async fn join_by_token(base: &str, token: &str, invite_token: &str) -> Result<i64> {
    let r = Client::new()
        .post(format!("{base}/api/invites/join"))  // Endpoint da aggiungere
        .bearer_auth(token)
        .json(&JoinByTokenReq { token: invite_token })
        .send().await?
        .error_for_status()?
        .json::<CreatedId>().await?;
    Ok(r.id)
}