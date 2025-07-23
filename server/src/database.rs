use sqlx::{SqlitePool, Row};
use anyhow::Result;
use uuid::Uuid;
use chrono::{DateTime, Utc};
use std::path::Path;

use crate::models::*;

pub async fn init_database(database_url: &str) -> Result<SqlitePool> {
    // Ensure database file exists by creating it if it doesn't
    let db_path = Path::new(database_url);
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    
    let pool = SqlitePool::connect(&format!("sqlite:{}?mode=rwc", database_url)).await?;
    
    // Create tables
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS users (
            id TEXT PRIMARY KEY,
            username TEXT UNIQUE NOT NULL,
            email TEXT UNIQUE NOT NULL,
            password_hash TEXT NOT NULL,
            created_at TEXT NOT NULL,
            last_seen TEXT NOT NULL,
            is_online BOOLEAN DEFAULT FALSE
        )
        "#,
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS groups (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            description TEXT,
            created_by TEXT NOT NULL,
            created_at TEXT NOT NULL,
            is_private BOOLEAN DEFAULT FALSE,
            FOREIGN KEY (created_by) REFERENCES users (id)
        )
        "#,
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS messages (
            id TEXT PRIMARY KEY,
            group_id TEXT NOT NULL,
            sender_id TEXT NOT NULL,
            content TEXT NOT NULL,
            message_type TEXT NOT NULL,
            sent_at TEXT NOT NULL,
            edited_at TEXT,
            FOREIGN KEY (group_id) REFERENCES groups (id),
            FOREIGN KEY (sender_id) REFERENCES users (id)
        )
        "#,
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS group_members (
            group_id TEXT NOT NULL,
            user_id TEXT NOT NULL,
            role TEXT NOT NULL,
            joined_at TEXT NOT NULL,
            PRIMARY KEY (group_id, user_id),
            FOREIGN KEY (group_id) REFERENCES groups (id),
            FOREIGN KEY (user_id) REFERENCES users (id)
        )
        "#,
    )
    .execute(&pool)
    .await?;

    // Tabella per gli inviti pendenti
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS group_invites (
            id TEXT PRIMARY KEY,
            group_id TEXT NOT NULL,
            inviter_id TEXT NOT NULL,
            invited_user_id TEXT NOT NULL,
            invited_at TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'pending',
            FOREIGN KEY (group_id) REFERENCES groups (id),
            FOREIGN KEY (inviter_id) REFERENCES users (id),
            FOREIGN KEY (invited_user_id) REFERENCES users (id),
            UNIQUE(group_id, invited_user_id)
        )
        "#,
    )
    .execute(&pool)
    .await?;

    Ok(pool)
}

// Funzione per pulire inviti pendenti scaduti o orfani
pub async fn cleanup_stale_invites(pool: &SqlitePool) -> Result<()> {
    // Remove all pending invites older than 24 hours or in inconsistent state
    sqlx::query(
        "DELETE FROM group_invites WHERE status = 'Pending'"
    )
    .execute(pool)
    .await?;
    
    Ok(())
}

pub async fn create_user(pool: &SqlitePool, user: &User) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO users (id, username, email, password_hash, created_at, last_seen, is_online)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
        "#,
    )
    .bind(user.id.to_string())
    .bind(&user.username)
    .bind(&user.email)
    .bind(&user.password_hash)
    .bind(user.created_at.to_rfc3339())
    .bind(user.last_seen.to_rfc3339())
    .bind(user.is_online)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn get_user_by_username(pool: &SqlitePool, username: &str) -> Result<Option<User>> {
    let row = sqlx::query(
        "SELECT id, username, email, password_hash, created_at, last_seen, is_online FROM users WHERE username = ?1"
    )
    .bind(username)
    .fetch_optional(pool)
    .await?;

    if let Some(row) = row {
        Ok(Some(User {
            id: Uuid::parse_str(&row.get::<String, _>("id"))?,
            username: row.get("username"),
            email: row.get("email"),
            password_hash: row.get("password_hash"),
            created_at: DateTime::parse_from_rfc3339(&row.get::<String, _>("created_at"))?.with_timezone(&Utc),
            last_seen: DateTime::parse_from_rfc3339(&row.get::<String, _>("last_seen"))?.with_timezone(&Utc),
            is_online: row.get("is_online"),
        }))
    } else {
        Ok(None)
    }
}

pub async fn update_user_online_status(pool: &SqlitePool, user_id: Uuid, is_online: bool) -> Result<()> {
    sqlx::query(
        "UPDATE users SET is_online = ?1, last_seen = ?2 WHERE id = ?3"
    )
    .bind(is_online)
    .bind(Utc::now().to_rfc3339())
    .bind(user_id.to_string())
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn create_group(pool: &SqlitePool, group: &Group) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO groups (id, name, description, created_by, created_at, is_private)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        "#,
    )
    .bind(group.id.to_string())
    .bind(&group.name)
    .bind(&group.description)
    .bind(group.created_by.to_string())
    .bind(group.created_at.to_rfc3339())
    .bind(group.is_private)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn add_group_member(pool: &SqlitePool, member: &GroupMember) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO group_members (group_id, user_id, role, joined_at)
        VALUES (?1, ?2, ?3, ?4)
        "#,
    )
    .bind(member.group_id.to_string())
    .bind(member.user_id.to_string())
    .bind(format!("{:?}", member.role))
    .bind(member.joined_at.to_rfc3339())
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn save_message(pool: &SqlitePool, message: &Message) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO messages (id, group_id, sender_id, content, message_type, sent_at, edited_at)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
        "#,
    )
    .bind(message.id.to_string())
    .bind(message.group_id.to_string())
    .bind(message.sender_id.to_string())
    .bind(&message.content)
    .bind(format!("{:?}", message.message_type))
    .bind(message.sent_at.to_rfc3339())
    .bind(message.edited_at.map(|dt| dt.to_rfc3339()))
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn get_group_messages(pool: &SqlitePool, group_id: Uuid, limit: u32, offset: u32) -> Result<Vec<Message>> {
    let rows = sqlx::query(
        r#"
        SELECT m.id, m.group_id, m.sender_id, m.content, m.message_type, m.sent_at, m.edited_at, u.username
        FROM messages m
        JOIN users u ON m.sender_id = u.id
        WHERE m.group_id = ?1 
        ORDER BY m.sent_at DESC 
        LIMIT ?2 OFFSET ?3
        "#,
    )
    .bind(group_id.to_string())
    .bind(limit as i64)
    .bind(offset as i64)
    .fetch_all(pool)
    .await?;

    let mut messages = Vec::new();
    for row in rows {
        messages.push(Message {
            id: Uuid::parse_str(&row.get::<String, _>("id"))?,
            group_id: Uuid::parse_str(&row.get::<String, _>("group_id"))?,
            sender_id: Uuid::parse_str(&row.get::<String, _>("sender_id"))?,
            content: row.get("content"),
            message_type: match row.get::<String, _>("message_type").as_str() {
                "Text" => MessageType::Text,
                "SystemNotification" => MessageType::SystemNotification,
                "UserJoined" => MessageType::UserJoined,
                "UserLeft" => MessageType::UserLeft,
                _ => MessageType::Text,
            },
            sent_at: DateTime::parse_from_rfc3339(&row.get::<String, _>("sent_at"))?.with_timezone(&Utc),
            edited_at: row.get::<Option<String>, _>("edited_at")
                .map(|s| DateTime::parse_from_rfc3339(&s).ok())
                .flatten()
                .map(|dt| dt.with_timezone(&Utc)),
            username: Some(row.get::<String, _>("username")),
        });
    }

    Ok(messages)
}

pub async fn get_user_by_id(pool: &SqlitePool, user_id: Uuid) -> Result<Option<User>> {
    let row = sqlx::query(
        "SELECT id, username, email, password_hash, created_at, last_seen, is_online FROM users WHERE id = ?1"
    )
    .bind(user_id.to_string())
    .fetch_optional(pool)
    .await?;

    if let Some(row) = row {
        Ok(Some(User {
            id: Uuid::parse_str(&row.get::<String, _>("id"))?,
            username: row.get("username"),
            email: row.get("email"),
            password_hash: row.get("password_hash"),
            created_at: DateTime::parse_from_rfc3339(&row.get::<String, _>("created_at"))?.with_timezone(&Utc),
            last_seen: DateTime::parse_from_rfc3339(&row.get::<String, _>("last_seen"))?.with_timezone(&Utc),
            is_online: row.get("is_online"),
        }))
    } else {
        Ok(None)
    }
}

pub async fn get_all_users(pool: &SqlitePool) -> Result<Vec<User>> {
    let rows = sqlx::query(
        "SELECT id, username, email, password_hash, created_at, last_seen, is_online FROM users"
    )
    .fetch_all(pool)
    .await?;

    let mut users = Vec::new();
    for row in rows {
        users.push(User {
            id: Uuid::parse_str(&row.get::<String, _>("id"))?,
            username: row.get("username"),
            email: row.get("email"),
            password_hash: row.get("password_hash"),
            created_at: DateTime::parse_from_rfc3339(&row.get::<String, _>("created_at"))?.with_timezone(&Utc),
            last_seen: DateTime::parse_from_rfc3339(&row.get::<String, _>("last_seen"))?.with_timezone(&Utc),
            is_online: row.get("is_online"),
        });
    }

    Ok(users)
}

pub async fn get_user_groups(pool: &SqlitePool, user_id: Uuid) -> Result<Vec<Group>> {
    let rows = sqlx::query(
        r#"
        SELECT g.id, g.name, g.description, g.created_by, g.created_at, g.is_private
        FROM groups g
        INNER JOIN group_members gm ON g.id = gm.group_id
        WHERE gm.user_id = ?1
        ORDER BY g.created_at DESC
        "#,
    )
    .bind(user_id.to_string())
    .fetch_all(pool)
    .await?;

    let mut groups = Vec::new();
    for row in rows {
        groups.push(Group {
            id: Uuid::parse_str(&row.get::<String, _>("id"))?,
            name: row.get("name"),
            description: row.get("description"),
            created_by: Uuid::parse_str(&row.get::<String, _>("created_by"))?,
            created_at: DateTime::parse_from_rfc3339(&row.get::<String, _>("created_at"))?.with_timezone(&Utc),
            is_private: row.get("is_private"),
        });
    }

    Ok(groups)
}

pub async fn get_group_by_id(pool: &SqlitePool, group_id: Uuid) -> Result<Option<Group>> {
    let row = sqlx::query(
        "SELECT id, name, description, created_by, created_at, is_private FROM groups WHERE id = ?1"
    )
    .bind(group_id.to_string())
    .fetch_optional(pool)
    .await?;

    if let Some(row) = row {
        Ok(Some(Group {
            id: Uuid::parse_str(&row.get::<String, _>("id"))?,
            name: row.get("name"),
            description: row.get("description"),
            created_by: Uuid::parse_str(&row.get::<String, _>("created_by"))?,
            created_at: DateTime::parse_from_rfc3339(&row.get::<String, _>("created_at"))?.with_timezone(&Utc),
            is_private: row.get("is_private"),
        }))
    } else {
        Ok(None)
    }
}

// Funzioni per la gestione degli inviti
pub async fn has_pending_invite(pool: &SqlitePool, group_id: Uuid, user_id: Uuid) -> Result<bool> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM group_invites WHERE group_id = ?1 AND invited_user_id = ?2 AND status = 'Pending'"
    )
    .bind(group_id.to_string())
    .bind(user_id.to_string())
    .fetch_one(pool)
    .await?;

    Ok(count > 0)
}

pub async fn create_group_invite(pool: &SqlitePool, invite: &GroupInvite) -> Result<()> {
    // First, clean up any existing invites for this user/group combination
    sqlx::query(
        "DELETE FROM group_invites WHERE group_id = ?1 AND invited_user_id = ?2"
    )
    .bind(invite.group_id.to_string())
    .bind(invite.invited_user_id.to_string())
    .execute(pool)
    .await?;

    // Now create the new invite
    sqlx::query(
        r#"
        INSERT INTO group_invites (id, group_id, inviter_id, invited_user_id, invited_at, status)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        "#,
    )
    .bind(invite.id.to_string())
    .bind(invite.group_id.to_string())
    .bind(invite.inviter_id.to_string())
    .bind(invite.invited_user_id.to_string())
    .bind(invite.invited_at.to_rfc3339())
    .bind(format!("{:?}", invite.status))
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn get_pending_invites_for_user(pool: &SqlitePool, user_id: Uuid) -> Result<Vec<GroupInvite>> {
    let rows = sqlx::query(
        r#"
        SELECT id, group_id, inviter_id, invited_user_id, invited_at, status
        FROM group_invites 
        WHERE invited_user_id = ?1 AND status = 'Pending'
        ORDER BY invited_at DESC
        "#,
    )
    .bind(user_id.to_string())
    .fetch_all(pool)
    .await?;

    let mut invites = Vec::new();
    for row in rows {
        invites.push(GroupInvite {
            id: Uuid::parse_str(&row.get::<String, _>("id"))?,
            group_id: Uuid::parse_str(&row.get::<String, _>("group_id"))?,
            inviter_id: Uuid::parse_str(&row.get::<String, _>("inviter_id"))?,
            invited_user_id: Uuid::parse_str(&row.get::<String, _>("invited_user_id"))?,
            invited_at: DateTime::parse_from_rfc3339(&row.get::<String, _>("invited_at"))?.with_timezone(&Utc),
            status: match row.get::<String, _>("status").as_str() {
                "Pending" => InviteStatus::Pending,
                "Accepted" => InviteStatus::Accepted,
                "Declined" => InviteStatus::Declined,
                "Expired" => InviteStatus::Expired,
                _ => InviteStatus::Pending,
            },
        });
    }

    Ok(invites)
}

pub async fn update_invite_status(pool: &SqlitePool, invite_id: Uuid, status: InviteStatus) -> Result<()> {
    sqlx::query(
        "UPDATE group_invites SET status = ?1 WHERE id = ?2"
    )
    .bind(format!("{:?}", status))
    .bind(invite_id.to_string())
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn get_invite_by_id(pool: &SqlitePool, invite_id: Uuid) -> Result<Option<GroupInvite>> {
    let row = sqlx::query(
        "SELECT id, group_id, inviter_id, invited_user_id, invited_at, status FROM group_invites WHERE id = ?1"
    )
    .bind(invite_id.to_string())
    .fetch_optional(pool)
    .await?;

    if let Some(row) = row {
        Ok(Some(GroupInvite {
            id: Uuid::parse_str(&row.get::<String, _>("id"))?,
            group_id: Uuid::parse_str(&row.get::<String, _>("group_id"))?,
            inviter_id: Uuid::parse_str(&row.get::<String, _>("inviter_id"))?,
            invited_user_id: Uuid::parse_str(&row.get::<String, _>("invited_user_id"))?,
            invited_at: DateTime::parse_from_rfc3339(&row.get::<String, _>("invited_at"))?.with_timezone(&Utc),
            status: match row.get::<String, _>("status").as_str() {
                "Pending" => InviteStatus::Pending,
                "Accepted" => InviteStatus::Accepted,
                "Declined" => InviteStatus::Declined,
                "Expired" => InviteStatus::Expired,
                _ => InviteStatus::Pending,
            },
        }))
    } else {
        Ok(None)
    }
}

pub async fn is_user_in_group(pool: &SqlitePool, user_id: Uuid, group_id: Uuid) -> Result<bool> {
    let result = sqlx::query(
        "SELECT COUNT(*) as count FROM group_members WHERE user_id = ?1 AND group_id = ?2"
    )
    .bind(user_id.to_string())
    .bind(group_id.to_string())
    .fetch_one(pool)
    .await?;

    Ok(result.get::<i64, _>("count") > 0)
}
