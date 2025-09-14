use anyhow::Result;
use reqwest::Client;
use uuid::Uuid;
use crate::models::{MessageDto, MessageResponse, SendMsgReq};

pub async fn get_messages(base:&str, token:&str, cid: Uuid) -> Result<Vec<MessageDto>> {
    Ok(Client::new()
        .get(format!("{base}/api/conversations/{cid}/messages"))
        .bearer_auth(token)
        .send().await?
        .error_for_status()?
        .json::<Vec<MessageDto>>().await?)
}

pub async fn send_message(base:&str, token:&str, cid: Uuid, content:&str) -> Result<()> {
    Client::new()
        .post(format!("{base}/api/conversations/{cid}/messages"))
        .bearer_auth(token)
        .json(&SendMsgReq{ content })
        .send().await?
        .error_for_status()?;
    Ok(())
}

/// NUOVO: Fetch messaggi per fetch-on-subscribe
pub async fn fetch_conversation_messages(
    base: &str,
    token: &str,
    conversation_id: Uuid,
    limit: Option<i64>
) -> Result<Vec<MessageDto>> {
    let mut url = format!("{base}/api/conversations/{conversation_id}/messages/fetch");

    if let Some(limit) = limit {
        url.push_str(&format!("?limit={}", limit));
    }

    let response_messages = Client::new()
        .get(&url)
        .bearer_auth(token)
        .send()
        .await?
        .error_for_status()?
        .json::<Vec<MessageResponse>>()
        .await?;

    // Converte MessageResponse in MessageDto
    let messages: Vec<MessageDto> = response_messages
        .into_iter()
        .filter_map(|msg| {
            // Parse UUID con gestione errori
            let id = Uuid::parse_str(&msg.id).ok()?;
            let author_id = Uuid::parse_str(&msg.author_id).ok()?;

            Some(MessageDto {
                id,
                author_id,
                conversation_id,
                author_username: msg.author_username,
                content: msg.content,
                created_at: msg.created_at,
            })
        })
        .collect();

    Ok(messages)
}
