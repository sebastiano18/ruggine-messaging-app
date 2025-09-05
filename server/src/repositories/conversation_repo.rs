use crate::error::Result;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct ConversationRepo;

impl ConversationRepo {

    pub async fn get_conversation_kind(pool: &SqlitePool, conversation_id: Uuid) -> Result<Option<String>> {
        let row = sqlx::query("SELECT kind FROM conversations WHERE id = ?")
            .bind(conversation_id.to_string())
            .fetch_optional(pool)
            .await?;

        Ok(row.map(|r| r.get("kind")))
    }

    /// Crea un nuovo gruppo e restituisce l'ID della conversazione (UUID).
    pub async fn create_group(pool: &SqlitePool, title: &str, owner_id: Uuid) -> Result<Uuid> {
        let conversation_id = Uuid::new_v4();

        // Inserisci la conversazione con ID esplicito (UUID come TEXT)
        sqlx::query(
            "INSERT INTO conversations(id, kind, title, owner_id, created_at)
             VALUES(?, 'group', ?, ?, strftime('%s','now'))",
        )
            .bind(conversation_id.to_string())
            .bind(title)
            .bind(owner_id.to_string())
            .execute(pool)
            .await?;

        // Aggiungi il creatore come partecipante con ruolo 'owner'
        sqlx::query(
            "INSERT INTO participants(conversation_id, user_id, role)
             VALUES(?, ?, 'owner')",
        )
            .bind(conversation_id.to_string())
            .bind(owner_id.to_string())
            .execute(pool)
            .await?;

        Ok(conversation_id)
    }

    /// Crea (o trova) una DM tra due utenti. Restituisce l'ID conversazione (UUID).
    pub async fn create_dm(pool: &SqlitePool, user1_id: Uuid, user2_id: Uuid) -> Result<Uuid> {
        // Se esiste già, restituiscila
        if let Some(existing_id) = Self::find_dm(pool, user1_id, user2_id).await? {
            return Ok(existing_id);
        }

        let conversation_id = Uuid::new_v4();

        // Crea la conversazione DM (title = NULL)
        sqlx::query(
            "INSERT INTO conversations(id, kind, title, owner_id, created_at)
             VALUES(?, 'dm', NULL, ?, strftime('%s','now'))",
        )
            .bind(conversation_id.to_string())
            .bind(user1_id.to_string())
            .execute(pool)
            .await?;

        // Aggiungi entrambi i partecipanti
        sqlx::query(
            "INSERT INTO participants(conversation_id, user_id, role)
             VALUES(?, ?, 'member'), (?, ?, 'member')",
        )
            .bind(conversation_id.to_string())
            .bind(user1_id.to_string())
            .bind(conversation_id.to_string())
            .bind(user2_id.to_string())
            .execute(pool)
            .await?;

        Ok(conversation_id)
    }

    /// Trova una DM esistente tra due utenti, se presente.
    pub async fn find_dm(pool: &SqlitePool, user1_id: Uuid, user2_id: Uuid) -> Result<Option<Uuid>> {
        let row = sqlx::query(
            r#"
            SELECT c.id
            FROM conversations c
            WHERE c.kind = 'dm'
              AND EXISTS(
                    SELECT 1 FROM participants p1
                    WHERE p1.conversation_id = c.id AND p1.user_id = ?
              )
              AND EXISTS(
                    SELECT 1 FROM participants p2
                    WHERE p2.conversation_id = c.id AND p2.user_id = ?
              )
            LIMIT 1
            "#,
        )
            .bind(user1_id.to_string())
            .bind(user2_id.to_string())
            .fetch_optional(pool)
            .await?;

        Ok(row.map(|r| {
            let id_str: String = r.get("id");
            Uuid::parse_str(&id_str).expect("DB must store valid UUIDs")
        }))
    }

    /// Aggiunge un membro a una conversazione (id conversazione e utente sono UUID).
    pub async fn add_member(pool: &SqlitePool, conversation_id: Uuid, user_id: Uuid) -> Result<()> {
        sqlx::query(
            "INSERT OR IGNORE INTO participants(conversation_id, user_id, role)
             VALUES(?, ?, 'member')",
        )
            .bind(conversation_id.to_string())
            .bind(user_id.to_string())
            .execute(pool)
            .await?;
        Ok(())
    }

    /// Restituisce le conversazioni dell'utente con tutti i campi necessari.
    /// Ritorna: (conversation_id, kind, display_title, owner_id, created_at)
    pub async fn by_user(pool: &SqlitePool, user_id: Uuid) -> Result<Vec<(Uuid, String, String, Uuid, i64)>> {
        let rows = sqlx::query(
            r#"
            SELECT
                c.id,
                c.kind,
                c.owner_id,
                c.created_at,
                CASE
                    WHEN c.kind = 'group' THEN c.title
                    WHEN c.kind = 'dm' THEN (
                        SELECT u.username
                        FROM participants p2
                        JOIN users u ON p2.user_id = u.id
                        WHERE p2.conversation_id = c.id AND p2.user_id != ?
                        LIMIT 1
                    )
                END AS display_title
            FROM conversations c
            JOIN participants p ON c.id = p.conversation_id
            WHERE p.user_id = ?
            ORDER BY c.created_at DESC
            "#,
        )
            .bind(user_id.to_string())
            .bind(user_id.to_string())
            .fetch_all(pool)
            .await?;

        Ok(rows
            .into_iter()
            .map(|r| {
                let id_str: String = r.get("id");
                let kind: String = r.get("kind");
                let owner_id_str: String = r.get("owner_id");
                let created_at: i64 = r.get("created_at");
                let title: Option<String> = r.get("display_title");
                (
                    Uuid::parse_str(&id_str).expect("DB must store valid UUIDs"),
                    kind,
                    title.unwrap_or_else(|| "Unknown".to_string()),
                    Uuid::parse_str(&owner_id_str).expect("DB must store valid UUIDs"),
                    created_at,
                )
            })
            .collect())
    }

    /// Verifica se `user_id` è owner della conversazione.
    pub async fn is_owner(pool: &SqlitePool, conversation_id: Uuid, user_id: Uuid) -> Result<bool> {
        let row = sqlx::query(
            "SELECT 1 FROM conversations WHERE id = ? AND owner_id = ?",
        )
            .bind(conversation_id.to_string())
            .bind(user_id.to_string())
            .fetch_optional(pool)
            .await?;
        Ok(row.is_some())
    }

    /// Verifica se `user_id` è partecipante della conversazione.
    pub async fn is_participant(pool: &SqlitePool, conversation_id: Uuid, user_id: Uuid) -> Result<bool> {
        let row = sqlx::query(
            "SELECT 1 FROM participants WHERE conversation_id = ? AND user_id = ?",
        )
            .bind(conversation_id.to_string())
            .bind(user_id.to_string())
            .fetch_optional(pool)
            .await?;
        Ok(row.is_some())
    }
}