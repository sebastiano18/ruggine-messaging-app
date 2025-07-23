use bcrypt::{hash, verify, DEFAULT_COST};
use anyhow::Result;
use uuid::Uuid;
use chrono::Utc;
use sqlx::SqlitePool;

use crate::models::User;
use crate::database;

pub async fn register_user(
    pool: &SqlitePool,
    username: String,
    email: String,
    password: String,
) -> Result<User> {
    // Check if user already exists
    if let Some(_) = database::get_user_by_username(pool, &username).await? {
        return Err(anyhow::anyhow!("Username already exists"));
    }

    // Hash password
    let password_hash = hash(password.as_bytes(), DEFAULT_COST)?;

    // Create user
    let user = User {
        id: Uuid::new_v4(),
        username,
        email,
        password_hash,
        created_at: Utc::now(),
        last_seen: Utc::now(),
        is_online: false,
    };

    database::create_user(pool, &user).await?;

    Ok(user)
}

pub async fn authenticate_user(
    pool: &SqlitePool,
    username: String,
    password: String,
) -> Result<User> {
    let user = database::get_user_by_username(pool, &username).await?
        .ok_or_else(|| anyhow::anyhow!("User not found"))?;

    if verify(password.as_bytes(), &user.password_hash)? {
        // Update online status
        database::update_user_online_status(pool, user.id, true).await?;
        
        let mut authenticated_user = user;
        authenticated_user.is_online = true;
        authenticated_user.last_seen = Utc::now();
        
        Ok(authenticated_user)
    } else {
        Err(anyhow::anyhow!("Invalid password"))
    }
}

pub fn generate_session_token() -> String {
    Uuid::new_v4().to_string()
}
