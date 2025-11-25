use serde_json::{Value, json};
use sqlx::{Row, SqlitePool};
use std::collections::HashMap;
use tracing::{debug, info};
use uuid::Uuid;

/// Struttura per lo stato iniziale di una connessione WebSocket
pub struct InitialState {
    pub conversations: Vec<Value>,
    pub user_sequence: u64,
    pub pending_events: Vec<Value>,
    pub members_by_conversation: HashMap<String, Vec<Value>>,
}

/// Carica lo stato iniziale per un utente (conversazioni, ultima sequenza, membri)
pub async fn get_initial_state(
    pool: &SqlitePool,
    user_id: Uuid,
) -> Result<InitialState, Box<dyn std::error::Error + Send + Sync>> {
    let user_id_str = user_id.to_string();

    // Query ottimizzata che gestisce correttamente i titoli DM e include author_id
    let query = r#"
            SELECT
                c.id as conv_id,
                c.title as conv_title,
                c.kind as conv_kind,
                c.owner_id as conv_owner_id,
                c.created_at as conv_created_at,
                CASE
                    WHEN c.kind = 'dm' AND (c.title IS NULL OR c.title = '') THEN (
                        SELECT u.username
                        FROM participants p2
                        INNER JOIN users u ON p2.user_id = u.id
                        WHERE p2.conversation_id = c.id
                        AND p2.user_id != ?
                        LIMIT 1
                    )
                    WHEN c.title IS NULL OR c.title = '' THEN 'Untitled'
                    ELSE c.title
                END as display_title,
                (SELECT m.id
                 FROM messages m
                 WHERE m.conversation_id = c.id
                 ORDER BY m.created_at DESC LIMIT 1) as last_msg_id,
                (SELECT m.content
                 FROM messages m
                 WHERE m.conversation_id = c.id
                 ORDER BY m.created_at DESC LIMIT 1) as last_content,
                (SELECT m.author_id
                 FROM messages m
                 WHERE m.conversation_id = c.id
                 ORDER BY m.created_at DESC LIMIT 1) as last_author_id,
                (SELECT u.username
                 FROM messages m
                 INNER JOIN users u ON m.author_id = u.id
                 WHERE m.conversation_id = c.id
                 ORDER BY m.created_at DESC LIMIT 1) as last_author,
                (SELECT m.created_at
                 FROM messages m
                 WHERE m.conversation_id = c.id
                 ORDER BY m.created_at DESC LIMIT 1) as last_msg_time,
                (SELECT m.sequence_num
                 FROM messages m
                 WHERE m.conversation_id = c.id
                 ORDER BY m.created_at DESC LIMIT 1) as last_sequence,
                p.last_read_sequence,
                (SELECT COUNT(*)
                 FROM messages m2
                 WHERE m2.conversation_id = c.id) as message_count
            FROM conversations c
            INNER JOIN participants p ON c.id = p.conversation_id
            WHERE p.user_id = ?
            ORDER BY COALESCE(
                (SELECT m.created_at FROM messages m WHERE m.conversation_id = c.id
                 ORDER BY m.created_at DESC LIMIT 1),
                c.created_at
            ) DESC
        "#;

    let rows = sqlx::query(query)
        .bind(&user_id_str) // Primo parametro per il CASE WHEN
        .bind(&user_id_str) // Secondo parametro per il WHERE principale
        .fetch_all(pool)
        .await?;

    let mut conversations: Vec<Value> = Vec::new();

    for row in rows {
        let id: String = row.try_get("conv_id").unwrap_or_default();
        let kind: String = row.try_get("conv_kind").unwrap_or_default();
        let owner_id: String = row.try_get("conv_owner_id").unwrap_or_default();
        let created_at: i64 = row.try_get("conv_created_at").unwrap_or(0);
        let message_count: i64 = row.try_get("message_count").unwrap_or(0);
        let last_read_seq: i64 = row.try_get("last_read_sequence").unwrap_or(0);

        let display_title: String = row.try_get("display_title").unwrap_or_else(|_| {
            if kind == "dm" {
                "Direct Message".to_string()
            } else {
                "Untitled Group".to_string()
            }
        });

        let mut conv = json!({
            "id": id,
            "kind": kind,
            "title": display_title,
            "owner_id": owner_id,
            "created_at": created_at,
            "message_count": message_count,
            "last_read_sequence": last_read_seq
        });

        // Aggiungi ultimo messaggio SOLO se esiste veramente
        if let Ok(Some(content)) = row.try_get::<Option<String>, _>("last_content") {
            if !content.is_empty() {
                if let Ok(Some(author)) = row.try_get::<Option<String>, _>("last_author") {
                    let mut last_message = json!({
                        "content": content,
                        "author_username": author
                    });

                    // Includi l'UUID del messaggio per il controllo duplicati lato client
                    if let Ok(Some(msg_id)) = row.try_get::<Option<String>, _>("last_msg_id") {
                        last_message["id"] = json!(msg_id);
                    }

                    // Includi author_id nel last_message
                    if let Ok(Some(author_id)) = row.try_get::<Option<String>, _>("last_author_id")
                    {
                        last_message["author_id"] = json!(author_id);
                    }

                    if let Ok(Some(msg_time)) = row.try_get::<Option<i64>, _>("last_msg_time") {
                        last_message["created_at"] = json!(msg_time);
                    }

                    if let Ok(Some(seq)) = row.try_get::<Option<i64>, _>("last_sequence") {
                        last_message["sequence_num"] = json!(seq);
                        conv["last_message"] = last_message;
                    } else {
                        debug!(
                            "Message without sequence for conversation {}, not including in initial state",
                            id
                        );
                    }
                }
            }
        }

        conversations.push(conv);
    }

    // Recupera ultima user sequence
    let user_sequence: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(sequence_num), 0) FROM user_events WHERE user_id = ?",
    )
        .bind(&user_id_str)
        .fetch_one(pool)
        .await
        .unwrap_or(0);

    info!(
        "Loaded {} conversations for user {} (only including last_message where messages exist)",
        conversations.len(),
        user_id_str
    );

    // Carica i membri per tutti i gruppi dell'utente
    let mut members_by_conversation = HashMap::new();

    // Query per ottenere i membri di tutti i gruppi dell'utente
    // Ordinati: owner prima, poi alfabeticamente per username
    let members_query = r#"
        SELECT
            p.conversation_id,
            p.user_id,
            u.username,
            p.role
        FROM participants p
        INNER JOIN users u ON p.user_id = u.id
        WHERE p.conversation_id IN (
            SELECT DISTINCT c.id
            FROM conversations c
            INNER JOIN participants p2 ON c.id = p2.conversation_id
            WHERE c.kind = 'group' AND p2.user_id = ?
        )
        ORDER BY p.conversation_id,
                 CASE WHEN LOWER(p.role) = 'owner' THEN 0 ELSE 1 END,
                 LOWER(u.username) ASC
    "#;

    let member_rows = sqlx::query(members_query)
        .bind(&user_id_str)
        .fetch_all(pool)
        .await?;

    let total_members = member_rows.len();

    for row in member_rows {
        let conv_id: String = row.try_get("conversation_id").unwrap_or_default();
        let member_user_id: String = row.try_get("user_id").unwrap_or_default();
        let username: String = row.try_get("username").unwrap_or_default();
        let role: String = row.try_get("role").unwrap_or_default();

        let member = json!({
            "user_id": member_user_id,
            "username": username,
            "role": role
        });

        members_by_conversation
            .entry(conv_id)
            .or_insert_with(Vec::new)
            .push(member);
    }

    info!(
        "Loaded members for {} groups (total {} members)",
        members_by_conversation.len(),
        total_members
    );

    Ok(InitialState {
        conversations,
        user_sequence: user_sequence as u64,
        pending_events: Vec::new(),
        members_by_conversation,
    })
}