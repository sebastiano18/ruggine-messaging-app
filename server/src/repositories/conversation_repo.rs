use crate::error::Result;
use sqlx::{Row, SqlitePool};

#[derive(Debug, Clone)]
pub struct ConversationRepo;

impl ConversationRepo {
    // Crea una nuova conversazione (gruppo)
    pub async fn create_group(pool: &SqlitePool, title: &str, owner_id: i64) -> Result<i64> {
        let res = sqlx::query(
            "INSERT INTO conversations(kind, title, owner_id, created_at) VALUES('group', ?, ?, strftime('%s','now'))",
        )
            .bind(title)
            .bind(owner_id)
            .execute(pool)
            .await?;

        let conversation_id = res.last_insert_rowid();

        // Aggiungi il creatore come partecipante con ruolo 'owner'
        sqlx::query(
            "INSERT INTO participants(conversation_id, user_id, role) VALUES(?, ?, 'owner')",
        )
            .bind(conversation_id)
            .bind(owner_id)
            .execute(pool)
            .await?;

        Ok(conversation_id)
    }

    // Crea una conversazione DM
    pub async fn create_dm(pool: &SqlitePool, user1_id: i64, user2_id: i64) -> Result<i64> {
        // Prima controlla se esiste già una DM tra questi due utenti
        if let Some(existing_id) = Self::find_dm(pool, user1_id, user2_id).await? {
            return Ok(existing_id);
        }

        let res = sqlx::query(
            "INSERT INTO conversations(kind, title, owner_id, created_at) VALUES('dm', NULL, ?, strftime('%s','now'))",
        )
            .bind(user1_id)
            .execute(pool)
            .await?;

        let conversation_id = res.last_insert_rowid();

        // Aggiungi entrambi gli utenti come partecipanti
        sqlx::query(
            "INSERT INTO participants(conversation_id, user_id, role) VALUES(?, ?, 'member'), (?, ?, 'member')",
        )
            .bind(conversation_id)
            .bind(user1_id)
            .bind(conversation_id)
            .bind(user2_id)
            .execute(pool)
            .await?;

        Ok(conversation_id)
    }

    // Trova DM esistente tra due utenti
    pub async fn find_dm(pool: &SqlitePool, user1_id: i64, user2_id: i64) -> Result<Option<i64>> {
        let row = sqlx::query(
            r#"
            SELECT c.id 
            FROM conversations c
            WHERE c.kind = 'dm' 
            AND EXISTS(SELECT 1 FROM participants p1 WHERE p1.conversation_id = c.id AND p1.user_id = ?)
            AND EXISTS(SELECT 1 FROM participants p2 WHERE p2.conversation_id = c.id AND p2.user_id = ?)
            "#,
        )
            .bind(user1_id)
            .bind(user2_id)
            .fetch_optional(pool)
            .await?;

        Ok(row.map(|r| r.get("id")))
    }

    // Aggiungi membro a una conversazione
    pub async fn add_member(pool: &SqlitePool, conversation_id: i64, user_id: i64) -> Result<()> {
        sqlx::query(
            "INSERT OR IGNORE INTO participants(conversation_id, user_id, role) VALUES(?, ?, 'member')",
        )
            .bind(conversation_id)
            .bind(user_id)
            .execute(pool)
            .await?;

        Ok(())
    }

    // Ottieni conversazioni di un utente
    pub async fn by_user(pool: &SqlitePool, user_id: i64) -> Result<Vec<(i64, String, String)>> {
        let rows = sqlx::query(
            r#"
            SELECT 
                c.id, 
                c.kind,
                CASE 
                    WHEN c.kind = 'group' THEN c.title
                    WHEN c.kind = 'dm' THEN (
                        SELECT u.username 
                        FROM participants p2 
                        JOIN users u ON p2.user_id = u.id 
                        WHERE p2.conversation_id = c.id AND p2.user_id != ?
                        LIMIT 1
                    )
                END as display_title
            FROM conversations c
            JOIN participants p ON c.id = p.conversation_id
            WHERE p.user_id = ?
            ORDER BY c.id DESC
            "#,
        )
            .bind(user_id)
            .bind(user_id)
            .fetch_all(pool)
            .await?;

        Ok(rows.into_iter().map(|r| (
            r.get("id"),
            r.get("kind"),
            r.get::<Option<String>, _>("display_title").unwrap_or_else(|| "Unknown".to_string())
        )).collect())
    }

    // Verifica se un utente è owner di una conversazione
    pub async fn is_owner(pool: &SqlitePool, conversation_id: i64, user_id: i64) -> Result<bool> {
        let row = sqlx::query(
            "SELECT 1 FROM conversations WHERE id = ? AND owner_id = ?",
        )
            .bind(conversation_id)
            .bind(user_id)
            .fetch_optional(pool)
            .await?;
        Ok(row.is_some())
    }

    // Verifica se un utente è partecipante di una conversazione
    pub async fn is_participant(pool: &SqlitePool, conversation_id: i64, user_id: i64) -> Result<bool> {
        let row = sqlx::query(
            "SELECT 1 FROM participants WHERE conversation_id = ? AND user_id = ?",
        )
            .bind(conversation_id)
            .bind(user_id)
            .fetch_optional(pool)
            .await?;
        Ok(row.is_some())
    }
}