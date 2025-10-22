use anyhow::Result;
use reqwest::Client;
use serde::{Serialize, Deserialize};
use uuid::Uuid;
use crate::models::LoginResp;

#[derive(Serialize)]
pub struct RegisterReq<'a> {
    pub username: &'a str,
    pub password: &'a str,
}

#[derive(Serialize)]
pub struct LoginReq<'a> {
    pub username: &'a str,
    pub password: &'a str,
}

pub async fn register(base: &str, u: &str, p: &str) -> Result<()> {
    Client::new()
        .post(format!("{base}/api/users/register"))
        .json(&RegisterReq { username: u, password: p })
        .send()
        .await?
        .error_for_status()?;
    Ok(())
}

// Aggiornato per restituire LoginResp con UUID
pub async fn login(base: &str, u: &str, p: &str) -> Result<LoginResp> {
    let r = Client::new()
        .post(format!("{base}/api/users/login"))
        .json(&LoginReq { username: u, password: p })
        .send()
        .await?
        .error_for_status()?
        .json::<LoginResp>()
        .await?;
    Ok(r)
}

#[derive(Serialize)]
struct LogoutReq<'a> {
    token: &'a str,
}

pub async fn logout(base: &str, token: &str) -> Result<()> {
    Client::new()
        .post(format!("{base}/api/users/logout"))
        .json(&LogoutReq { token })
        .send()
        .await?
        .error_for_status()?;
    Ok(())
}

pub async fn delete_account(base: &str, token: &str) -> Result<()> {
    Client::new()
        .delete(format!("{base}/api/users/deleteMe"))
        .bearer_auth(token)
        .send()
        .await?
        .error_for_status()?;
    Ok(())
}