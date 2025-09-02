use anyhow::Result;
use reqwest::Client;
use crate::models::{MessageDto, SendMsgReq};

pub async fn get_messages(base:&str, token:&str, cid:i64) -> Result<Vec<MessageDto>> {
    Ok(Client::new()
        .get(format!("{base}/api/conversations/{cid}/messages"))
        .bearer_auth(token)
        .send().await?
        .error_for_status()?
        .json::<Vec<MessageDto>>().await?)
}

pub async fn send_message(base:&str, token:&str, cid:i64, body:&str) -> Result<()> {
    Client::new()
        .post(format!("{base}/api/conversations/{cid}/messages"))
        .bearer_auth(token)
        .json(&SendMsgReq{ body })
        .send().await?
        .error_for_status()?;
    Ok(())
}
