use std::{fs, path::Path};
use std::str::FromStr;
use sqlx::{SqlitePool, sqlite::{SqliteConnectOptions, SqlitePoolOptions, SqliteJournalMode}};
use anyhow::Context;

/// Inizializza il pool SQLite creando la cartella del DB se manca.
/// Accetta URL del tipo:
/// - sqlite://db/ruggine.sqlite   (relativo)
/// - sqlite:///C:/path/ruggine.sqlite (assoluto su Windows)
/// - sqlite://./ruggine.sqlite
/// - :memory:
pub async fn init_pool(url: &str) -> anyhow::Result<SqlitePool> {
    // Estrai il path del file per creare la directory, se non è :memory:
    if url != ":memory:" {
        // Togli lo schema "sqlite://" se presente
        let mut path = url.strip_prefix("sqlite://").unwrap_or(url);

        // Su Windows, un assoluto arriva come "/C:/...": togli lo slash iniziale
        #[cfg(windows)]
        {
            if let Some(rest) = path.strip_prefix('/') {
                // solo se il formato è tipo /C:/...
                if rest.chars().nth(1) == Some(':') {
                    path = rest;
                }
            }
        }

        // Se non è un URL in-memory, crea la cartella padre
        if !path.eq_ignore_ascii_case(":memory:") {
            if let Some(parent) = Path::new(path).parent() {
                if !parent.as_os_str().is_empty() {
                    fs::create_dir_all(parent)
                        .with_context(|| format!("Impossibile creare la cartella del DB: {}", parent.display()))?;
                }
            }
        }
    }

    // Costruisci le opzioni di connessione da URL
    let mut opts = SqliteConnectOptions::from_str(url)
        .with_context(|| format!("URL SQLite non valido: {url}"))?;
    opts = opts
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)   // WAL migliora robustezza e concorrenza
        .foreign_keys(true);                    // PRAGMA foreign_keys = ON

    // Connetti il pool
    let pool = SqlitePoolOptions::new()
        .max_connections(10)
        .connect_with(opts)
        .await
        .context("Connessione a SQLite fallita")?;

    Ok(pool)
}
