use anyhow::Result;
use reqwest::Client;
use serde::Serialize;
use crate::models::{RegisterReq, LoginResp};

pub async fn register(base:&str, u:&str, p:&str) -> Result<()> {
    Client::new()
        .post(format!("{base}/api/users/register"))
        .json(&RegisterReq{username:u, password:p})
        .send().await?
        .error_for_status()?;
    Ok(())
}

pub async fn login(base:&str, u:&str, p:&str) -> Result<String> {
    let r = Client::new()
        .post(format!("{base}/api/login"))
        .json(&RegisterReq{username:u, password:p})
        .send().await?;
    Ok(r.error_for_status()?.json::<LoginResp>().await?.token)
}

#[derive(Serialize)]
struct LogoutReq<'a> {
    token: &'a str,
}

pub async fn logout(base: &str, token: &str) -> Result<()> {
    Client::new()
        .post(format!("{base}/api/logout"))
        .json(&LogoutReq { token })
        .send()
        .await?
        .error_for_status()?; // Se errore HTTP, ritorna Err
    Ok(())
}
