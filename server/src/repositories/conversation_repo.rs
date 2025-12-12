use crate::error::Result;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;
use crate::error::AppError;
use crate::models::{Message, ConversationWithLastMessage};

#[derive(Debug, Clone)]
pub struct ConversationRepo;

impl ConversationRepo {

    
    pub async fn get_conversations(
        pool: &sqlx::SqlitePool,
        user_id: Uuid,
        limit: i32,
        before: Option<i64>
    ) -> Result<Vec<ConversationWithLastMessage>> {
        let user_id_str = user_id.to_string();

        let query = if let Some(_) = before {
            r#"
        SELECT 
            c.id,
            c.kind,
            CASE
                WHEN c.kind = 'group' THEN COALESCE(c.title, 'Gruppo')
                WHEN c.kind = 'dm' THEN (
                    COALESCE(
                        (SELECT u.username
                         FROM participants p2
                         JOIN users u ON p2.user_id = u.id
                         WHERE p2.conversation_id = c.id AND p2.user_id != ?
                         LIMIT 1),
                        (SELECT u.username
                         FROM messages m
                         JOIN users u ON m.author_id = u.id
                         WHERE m.conversation_id = c.id AND m.author_id != ?
                         ORDER BY m.created_at DESC
                         LIMIT 1),
                        'Utente sconosciuto'
                    )
                )
                ELSE 'Unknown'
            END AS title,
            c.owner_id,
            c.created_at,
            p.last_read_sequence,
            COALESCE(ms.last_updated, c.created_at) as last_activity,
            COALESCE(ms.current_sequence, 0) as last_msg_seq,
            last_m.id as last_msg_id,
            last_m.author_id as last_msg_author_id,
            last_u.username as last_msg_author_username,
            last_m.content as last_msg_content,
            last_m.created_at as last_msg_timestamp,
            last_m.sequence_num as last_msg_sequence
        FROM conversations c
        JOIN participants p ON c.id = p.conversation_id
        LEFT JOIN message_sequences ms ON c.id = ms.conversation_id
        LEFT JOIN messages last_m ON c.id = last_m.conversation_id 
            AND last_m.sequence_num = ms.current_sequence
        LEFT JOIN users last_u ON last_m.author_id = last_u.id
        WHERE p.user_id = ?
          AND COALESCE(ms.last_updated, c.created_at) < ?
        ORDER BY last_activity DESC
        LIMIT ?
        "#
        } else {
            r#"
        SELECT 
            c.id,
            c.kind,
            CASE
                WHEN c.kind = 'group' THEN COALESCE(c.title, 'Gruppo')
                WHEN c.kind = 'dm' THEN (
                    COALESCE(
                        (SELECT u.username
                         FROM participants p2
                         JOIN users u ON p2.user_id = u.id
                         WHERE p2.conversation_id = c.id AND p2.user_id != ?
                         LIMIT 1),
                        (SELECT u.username
                         FROM messages m
                         JOIN users u ON m.author_id = u.id
                         WHERE m.conversation_id = c.id AND m.author_id != ?
                         ORDER BY m.created_at DESC
                         LIMIT 1),
                        'Utente sconosciuto'
                    )
                )
                ELSE 'Unknown'
            END AS title,
            c.owner_id,
            c.created_at,
            p.last_read_sequence,
            COALESCE(ms.last_updated, c.created_at) as last_activity,
            COALESCE(ms.current_sequence, 0) as last_msg_seq,
            last_m.id as last_msg_id,
            last_m.author_id as last_msg_author_id,
            last_u.username as last_msg_author_username,
            last_m.content as last_msg_content,
            last_m.created_at as last_msg_timestamp,
            last_m.sequence_num as last_msg_sequence
        FROM conversations c
        JOIN participants p ON c.id = p.conversation_id
        LEFT JOIN message_sequences ms ON c.id = ms.conversation_id
        LEFT JOIN messages last_m ON c.id = last_m.conversation_id 
            AND last_m.sequence_num = ms.current_sequence
        LEFT JOIN users last_u ON last_m.author_id = last_u.id
        WHERE p.user_id = ?
        ORDER BY last_activity DESC
        LIMIT ?
        "#
        };

        let rows = if let Some(before_ts) = before {
            sqlx::query(query)
                .bind(&user_id_str)
                .bind(&user_id_str)
                .bind(&user_id_str)
                .bind(before_ts)
                .bind(limit)
                .fetch_all(pool)
                .await?
        } else {
            sqlx::query(query)
                .bind(&user_id_str)
                .bind(&user_id_str)
                .bind(&user_id_str)
                .bind(limit)
                .fetch_all(pool)
                .await?
        };

        let mut conversations = Vec::new();
        let mut conversation_ids = Vec::new();

        for row in rows {
            let id_str: String = row.get("id");
            let conv_id = Uuid::parse_str(&id_str).unwrap();
            conversation_ids.push(id_str.clone());

            let owner_id_str: String = row.get("owner_id");

            conversations.push(ConversationWithLastMessage {
                id: conv_id,
                kind: row.get("kind"),
                title: row.get("title"),
                owner_id: Uuid::parse_str(&owner_id_str).unwrap(),
                created_at: row.get("created_at"),
                last_read_sequence: row.get("last_read_sequence"),
                last_activity: row.get("last_activity"),
                last_msg_seq: row.get("last_msg_seq"),
                last_msg_id: row.get::<Option<String>, _>("last_msg_id")
                    .and_then(|s| Uuid::parse_str(&s).ok()),
                last_msg_author_id: row.get::<Option<String>, _>("last_msg_author_id")
                    .and_then(|s| Uuid::parse_str(&s).ok()),
                last_msg_author_username: row.get("last_msg_author_username"),
                last_msg_content: row.get("last_msg_content"),
                last_msg_timestamp: row.get("last_msg_timestamp"),
                last_msg_sequence: row.get("last_msg_sequence"),
                members: None,
            });
        }

        if conversations.is_empty() {
            return Ok(conversations);
        }

        // Batch load membri SOLO per gruppi
        let placeholders = conversation_ids
            .iter()
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(", ");

        let members_query = format!(
            r#"
        SELECT
            p.conversation_id,
            p.user_id,
            u.username,
            p.role
        FROM participants p
        INNER JOIN users u ON p.user_id = u.id
        INNER JOIN conversations c ON p.conversation_id = c.id
        WHERE p.conversation_id IN ({})
          AND c.kind = 'group'
        ORDER BY p.conversation_id, 
                 CASE WHEN LOWER(p.role) = 'owner' THEN 0 ELSE 1 END,
                 u.username
        "#,
            placeholders
        );

        let mut query_builder = sqlx::query(&members_query);
        for id in &conversation_ids {
            query_builder = query_builder.bind(id);
        }

        let member_rows = query_builder.fetch_all(pool).await?;

        let mut members_by_conv: std::collections::HashMap<Uuid, Vec<crate::models::ParticipantInfo>> =
            std::collections::HashMap::new();

        for row in member_rows {
            let conv_id_str: String = row.get("conversation_id");
            let conv_id = Uuid::parse_str(&conv_id_str).unwrap();

            let user_id_str: String = row.get("user_id");
            let user_id = Uuid::parse_str(&user_id_str).unwrap();

            members_by_conv
                .entry(conv_id)
                .or_insert_with(Vec::new)
                .push(crate::models::ParticipantInfo {
                    user_id,
                    username: row.get("username"),
                    role: row.get("role"),
                });
        }

        for conv in &mut conversations {
            if let Some(members) = members_by_conv.remove(&conv.id) {
                conv.members = Some(members);
            }
        }

        Ok(conversations)
    }

    pub async fn get_single_conversation(
        pool: &sqlx::SqlitePool,
        conversation_id: Uuid,
        user_id: Uuid
    ) -> Result<Option<ConversationWithLastMessage>> {
        let user_id_str = user_id.to_string();
        let conv_id_str = conversation_id.to_string();

        let query = r#"
        SELECT 
            c.id,
            c.kind,
            CASE
                WHEN c.kind = 'group' THEN COALESCE(c.title, 'Gruppo')
                WHEN c.kind = 'dm' THEN (
                    COALESCE(
                        (SELECT u.username
                         FROM participants p2
                         JOIN users u ON p2.user_id = u.id
                         WHERE p2.conversation_id = c.id AND p2.user_id != ?
                         LIMIT 1),
                        'Utente sconosciuto'
                    )
                )
                ELSE 'Unknown'
            END AS title,
            c.owner_id,
            c.created_at,
            p.last_read_sequence,
            COALESCE(ms.last_updated, c.created_at) as last_activity,
            COALESCE(ms.current_sequence, 0) as last_msg_seq,
            last_m.id as last_msg_id,
            last_m.author_id as last_msg_author_id,
            last_u.username as last_msg_author_username,
            last_m.content as last_msg_content,
            last_m.created_at as last_msg_timestamp,
            last_m.sequence_num as last_msg_sequence
        FROM conversations c
        JOIN participants p ON c.id = p.conversation_id
        LEFT JOIN message_sequences ms ON c.id = ms.conversation_id
        LEFT JOIN messages last_m ON c.id = last_m.conversation_id 
            AND last_m.sequence_num = ms.current_sequence
        LEFT JOIN users last_u ON last_m.author_id = last_u.id
        WHERE c.id = ?
          AND p.user_id = ?
    "#;

        let row_opt = sqlx::query(query)
            .bind(&user_id_str)
            .bind(&conv_id_str)
            .bind(&user_id_str)
            .fetch_optional(pool)
            .await?;

        let Some(row) = row_opt else {
            return Ok(None);
        };

        let id_str: String = row.get("id");
        let conv_id = Uuid::parse_str(&id_str).unwrap();
        let owner_id_str: String = row.get("owner_id");
        let kind: String = row.get("kind");

        let mut conversation = ConversationWithLastMessage {
            id: conv_id,
            kind: kind.clone(),
            title: row.get("title"),
            owner_id: Uuid::parse_str(&owner_id_str).unwrap(),
            created_at: row.get("created_at"),
            last_read_sequence: row.get("last_read_sequence"),
            last_activity: row.get("last_activity"),
            last_msg_seq: row.get("last_msg_seq"),
            last_msg_id: row.get::<Option<String>, _>("last_msg_id")
                .and_then(|s| Uuid::parse_str(&s).ok()),
            last_msg_author_id: row.get::<Option<String>, _>("last_msg_author_id")
                .and_then(|s| Uuid::parse_str(&s).ok()),
            last_msg_author_username: row.get("last_msg_author_username"),
            last_msg_content: row.get("last_msg_content"),
            last_msg_timestamp: row.get("last_msg_timestamp"),
            last_msg_sequence: row.get("last_msg_sequence"),
            members: None,
        };

        // ✅ Popola members se è un gruppo
        if kind == "group" {
            let members_query = r#"
            SELECT
                p.user_id,
                u.username,
                p.role
            FROM participants p
            INNER JOIN users u ON p.user_id = u.id
            WHERE p.conversation_id = ?
            ORDER BY CASE WHEN LOWER(p.role) = 'owner' THEN 0 ELSE 1 END,
                     u.username
        "#;

            let member_rows = sqlx::query(members_query)
                .bind(&conv_id_str)
                .fetch_all(pool)
                .await?;

            let members: Vec<crate::models::ParticipantInfo> = member_rows
                .into_iter()
                .map(|row| {
                    let user_id_str: String = row.get("user_id");
                    crate::models::ParticipantInfo {
                        user_id: Uuid::parse_str(&user_id_str).unwrap(),
                        username: row.get("username"),
                        role: row.get("role"),
                    }
                })
                .collect();

            conversation.members = Some(members);
        }

        Ok(Some(conversation))
    }
    
    pub async fn get_conversation_kind(
        pool: &SqlitePool,
        conversation_id: Uuid,
    ) -> Result<Option<String>> {
        let row = sqlx::query("SELECT kind FROM conversations WHERE id = ?")
            .bind(conversation_id.to_string())
            .fetch_optional(pool)
            .await?;

        Ok(row.map(|r| r.get("kind")))
    }


    pub async fn create_group(pool: &SqlitePool, name: &str, owner_id: Uuid) -> Result<Uuid> {
        let conversation_id = Uuid::new_v4();
        let cid_str = conversation_id.to_string();
        let owner_id_str = owner_id.to_string();

        sqlx::query(
            "INSERT INTO conversations (id, kind, title, owner_id, created_at) VALUES (?, 'group', ?, ?, ?)"
        )
            .bind(&cid_str)
            .bind(name)
            .bind(&owner_id_str)
            .bind(chrono::Utc::now().timestamp())
            .execute(pool)
            .await?;

        sqlx::query("INSERT INTO participants (conversation_id, user_id, role) VALUES (?, ?, 'owner')")
            .bind(&cid_str)
            .bind(&owner_id_str)
            .execute(pool)
            .await?;

        Ok(conversation_id)
    }

    pub async fn create_dm(pool: &SqlitePool, user1_id: Uuid, user2_id: Uuid) -> Result<Uuid> {
        let u1_str = user1_id.to_string();
        let u2_str = user2_id.to_string();

        let existing: Option<String> = sqlx::query_scalar(
            r#"
            SELECT c.id
            FROM conversations c
            JOIN participants p1 ON c.id = p1.conversation_id
            JOIN participants p2 ON c.id = p2.conversation_id
            WHERE c.kind = 'dm'
              AND p1.user_id = ?
              AND p2.user_id = ?
            LIMIT 1
            "#,
        )
            .bind(&u1_str)
            .bind(&u2_str)
            .fetch_optional(pool)
            .await?;

        if let Some(cid_str) = existing {
            return Ok(Uuid::parse_str(&cid_str).unwrap());
        }

        let conversation_id = Uuid::new_v4();
        let cid_str = conversation_id.to_string();

        sqlx::query(
            "INSERT INTO conversations (id, kind, owner_id, created_at) VALUES (?, 'dm', ?, ?)"
        )
            .bind(&cid_str)
            .bind(&u1_str)
            .bind(chrono::Utc::now().timestamp())
            .execute(pool)
            .await?;

        sqlx::query("INSERT INTO participants (conversation_id, user_id, role) VALUES (?, ?, 'member')")
            .bind(&cid_str)
            .bind(&u1_str)
            .execute(pool)
            .await?;

        sqlx::query("INSERT INTO participants (conversation_id, user_id, role) VALUES (?, ?, 'member')")
            .bind(&cid_str)
            .bind(&u2_str)
            .execute(pool)
            .await?;

        Ok(conversation_id)
    }

    pub async fn add_member(
        pool: &SqlitePool,
        conversation_id: Uuid,
        member_id: Uuid,
    ) -> Result<()> {
        sqlx::query("INSERT INTO participants (conversation_id, user_id, role) VALUES (?, ?, 'member')")
            .bind(conversation_id.to_string())
            .bind(member_id.to_string())
            .execute(pool)
            .await?;
        Ok(())
    }

    pub async fn by_user(pool: &SqlitePool, user_id: Uuid) -> Result<Vec<(Uuid, String, String, Uuid, i64, i64, i64, i64)>> {
        let rows = sqlx::query(
            r#"
        SELECT
            c.id,
            c.kind,
            CASE
                WHEN c.kind = 'group' THEN c.title
                WHEN c.kind = 'dm' THEN (
                    COALESCE(
                        (SELECT u.username
                         FROM participants p2
                         JOIN users u ON p2.user_id = u.id
                         WHERE p2.conversation_id = c.id AND p2.user_id != ?
                         LIMIT 1),
                        (SELECT u.username
                         FROM messages m
                         JOIN users u ON m.author_id = u.id
                         WHERE m.conversation_id = c.id AND m.author_id != ?
                         ORDER BY m.created_at DESC
                         LIMIT 1)
                    )
                )
                ELSE 'Unknown'
            END AS display_title,
            c.owner_id,
            c.created_at,
            COALESCE(p.last_read_sequence, 0) AS last_read_sequence,
            COALESCE(
                (SELECT MAX(m.created_at) 
                 FROM messages m 
                 WHERE m.conversation_id = c.id),
                c.created_at
            ) AS last_activity,
            COALESCE(
                (SELECT current_sequence 
                 FROM message_sequences 
                 WHERE conversation_id = c.id),
                0
            ) AS last_msg_seq
        FROM conversations c
        LEFT JOIN participants p ON c.id = p.conversation_id AND p.user_id = ?
        WHERE p.user_id = ?
           OR (c.kind = 'dm' AND EXISTS(
                SELECT 1 FROM messages m 
                WHERE m.conversation_id = c.id AND m.author_id = ?
           ))
        ORDER BY last_activity DESC
        "#,
        )
            .bind(user_id.to_string())
            .bind(user_id.to_string())
            .bind(user_id.to_string())
            .bind(user_id.to_string())
            .bind(user_id.to_string())
            .fetch_all(pool)
            .await?;

        Ok(rows
            .into_iter()
            .map(|r| {
                let id_str: String = r.get("id");
                let kind: String = r.get("kind");
                let display_title: Option<String> = r.get("display_title");
                let owner_id_str: String = r.get("owner_id");
                let created_at: i64 = r.get("created_at");
                let last_read_sequence: i64 = r.get("last_read_sequence");
                let last_activity: i64 = r.get("last_activity");
                let last_msg_seq: i64 = r.get("last_msg_seq");
                (
                    Uuid::parse_str(&id_str).expect("DB must store valid UUIDs"),
                    kind,
                    display_title.unwrap_or_else(|| "Unknown".to_string()),
                    Uuid::parse_str(&owner_id_str).expect("DB must store valid UUIDs"),
                    created_at,
                    last_read_sequence,
                    last_activity,
                    last_msg_seq,
                )
            })
            .collect())
    }

    pub async fn is_owner(pool: &SqlitePool, conversation_id: Uuid, user_id: Uuid) -> Result<bool> {
        let row = sqlx::query("SELECT 1 FROM conversations WHERE id = ? AND owner_id = ?")
            .bind(conversation_id.to_string())
            .bind(user_id.to_string())
            .fetch_optional(pool)
            .await?;
        Ok(row.is_some())
    }

    pub async fn is_participant(
        pool: &SqlitePool,
        conversation_id: Uuid,
        user_id: Uuid,
    ) -> Result<bool> {
        let row =
            sqlx::query("SELECT 1 FROM participants WHERE conversation_id = ? AND user_id = ?")
                .bind(conversation_id.to_string())
                .bind(user_id.to_string())
                .fetch_optional(pool)
                .await?;
        Ok(row.is_some())
    }

    pub async fn delete_conversation(pool: &SqlitePool, conversation_id: Uuid) -> Result<()> {
        sqlx::query("DELETE FROM conversations WHERE id = ?")
            .bind(conversation_id.to_string())
            .execute(pool)
            .await?;
        Ok(())
    }

    pub async fn user_has_dm_access(pool: &SqlitePool, conversation_id: Uuid, user_id: Uuid) -> Result<bool> {
        let cid = conversation_id.to_string();
        let uid = user_id.to_string();

        let participant_exists: Option<i64> = sqlx::query_scalar(
            "SELECT 1 FROM participants WHERE conversation_id = ? AND user_id = ? LIMIT 1",
        )
            .bind(&cid)
            .bind(&uid)
            .fetch_optional(pool)
            .await?;

        if participant_exists.is_some() {
            return Ok(true);
        }

        let author_exists: Option<i64> = sqlx::query_scalar(
            "SELECT 1 FROM messages WHERE conversation_id = ? AND author_id = ? LIMIT 1",
        )
            .bind(&cid)
            .bind(&uid)
            .fetch_optional(pool)
            .await?;

        Ok(author_exists.is_some())
    }

    pub async fn list_participant_ids(
        pool: &sqlx::Pool<sqlx::Sqlite>,
        conversation_id: Uuid,
    ) -> Result<Vec<Uuid>> {
        let conv_id_str = conversation_id.to_string();

        let id_strs: Vec<String> = sqlx::query_scalar(
            "SELECT user_id FROM participants WHERE conversation_id = ?"
        )
            .bind(&conv_id_str)
            .fetch_all(pool)
            .await
            .map_err(AppError::from)?;

        let mut ids = Vec::with_capacity(id_strs.len());
        for s in id_strs {
            match Uuid::parse_str(&s) {
                Ok(u) => ids.push(u),
                Err(_) => {
                    tracing::warn!("Invalid UUID string in participants.user_id: {}", s);
                }
            }
        }

        Ok(ids)
    }

    pub async fn get_members(
        pool: &SqlitePool,
        conversation_id: Uuid,
    ) -> Result<Vec<(Uuid, String, String, i64)>> {
        tracing::info!("Getting members for conversation: {}", conversation_id);

        let rows = sqlx::query(
            r#"
            SELECT
                p.user_id,
                u.username,
                p.role,
                0 as joined_at
            FROM participants p
            JOIN users u ON p.user_id = u.id
            WHERE p.conversation_id = ?
            ORDER BY u.username ASC
            "#,
        )
            .bind(conversation_id.to_string())
            .fetch_all(pool)
            .await
            .map_err(|e| {
                tracing::error!("Database error in get_members: {:?}", e);
                e
            })?;

        tracing::info!("Found {} members", rows.len());

        let members: Vec<(Uuid, String, String, i64)> = rows
            .into_iter()
            .map(|r| {
                let user_id_str: String = r.get("user_id");
                let username: String = r.get("username");
                let role: String = r.get("role");
                let joined_at: i64 = r.get("joined_at");
                (
                    Uuid::parse_str(&user_id_str).expect("DB must store valid UUIDs"),
                    username,
                    role,
                    joined_at,
                )
            })
            .collect();

        Ok(members)
    }

    pub async fn remove_member(
        pool: &SqlitePool,
        conversation_id: Uuid,
        user_id: Uuid,
    ) -> Result<()> {
        sqlx::query("DELETE FROM participants WHERE conversation_id = ? AND user_id = ?")
            .bind(conversation_id.to_string())
            .bind(user_id.to_string())
            .execute(pool)
            .await?;

        Ok(())
    }
}