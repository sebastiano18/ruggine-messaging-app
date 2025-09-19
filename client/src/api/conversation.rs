use crate::models::{ConversationDto, MessageDto};
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

// === API client ===

#[derive(Deserialize)]
pub struct ConversationWithMessages {
    pub conversation: ConversationDto,
    pub messages: Vec<MessageDto>,
}

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

// Crea un nuovo gruppo
pub async fn create_group(base: &str, token: &str, name: &str) -> Result<Uuid> {
    let r = Client::new()
        .post(format!("{base}/api/conversations/groups"))
        .bearer_auth(token)
        .json(&GroupReq { name })
        .send()
        .await?
        .error_for_status()?
        .json::<CreatedId>()
        .await?;
    Ok(r.id)
}

// Crea o trova una DM
pub async fn create_dm(base: &str, token: &str, user_username: String) -> Result<Uuid> {
    let r = Client::new()
        .post(format!("{base}/api/conversations/dm"))
        .bearer_auth(token)
        .json(&DmReq { user_username })
        .send()
        .await?
        .error_for_status()?
        .json::<CreatedId>()
        .await?;
    Ok(r.id)
}

// Ottieni le mie conversazioni (refresh completo)
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

// NUOVO: Ottieni singola conversazione (fetch mirata)
pub async fn get_single_conversation(base: &str, token: &str, conversation_id: Uuid) -> Result<ConversationDto> {
    let r = Client::new()
        .get(format!("{base}/api/conversations/{conversation_id}"))
        .bearer_auth(token)
        .send()
        .await?
        .error_for_status()?
        .json::<ConversationDto>()
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

pub async fn delete_conversation(base: &str, token: &str, conversation_id: Uuid) -> Result<()> {
    Client::new()
        .delete(format!("{base}/api/conversations/{conversation_id}"))
        .bearer_auth(token)
        .send()
        .await?
        .error_for_status()?;
    Ok(())
}