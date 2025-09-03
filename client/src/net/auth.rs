use anyhow::Result;
use reqwest::Client;
use serde::{Serialize, Deserialize};

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

#[derive(Deserialize)]
pub struct LoginResp {
    pub token: String,
    pub user_id: i64,
    pub username: String,
}

pub async fn register(base: &str, u: &str, p: &str) -> Result<()> {
    Client::new()
        .post(format!("{base}/api/users/register"))
        .json(&RegisterReq { username: u, password: p })
        .send().await?
        .error_for_status()?;
    Ok(())
}

// Aggiornato per restituire LoginResp completa
pub async fn login(base: &str, u: &str, p: &str) -> Result<LoginResp> {
    let r = Client::new()
        .post(format!("{base}/api/users/login"))
        .json(&LoginReq { username: u, password: p })
        .send().await?
        .error_for_status()?
        .json::<LoginResp>().await?;
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