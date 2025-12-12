use crate::models::{ConversationDto, MessageDto, ParticipantInfo, ConversationSummary};
use anyhow::Result;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Serialize)]
pub struct GroupReq<'a> {
    pub name: &'a str,
}

#[derive(Serialize)]
pub struct DmReq {
    pub user_username: String,
}

#[derive(Deserialize)]
pub struct CreatedId {
    pub id: Uuid,
}

#[derive(Serialize)]
pub struct InviteReq {
    pub conversation_id: Uuid,
}

#[derive(Deserialize)]
pub struct InviteResp {
    pub token: String,
}

#[derive(Serialize)]
pub struct JoinByTokenReq<'a> {
    pub token: &'a str,
}

// === Response types ===

#[derive(Deserialize)]
pub struct ConversationWithMessages {
    pub conversation: ConversationDto,
    pub messages: Vec<MessageDto>,
    #[serde(default)]
    pub members: Vec<ParticipantInfo>,
}

// NUOVO: Response paginata
#[derive(Deserialize, Debug, Clone)]
pub struct PaginatedConversationsResponse {
    pub conversations: Vec<ConversationSummary>,
    pub next_cursor: Option<i64>,  // Timestamp per prossima pagina
    pub has_more: bool,
}

// === API client ===

// Get conversation with its messages in a single API call
pub async fn get_conversation_with_messages(
    base: &str,
    token: &str,
    conversation_id: Uuid
) -> Result<ConversationWithMessages> {
    let r = Client::new()
        .get(format!("{base}/api/conversations/{conversation_id}/with-messages"))
        .bearer_auth(token)
        .send()
        .await?
        .error_for_status()?
        .json::<ConversationWithMessages>()
        .await?;
    Ok(r)
}

// AGGIORNATO: Ottieni le mie conversazioni (deprecated, usa get_conversations_paginated)
pub async fn get_conversations(base: &str, token: &str) -> Result<Vec<ConversationDto>> {
    let r = Client::new()
        .get(format!("{base}/api/conversations"))
        .bearer_auth(token)
        .send()
        .await?
        .error_for_status()?
        .json::<Vec<ConversationDto>>()
        .await?;
    Ok(r)
}

// ✨ NUOVO: Ottieni conversazioni con pagination
/// Ottieni conversazioni paginato (20 alla volta)
///
/// # Arguments
/// * `base` - Base URL del server
/// * `token` - JWT token
/// * `before` - Timestamp cursor (last_activity della conversazione più vecchia caricata)
/// * `limit` - Numero di conversazioni da caricare (default 20)
///
/// # Example
/// ```
/// // Prima pagina
/// let first_page = get_conversations_paginated(base, token, None, 20).await?;
///
/// // Pagina successiva
/// if first_page.has_more {
///     let next_page = get_conversations_paginated(
///         base, 
///         token, 
///         first_page.next_cursor,  // Usa il cursor della risposta precedente
///         20
///     ).await?;
/// }
/// ```
pub async fn get_conversations_paginated(
    base: &str,
    token: &str,
    before: Option<i64>,  // Timestamp cursor
    limit: i32,
) -> Result<PaginatedConversationsResponse> {
    let mut url = format!("{base}/api/conversations?limit={limit}");

    if let Some(cursor) = before {
        url.push_str(&format!("&before={cursor}"));
    }

    let r = Client::new()
        .get(&url)
        .bearer_auth(token)
        .send()
        .await?
        .error_for_status()?
        .json::<PaginatedConversationsResponse>()
        .await?;

    Ok(r)
}

// ✨ AGGIORNATO: Ottieni singola conversazione con summary (ultimo messaggio + membri)
/// Ottieni una singola conversazione con metadata, ultimo messaggio e membri (se gruppo)
/// Utile per caricare conversazioni on-demand quando arriva un messaggio in una chat non ancora caricata
pub async fn get_conversation(
    base: &str,
    token: &str,
    conversation_id: Uuid
) -> Result<ConversationSummary> {
    let r = Client::new()
        .get(format!("{base}/api/conversations/{conversation_id}"))
        .bearer_auth(token)
        .send()
        .await?
        .error_for_status()?
        .json::<ConversationSummary>()
        .await?;
    Ok(r)
}

// Crea invito per una conversazione (solo gruppi)
pub async fn create_invite(base: &str, token: &str, conversation_id: Uuid) -> Result<String> {
    let r = Client::new()
        .post(format!("{base}/api/invites"))
        .bearer_auth(token)
        .json(&InviteReq { conversation_id })
        .send()
        .await?
        .error_for_status()?
        .json::<InviteResp>()
        .await?;
    Ok(r.token)
}

// Join tramite token
pub async fn join_by_token(base: &str, token: &str, invite_token: &str) -> Result<Uuid> {
    let r = Client::new()
        .post(format!("{base}/api/invites/join"))
        .bearer_auth(token)
        .json(&JoinByTokenReq {
            token: invite_token,
        })
        .send()
        .await?
        .error_for_status()?
        .json::<CreatedId>()
        .await?;
    Ok(r.id)
}

// Get members of a conversation
pub async fn get_conversation_members(base: &str, token: &str, conversation_id: Uuid) -> Result<Vec<ParticipantInfo>> {
    let r = Client::new()
        .get(format!("{base}/api/conversations/{conversation_id}/members"))
        .bearer_auth(token)
        .send()
        .await?
        .error_for_status()?
        .json::<Vec<ParticipantInfo>>()
        .await?;
    Ok(r)
}

// Kick a member from a conversation
pub async fn kick_member(base: &str, token: &str, conversation_id: Uuid, user_id: Uuid) -> Result<()> {
    Client::new()
        .delete(format!("{base}/api/conversations/{conversation_id}/members/{user_id}"))
        .bearer_auth(token)
        .send()
        .await?
        .error_for_status()?;
    Ok(())
}