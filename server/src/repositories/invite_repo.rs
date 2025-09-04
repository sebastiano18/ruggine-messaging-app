use crate::error::Result;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct InviteRepo;

impl InviteRepo {
    /// Crea un nuovo token di invito per una conversazione
    pub async fn create_invite(
        pool: &SqlitePool,
        conversation_id: Uuid,
        expires_at: i64 // Unix timestamp (sempre richiesto nel tuo schema)
    ) -> Result<String> {
        let invite_id = Uuid::new_v4();
        let token = Self::generate_token();

        sqlx::query(
            "INSERT INTO invites(id, conversation_id, token, expires_at, used)
             VALUES(?, ?, ?, ?, 0)",
        )
            .bind(invite_id.to_string())
            .bind(conversation_id.to_string())
            .bind(&token)
            .bind(expires_at)
            .execute(pool)
            .await?;

        Ok(token)
    }

    /// Trova un invito valido per token
    pub async fn find_valid_invite(
        pool: &SqlitePool,
        token: &str
    ) -> Result<Option<Uuid>> { // Restituisce conversation_id se valido
        let row = sqlx::query(
            r#"
            SELECT conversation_id
            FROM invites
            WHERE token = ?
              AND expires_at > strftime('%s','now')
              AND used = 0
            "#,
        )
            .bind(token)
            .fetch_optional(pool)
            .await?;

        Ok(row.map(|r| {
            let conv_id_str: String = r.get("conversation_id");
            Uuid::parse_str(&conv_id_str).expect("DB must store valid UUIDs")
        }))
    }

    /// Marca un invito come utilizzato (nel tuo schema è boolean, non counter)
    pub async fn mark_as_used(pool: &SqlitePool, token: &str) -> Result<()> {
        sqlx::query(
            "UPDATE invites SET used = 1 WHERE token = ?",
        )
            .bind(token)
            .execute(pool)
            .await?;
        Ok(())
    }

    /// Ottieni tutti gli inviti per una conversazione
    pub async fn by_conversation(
        pool: &SqlitePool,
        conversation_id: Uuid
    ) -> Result<Vec<(String, String, i64, bool)>> { // (id, token, expires_at, used)
        let rows = sqlx::query(
            r#"
            SELECT id, token, expires_at, used
            FROM invites
            WHERE conversation_id = ?
            ORDER BY expires_at DESC
            "#,
        )
            .bind(conversation_id.to_string())
            .fetch_all(pool)
            .await?;

        Ok(rows
            .into_iter()
            .map(|r| {
                let id: String = r.get("id");
                let token: String = r.get("token");
                let expires_at: i64 = r.get("expires_at");
                let used: bool = r.get("used");
                (id, token, expires_at, used)
            })
            .collect())
    }

    /// Elimina un invito (revoca)
    pub async fn delete_invite(pool: &SqlitePool, token: &str) -> Result<()> {
        sqlx::query(
            "DELETE FROM invites WHERE token = ?",
        )
            .bind(token)
            .execute(pool)
            .await?;
        Ok(())
    }

    /// Genera un token casuale per l'invito
    fn generate_token() -> String {
        use rand::Rng;
        const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
        let mut rng = rand::thread_rng();

        (0..12)
            .map(|_| {
                let idx = rng.gen_range(0..CHARSET.len());
                CHARSET[idx] as char
            })
            .collect()
    }
}