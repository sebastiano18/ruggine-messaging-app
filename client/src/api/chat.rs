use anyhow::Result;
use reqwest::Client;
use uuid::Uuid;
use crate::models::{MessageDto, MessageResponse, SendMsgReq};

pub async fn get_messages(base: &str, token: &str, cid: Uuid) -> Result<Vec<MessageDto>> {
    let response_messages = Client::new()
        .get(format!("{base}/api/conversations/{cid}/messages"))
        .bearer_auth(token)
        .send()
        .await?
        .error_for_status()?
        .json::<Vec<MessageResponse>>()
        .await?;

    // Converte MessageResponse in MessageDto preservando la sequence
    let messages: Vec<MessageDto> = response_messages
        .into_iter()
        .filter_map(|msg| {
            let id = Uuid::parse_str(&msg.id).ok()?;
            let author_id = Uuid::parse_str(&msg.author_id).ok()?;

            Some(MessageDto {
                id,
                author_id,
                conversation_id: cid,
                author_username: msg.author_username,
                content: msg.content,
                created_at: msg.created_at,
                sequence_num: msg.sequence_num.map(|s| s as u64), // Preserva sequence_num dal server
            })
        })
        .collect();

    Ok(messages)
}

pub async fn send_message(base: &str, token: &str, cid: Uuid, content: &str) -> Result<()> {
    Client::new()
        .post(format!("{base}/api/conversations/{cid}/messages"))
        .bearer_auth(token)
        .json(&SendMsgReq { content })
        .send()
        .await?
        .error_for_status()?;
    Ok(())
}

/// Fetch messaggi per fetch-on-subscribe con supporto sequence
pub async fn fetch_conversation_messages(
    base: &str,
    token: &str,
    conversation_id: Uuid,
    limit: Option<i64>
) -> Result<Vec<MessageDto>> {
    let mut url = format!("{base}/api/conversations/{conversation_id}/messages");

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

    // IMPORTANTE: Preserva la sequence dal server
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
                sequence_num: msg.sequence_num.map(|s| s as u64), // CORRETTO: Preserva la sequence
            })
        })
        .collect();

    // Log per debug
    let sequences: Vec<u64> = messages
        .iter()
        .filter_map(|m| m.sequence_num)
        .collect();

    if !sequences.is_empty() {
        tracing::debug!(
            "Fetched {} messages for conversation {} with sequences: {:?}",
            messages.len(),
            conversation_id,
            sequences
        );
    }

    Ok(messages)
}

/// Recupera messaggi con un range di sequence specifico
pub async fn fetch_messages_by_sequence_range(
    base: &str,
    token: &str,
    conversation_id: Uuid,
    from_sequence: u64,
    to_sequence: Option<u64>,
    limit: Option<i64>
) -> Result<Vec<MessageDto>> {
    let mut url = format!(
        "{base}/api/conversations/{conversation_id}/messages?from_sequence={}",
        from_sequence
    );

    if let Some(to_seq) = to_sequence {
        url.push_str(&format!("&to_sequence={}", to_seq));
    }

    if let Some(limit) = limit {
        url.push_str(&format!("&limit={}", limit));
    }

    let response_messages = Client::new()
        .get(&url)
        .bearer_auth(token)
        .send()
        .await?
        .error_for_status()?
        .json::<Vec<MessageResponse>>()
        .await?;

    let messages: Vec<MessageDto> = response_messages
        .into_iter()
        .filter_map(|msg| {
            let id = Uuid::parse_str(&msg.id).ok()?;
            let author_id = Uuid::parse_str(&msg.author_id).ok()?;

            Some(MessageDto {
                id,
                author_id,
                conversation_id,
                author_username: msg.author_username,
                content: msg.content,
                created_at: msg.created_at,
                sequence_num: msg.sequence_num.map(|s| s as u64),
            })
        })
        .collect();

    tracing::info!(
        "Fetched {} messages for sequence range {}-{:?}",
        messages.len(),
        from_sequence,
        to_sequence
    );

    Ok(messages)
}