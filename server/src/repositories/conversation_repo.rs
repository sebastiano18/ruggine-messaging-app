use crate::error::Result;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;
use crate::error::AppError;

#[derive(Debug, Clone)]
pub struct ConversationRepo;

impl ConversationRepo {
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

    /// NUOVO: Ottiene una singola conversazione con dettagli completi
    /// Ritorna: (conversation_id, kind, display_title, owner_id, created_at)
    pub async fn get_single_conversation(
        pool: &SqlitePool,
        conversation_id: Uuid,
        user_id: Uuid,
    ) -> Result<Option<(Uuid, String, String, Uuid, i64)>> {
        let row = sqlx::query(
            r#"
            SELECT
                c.id,
                c.kind,
                CASE
                    WHEN c.kind = 'group' THEN c.title
                    WHEN c.kind = 'dm' THEN (
                        -- Per DM, mostra l'altro utente
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
                c.created_at
            FROM conversations c
            LEFT JOIN participants p ON c.id = p.conversation_id AND p.user_id = ?
            WHERE c.id = ?
              AND (p.user_id = ? OR (c.kind = 'dm' AND EXISTS(
                    SELECT 1 FROM messages m
                    WHERE m.conversation_id = c.id AND m.author_id = ?
              )))
            "#,
        )
            .bind(user_id.to_string())
            .bind(user_id.to_string())
            .bind(user_id.to_string())
            .bind(conversation_id.to_string())
            .bind(user_id.to_string())
            .bind(user_id.to_string())
            .fetch_optional(pool)
            .await?;

        Ok(row.map(|r| {
            let id_str: String = r.get("id");
            let kind: String = r.get("kind");
            let display_title: Option<String> = r.get("display_title");
            let owner_id_str: String = r.get("owner_id");
            let created_at: i64 = r.get("created_at");
            (
                Uuid::parse_str(&id_str).expect("DB must store valid UUIDs"),
                kind,
                display_title.unwrap_or_else(|| "Unknown".to_string()),
                Uuid::parse_str(&owner_id_str).expect("DB must store valid UUIDs"),
                created_at,
            )
        }))
    }

    /// Crea un nuovo gruppo e restituisce l'ID della conversazione (UUID).
    /// IMPORTANTE: Solo il creatore viene aggiunto come partecipante
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

        // Aggiungi solo il creatore come partecipante con ruolo 'owner'
        // Gli altri membri si aggiungeranno tramite lazy registration quando inviano messaggi
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

    /// Crea una DM tra due utenti SENZA aggiungere partecipanti automaticamente.
    /// I partecipanti verranno aggiunti tramite lazy registration quando inviano il primo messaggio.
    pub async fn create_dm(pool: &SqlitePool, user1_id: Uuid, user2_id: Uuid) -> Result<Uuid> {
        // Se esiste già una DM tra questi utenti, restituiscila
        if let Some(existing_id) = Self::find_dm_by_involved_users(pool, user1_id, user2_id).await?
        {
            return Ok(existing_id);
        }

        let conversation_id = Uuid::new_v4();

        // Crea SOLO la conversazione DM - NESSUN partecipante automatico
        sqlx::query(
            "INSERT INTO conversations(id, kind, title, owner_id, created_at)
             VALUES(?, 'dm', NULL, ?, strftime('%s','now'))",
        )
            .bind(conversation_id.to_string())
            .bind(user1_id.to_string())
            .execute(pool)
            .await?;

        tracing::info!(
            "Created empty DM conversation {} for users {} and {} (no auto-participants)",
            conversation_id,
            user1_id,
            user2_id
        );

        Ok(conversation_id)
    }

    /// Trova una DM esistente tra due utenti basandosi sui messaggi inviati o partecipazione
    pub async fn find_dm_by_involved_users(
        pool: &SqlitePool,
        user1_id: Uuid,
        user2_id: Uuid,
    ) -> Result<Option<Uuid>> {
        // Prima prova: cerca DM con entrambi come partecipanti (metodo tradizionale)
        let participant_based = sqlx::query_scalar::<_, String>(
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

        if let Some(conv_id_str) = participant_based {
            return Ok(Some(
                Uuid::parse_str(&conv_id_str).expect("DB must store valid UUIDs"),
            ));
        }

        // Seconda prova: cerca DM basandosi sui messaggi inviati (per lazy registration)
        let message_based = sqlx::query_scalar::<_, String>(
            r#"
            SELECT c.id
            FROM conversations c
            WHERE c.kind = 'dm'
              AND EXISTS(
                    SELECT 1 FROM messages m1
                    WHERE m1.conversation_id = c.id AND m1.author_id = ?
              )
              AND EXISTS(
                    SELECT 1 FROM messages m2
                    WHERE m2.conversation_id = c.id AND m2.author_id = ?
              )
            LIMIT 1
            "#,
        )
            .bind(user1_id.to_string())
            .bind(user2_id.to_string())
            .fetch_optional(pool)
            .await?;

        Ok(message_based
            .map(|conv_id_str| Uuid::parse_str(&conv_id_str).expect("DB must store valid UUIDs")))
    }

    /// Trova una DM esistente (metodo legacy per compatibilità)
    pub async fn find_dm(
        pool: &SqlitePool,
        user1_id: Uuid,
        user2_id: Uuid,
    ) -> Result<Option<Uuid>> {
        Self::find_dm_by_involved_users(pool, user1_id, user2_id).await
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
    pub async fn by_user(
        pool: &SqlitePool,
        user_id: Uuid,
    ) -> Result<Vec<(Uuid, String, String, Uuid, i64)>> {
        let rows = sqlx::query(
            r#"
        SELECT
            c.id,
            c.kind,
            CASE
                WHEN c.kind = 'group' THEN c.title
                WHEN c.kind = 'dm' THEN (
                    -- Per DM, mostra l'altro utente basandosi sui messaggi se non ci sono partecipanti
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
            c.created_at
        FROM conversations c
        LEFT JOIN participants p ON c.id = p.conversation_id AND p.user_id = ?
        WHERE p.user_id = ?
           OR (c.kind = 'dm' AND EXISTS(
                SELECT 1 FROM messages m 
                WHERE m.conversation_id = c.id AND m.author_id = ?
           ))
        ORDER BY c.created_at DESC
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
                (
                    Uuid::parse_str(&id_str).expect("DB must store valid UUIDs"),
                    kind,
                    display_title.unwrap_or_else(|| "Unknown".to_string()),
                    Uuid::parse_str(&owner_id_str).expect("DB must store valid UUIDs"),
                    created_at,
                )
            })
            .collect())
    }

    /// Verifica se `user_id` è owner della conversazione.
    pub async fn is_owner(pool: &SqlitePool, conversation_id: Uuid, user_id: Uuid) -> Result<bool> {
        let row = sqlx::query("SELECT 1 FROM conversations WHERE id = ? AND owner_id = ?")
            .bind(conversation_id.to_string())
            .bind(user_id.to_string())
            .fetch_optional(pool)
            .await?;
        Ok(row.is_some())
    }

    /// Verifica se `user_id` è partecipante della conversazione.
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

    // Funzione helper
    /// Per DM: consente accesso se l'utente è partecipante oppure ha scritto almeno un messaggio.
    pub async fn user_has_dm_access(pool: &SqlitePool, conversation_id: Uuid, user_id: Uuid) -> Result<bool> {
        let cid = conversation_id.to_string();
        let uid = user_id.to_string();

        // Partecipante?
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

        // Ha scritto almeno un messaggio?
        let author_exists: Option<i64> = sqlx::query_scalar(
            "SELECT 1 FROM messages WHERE conversation_id = ? AND author_id = ? LIMIT 1",
        )
            .bind(&cid)
            .bind(&uid)
            .fetch_optional(pool)
            .await?;

        Ok(author_exists.is_some())
    }

    /// Restituisce la lista degli user_id (Uuid) dei partecipanti alla conversazione.
    /// Nel DB gli UUID sono salvati come TEXT, quindi si fa parse da String -> Uuid.
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
}
