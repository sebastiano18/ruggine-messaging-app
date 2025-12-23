use anyhow::Result;
use reqwest::Client;
use uuid::Uuid;
use crate::models::{MessageDto, MessageResponse};

// Get messages con paginazione
pub async fn get_messages_paginated(
    base: &str,
    token: &str,
    cid: Uuid,
    limit: Option<i64>,
    before_sequence: Option<i64>,
) -> Result<Vec<MessageDto>> {
    let mut url = format!("{base}/api/conversations/{cid}/messages");

    let mut params = Vec::new();
    if let Some(limit) = limit {
        params.push(format!("limit={}", limit));
    }
    if let Some(before_seq) = before_sequence {
        params.push(format!("before_sequence={}", before_seq));
    }

    if !params.is_empty() {
        url.push_str("?");
        url.push_str(&params.join("&"));
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
                conversation_id: cid,
                author_username: msg.author_username,
                content: msg.content,
                created_at: msg.created_at,
                sequence_num: msg.sequence_num.map(|s| s as u64),
                client_msg_id: None,
                is_confirmed: Some(true),
            })
        })
        .collect();

    tracing::debug!(
        "Fetched {} messages for conversation {} (before_seq: {:?})",
        messages.len(),
        cid,
        before_sequence
    );

    Ok(messages)
}

