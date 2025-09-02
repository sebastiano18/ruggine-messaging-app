use anyhow::Result;
use reqwest::Client;
use serde_json::json;
use crate::models::{GroupReq, InviteReq, InviteResp, JoinByTokenReq};

pub async fn create_group(base:&str, token:&str, name:&str) -> Result<i64> {
    let r = Client::new()
        .post(format!("{base}/api/groups"))
        .bearer_auth(token)
        .json(&GroupReq{ name })
        .send().await?
        .error_for_status()?
        .json::<serde_json::Value>().await?;
    Ok(r["conversation_id"].as_i64().unwrap_or(0))
}

pub async fn create_invite(base:&str, token:&str, group_id:i64) -> Result<String> {
    let r = Client::new()
        .post(format!("{base}/api/groups/invite"))
        .bearer_auth(token)
        .json(&InviteReq{ group_id })
        .send().await?
        .error_for_status()?
        .json::<InviteResp>().await?;
    Ok(r.token)
}

pub async fn join_by_token(base:&str, token:&str, invite_token:&str) -> Result<i64> {
    let r = Client::new()
        .post(format!("{base}/api/groups/join"))
        .bearer_auth(token)
        .json(&JoinByTokenReq{ token: invite_token })
        .send().await?
        .error_for_status()?
        .json::<serde_json::Value>().await?;
    Ok(r["conversation_id"].as_i64().unwrap_or(0))
}
