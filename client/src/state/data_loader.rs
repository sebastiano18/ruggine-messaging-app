use crate::models::*;
use uuid::Uuid;
use std::collections::HashMap;
use tracing::{info, warn, error};

pub struct DataLoader;

impl DataLoader {
    /// Avvia il caricamento completo di tutti i dati dell'utente dopo il login
    pub fn preload_all_data(state: &mut super::core::AppState, token: String) {
        state.is_loading = true;

        let base = state.base.clone();
        let tx = state.ui_tx.clone();

        state.rt.spawn(async move {
            Self::execute_full_preload(base, token, tx).await;
        });
    }

    /// Carica i messaggi di una singola conversazione
    pub fn load_single_conversation_messages(state: &super::core::AppState, cid: Uuid) {
        if let Some(ref token) = state.token {
            let base = state.base.clone();
            let token = token.clone();
            let tx = state.ui_tx.clone();

            state.rt.spawn(async move {
                Self::execute_single_conversation_load(base, token, cid, tx).await;
            });
        }
    }

    /// Ricarica tutte le conversazioni (senza messaggi) - Metodo principale per il refresh
    pub fn refresh_conversations(state: &super::core::AppState) {
        if let Some(ref token) = state.token {
            let base = state.base.clone();
            let token = token.clone();
            let tx = state.ui_tx.clone();

            state.rt.spawn(async move {
                match crate::api::conversation::get_conversations(&base, &token).await {
                    Ok(conversations) => {
                        let _ = tx.send(UiEvent::ConversationsLoaded(conversations));
                    }
                    Err(e) => {
                        let _ = tx.send(UiEvent::Error(format!("Ricaricamento conversazioni fallito: {}", e)));
                    }
                }
            });
        }
    }

    /// Refresh delle conversazioni con apertura automatica di una conversazione specifica
    /// MIGLIORATO: Implementa backoff esponenziale e gestione errori più robusta
    pub fn refresh_conversations_and_open(state: &super::core::AppState, conversation_to_open: Uuid) {
        if let Some(ref token) = state.token {
            let base = state.base.clone();
            let token = token.clone();
            let tx = state.ui_tx.clone();

            state.rt.spawn(async move {
                Self::execute_retry_with_backoff(base, token, tx, conversation_to_open).await;
            });
        }
    }

    /// NUOVO: Implementa strategia di retry con backoff esponenziale e jitter
    async fn execute_retry_with_backoff(
        base: String,
        token: String,
        tx: tokio::sync::mpsc::UnboundedSender<UiEvent>,
        conversation_to_open: Uuid,
    ) {
        const MAX_ATTEMPTS: u32 = 6;
        const BASE_DELAY_MS: u64 = 150;
        const MAX_DELAY_MS: u64 = 5000;

        for attempt in 1..=MAX_ATTEMPTS {
            // Calcola delay con backoff esponenziale + jitter
            let delay_ms = if attempt == 1 {
                0 // Primo tentativo immediato
            } else {
                let base_delay = BASE_DELAY_MS * (2_u64.pow(attempt - 2));
                let capped_delay = base_delay.min(MAX_DELAY_MS);
                // Aggiungi jitter ±25%
                let jitter = (capped_delay as f64 * 0.25 * (rand::random::<f64>() - 0.5)) as u64;
                capped_delay + jitter
            };

            if delay_ms > 0 {
                info!("Retry attempt {} in {}ms for conversation {}", attempt, delay_ms, conversation_to_open);
                let _ = tx.send(UiEvent::Info(format!("Sincronizzazione... ({}°)", attempt)));
                tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
            }

            match crate::api::conversation::get_conversations(&base, &token).await {
                Ok(conversations) => {
                    // Verifica se la conversazione target esiste
                    if conversations.iter().any(|c| c.id == conversation_to_open) {
                        info!("Successfully found conversation {} after {} attempts", 
                              conversation_to_open, attempt);
                        let _ = tx.send(UiEvent::ConversationsLoaded(conversations));
                        let _ = tx.send(UiEvent::Info("Nuova conversazione sincronizzata".into()));
                        return; // Successo, esci
                    } else if attempt == MAX_ATTEMPTS {
                        // Ultimo tentativo fallito, carica comunque e segnala errore
                        warn!("Conversation {} not found after {} attempts", 
                              conversation_to_open, MAX_ATTEMPTS);
                        let _ = tx.send(UiEvent::ConversationsLoaded(conversations));
                        let _ = tx.send(UiEvent::Error(
                            "Conversazione non trovata. Potrebbe essere necessario ricaricare manualmente.".into()
                        ));
                        return;
                    }
                    // Continua il loop per retry
                }
                Err(e) => {
                    error!("API error during conversation sync attempt {}: {}", attempt, e);

                    if attempt == MAX_ATTEMPTS {
                        let _ = tx.send(UiEvent::Error(
                            format!("Sincronizzazione fallita dopo {} tentativi: {}", MAX_ATTEMPTS, e)
                        ));
                        return;
                    }

                    // Per errori di rete, continua a provare
                    // Per errori di autenticazione, ferma subito
                    if e.to_string().to_lowercase().contains("unauthorized") ||
                        e.to_string().to_lowercase().contains("forbidden") {
                        let _ = tx.send(UiEvent::Error("Errore di autenticazione. Rilogga.".into()));
                        return;
                    }

                    // Continua il loop per altri tipi di errore
                }
            }
        }
    }

    /// Esegue il precaricamento completo (conversazioni + tutti i messaggi)
    /// MIGLIORATO: Gestione errori più robusta e timeout
    async fn execute_full_preload(
        base: String,
        token: String,
        tx: tokio::sync::mpsc::UnboundedSender<UiEvent>
    ) {
        let _ = tx.send(UiEvent::LoadingProgress("Caricamento conversazioni...".into()));

        // 1. Carica tutte le conversazioni con timeout
        let conversations = match tokio::time::timeout(
            tokio::time::Duration::from_secs(30),
            crate::api::conversation::get_conversations(&base, &token)
        ).await {
            Ok(Ok(convs)) => {
                let _ = tx.send(UiEvent::ConversationsLoaded(convs.clone()));
                convs
            }
            Ok(Err(e)) => {
                error!("Failed to load conversations: {}", e);
                let _ = tx.send(UiEvent::Error(format!("Caricamento conversazioni fallito: {}", e)));
                return;
            }
            Err(_) => {
                error!("Timeout loading conversations");
                let _ = tx.send(UiEvent::Error("Timeout nel caricamento conversazioni".into()));
                return;
            }
        };

        // Se non ci sono conversazioni, completa comunque il caricamento
        if conversations.is_empty() {
            let _ = tx.send(UiEvent::AllMessagesLoaded(HashMap::new()));
            let _ = tx.send(UiEvent::InitialLoadComplete);
            return;
        }

        // 2. Carica i messaggi per ogni conversazione in parallelo con controllo concorrenza
        let total_conversations = conversations.len();
        let _ = tx.send(UiEvent::LoadingProgress(
            format!("Caricamento messaggi per {} conversazioni...", total_conversations)
        ));

        let mut all_messages = HashMap::new();
        // Limita concorrenza basata sul numero di conversazioni
        let max_concurrent = (total_conversations.min(8)).max(2);
        let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(max_concurrent));

        let mut handles = vec![];
        for (index, conv) in conversations.into_iter().enumerate() {
            let base_clone = base.clone();
            let token_clone = token.clone();
            let tx_clone = tx.clone();
            let semaphore_clone = semaphore.clone();

            let handle = tokio::spawn(async move {
                let _permit = semaphore_clone.acquire().await.unwrap();

                let _ = tx_clone.send(UiEvent::LoadingProgress(
                    format!("Caricando messaggi ({}/{}): {}",
                            index + 1, total_conversations,
                            conv.title.chars().take(30).collect::<String>())
                ));

                // Timeout per singola conversazione
                let messages = match tokio::time::timeout(
                    tokio::time::Duration::from_secs(20),
                    Self::load_conversation_messages(&base_clone, &token_clone, conv.id)
                ).await {
                    Ok(Ok(msgs)) => msgs,
                    Ok(Err(e)) => {
                        warn!("Failed to load messages for conversation {}: {}", conv.id, e);
                        vec![MessageDto::system_message(
                            format!("Errore caricamento messaggi: {}", e)
                        )]
                    }
                    Err(_) => {
                        warn!("Timeout loading messages for conversation {}", conv.id);
                        vec![MessageDto::system_message(
                            "Timeout nel caricamento messaggi".into()
                        )]
                    }
                };

                (conv.id, messages)
            });

            handles.push(handle);
        }

        // Attendi tutti i caricamenti con timeout globale
        let collect_future = async {
            for handle in handles {
                if let Ok((conv_id, messages)) = handle.await {
                    all_messages.insert(conv_id, messages);
                }
            }
        };

        match tokio::time::timeout(tokio::time::Duration::from_secs(120), collect_future).await {
            Ok(_) => {
                info!("Successfully loaded all conversation messages");
            }
            Err(_) => {
                warn!("Timeout waiting for all messages to load");
                let _ = tx.send(UiEvent::Info("Alcuni messaggi potrebbero non essere stati caricati completamente".into()));
            }
        }

        // 3. Invia tutti i messaggi e completa il caricamento
        let _ = tx.send(UiEvent::AllMessagesLoaded(all_messages));
        let _ = tx.send(UiEvent::InitialLoadComplete);
    }

    /// Esegue il caricamento dei messaggi di una singola conversazione
    /// MIGLIORATO: Aggiunto timeout
    async fn execute_single_conversation_load(
        base: String,
        token: String,
        cid: Uuid,
        tx: tokio::sync::mpsc::UnboundedSender<UiEvent>
    ) {
        let load_future = Self::load_conversation_messages(&base, &token, cid);

        match tokio::time::timeout(tokio::time::Duration::from_secs(15), load_future).await {
            Ok(Ok(messages)) => {
                let _ = tx.send(UiEvent::RefreshedMsgs(messages));
            }
            Ok(Err(e)) => {
                let _ = tx.send(UiEvent::Error(format!("Caricamento messaggi fallito: {}", e)));
            }
            Err(_) => {
                let _ = tx.send(UiEvent::Error("Timeout nel caricamento messaggi".into()));
            }
        }
    }

    /// Helper per caricare i messaggi di una conversazione specifica
    async fn load_conversation_messages(
        base: &str,
        token: &str,
        conversation_id: Uuid
    ) -> Result<Vec<MessageDto>, Box<dyn std::error::Error + Send + Sync>> {
        let messages = crate::api::chat::get_messages(base, token, conversation_id).await?;
        Ok(messages)
    }

    /// Carica i messaggi più recenti per una conversazione (utile per aggiornamenti incrementali)
    /// MIGLIORATO: Aggiunto timeout e gestione limiti
    pub fn load_recent_messages(state: &super::core::AppState, cid: Uuid, limit: Option<u32>) {
        if let Some(ref token) = state.token {
            let base = state.base.clone();
            let token = token.clone();
            let tx = state.ui_tx.clone();
            let limit = limit.unwrap_or(50).min(200); // Cap a 200 messaggi max

            state.rt.spawn(async move {
                let load_future = Self::load_conversation_messages(&base, &token, cid);

                match tokio::time::timeout(tokio::time::Duration::from_secs(10), load_future).await {
                    Ok(Ok(mut messages)) => {
                        // Limita i messaggi se richiesto
                        if messages.len() > limit as usize {
                            let start = messages.len().saturating_sub(limit as usize);
                            messages = messages.split_off(start);
                        }
                        let _ = tx.send(UiEvent::RefreshedMsgs(messages));
                    }
                    Ok(Err(e)) => {
                        let _ = tx.send(UiEvent::Error(format!("Caricamento messaggi recenti fallito: {}", e)));
                    }
                    Err(_) => {
                        let _ = tx.send(UiEvent::Error("Timeout messaggi recenti".into()));
                    }
                }
            });
        }
    }

    /// Precarica solo i metadati delle conversazioni (senza messaggi) - utile per refresh rapidi
    /// MIGLIORATO: Aggiunto timeout
    pub fn preload_conversations_only(state: &mut super::core::AppState, token: String) {
        let base = state.base.clone();
        let tx = state.ui_tx.clone();

        state.rt.spawn(async move {
            let _ = tx.send(UiEvent::LoadingProgress("Caricamento conversazioni...".into()));

            let load_future = crate::api::conversation::get_conversations(&base, &token);
            match tokio::time::timeout(tokio::time::Duration::from_secs(15), load_future).await {
                Ok(Ok(conversations)) => {
                    let _ = tx.send(UiEvent::ConversationsLoaded(conversations));
                }
                Ok(Err(e)) => {
                    let _ = tx.send(UiEvent::Error(format!("Caricamento conversazioni fallito: {}", e)));
                }
                Err(_) => {
                    let _ = tx.send(UiEvent::Error("Timeout caricamento conversazioni".into()));
                }
            }
        });
    }

    /// Forza il refresh di una conversazione specifica e i suoi messaggi
    /// MIGLIORATO: Gestione errori più robusta
    pub fn force_refresh_conversation(state: &super::core::AppState, cid: Uuid) {
        if let Some(ref token) = state.token {
            let base = state.base.clone();
            let token = token.clone();
            let tx = state.ui_tx.clone();

            state.rt.spawn(async move {
                let _ = tx.send(UiEvent::LoadingProgress("Aggiornamento conversazione...".into()));

                // Prima ricarica la lista conversazioni con timeout
                let conv_future = crate::api::conversation::get_conversations(&base, &token);
                match tokio::time::timeout(tokio::time::Duration::from_secs(10), conv_future).await {
                    Ok(Ok(conversations)) => {
                        let _ = tx.send(UiEvent::ConversationsLoaded(conversations));

                        // Poi ricarica i messaggi specifici
                        let msg_future = Self::load_conversation_messages(&base, &token, cid);
                        match tokio::time::timeout(tokio::time::Duration::from_secs(15), msg_future).await {
                            Ok(Ok(messages)) => {
                                let _ = tx.send(UiEvent::RefreshedMsgs(messages));
                            }
                            Ok(Err(e)) => {
                                let _ = tx.send(UiEvent::Error(format!("Ricaricamento messaggi fallito: {}", e)));
                            }
                            Err(_) => {
                                let _ = tx.send(UiEvent::Error("Timeout ricaricamento messaggi".into()));
                            }
                        }
                    }
                    Ok(Err(e)) => {
                        let _ = tx.send(UiEvent::Error(format!("Ricaricamento conversazioni fallito: {}", e)));
                    }
                    Err(_) => {
                        let _ = tx.send(UiEvent::Error("Timeout ricaricamento conversazioni".into()));
                    }
                }
            });
        }
    }

    /// Carica messaggi in batch per multiple conversazioni
    /// MIGLIORATO: Migliore controllo concorrenza e timeout
    pub fn load_messages_batch(state: &super::core::AppState, conversation_ids: Vec<Uuid>) {
        if let Some(ref token) = state.token {
            let base = state.base.clone();
            let token = token.clone();
            let tx = state.ui_tx.clone();

            if conversation_ids.is_empty() {
                return;
            }

            state.rt.spawn(async move {
                let _ = tx.send(UiEvent::LoadingProgress(
                    format!("Caricamento batch {} conversazioni...", conversation_ids.len())
                ));

                let mut messages_map = HashMap::new();
                let max_concurrent = conversation_ids.len().min(5).max(1);
                let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(max_concurrent));

                let mut handles = vec![];
                for cid in conversation_ids {
                    let base_clone = base.clone();
                    let token_clone = token.clone();
                    let semaphore_clone = semaphore.clone();

                    let handle = tokio::spawn(async move {
                        let _permit = semaphore_clone.acquire().await.unwrap();

                        let load_future = Self::load_conversation_messages(&base_clone, &token_clone, cid);
                        match tokio::time::timeout(tokio::time::Duration::from_secs(20), load_future).await {
                            Ok(Ok(messages)) => Some((cid, messages)),
                            Ok(Err(e)) => {
                                warn!("Failed to load messages for conversation {} in batch: {}", cid, e);
                                None
                            }
                            Err(_) => {
                                warn!("Timeout loading messages for conversation {} in batch", cid);
                                None
                            }
                        }
                    });

                    handles.push(handle);
                }

                // Raccogli tutti i risultati con timeout globale
                let collect_future = async {
                    for handle in handles {
                        if let Ok(Some((cid, messages))) = handle.await {
                            messages_map.insert(cid, messages);
                        }
                    }
                };

                match tokio::time::timeout(tokio::time::Duration::from_secs(60), collect_future).await {
                    Ok(_) => {
                        if !messages_map.is_empty() {
                            let _ = tx.send(UiEvent::AllMessagesLoaded(messages_map));
                        } else {
                            let _ = tx.send(UiEvent::Error("Nessun messaggio caricato nel batch".into()));
                        }
                    }
                    Err(_) => {
                        let _ = tx.send(UiEvent::Error("Timeout nel caricamento batch".into()));
                    }
                }
            });
        }
    }

    /// NUOVO: Implementa connection health check per determinare quando fare retry
    pub async fn check_connection_health(base: &str, token: &str) -> bool {
        let health_future = async {
            // Prova una richiesta leggera per verificare la connessione
            match crate::api::conversation::get_conversations(base, token).await {
                Ok(_) => true,
                Err(e) => {
                    let error_str = e.to_string().to_lowercase();
                    // Distingui tra errori di rete e errori di applicazione
                    !error_str.contains("network") && !error_str.contains("timeout") && !error_str.contains("connection")
                }
            }
        };

        match tokio::time::timeout(tokio::time::Duration::from_secs(5), health_future).await {
            Ok(result) => result,
            Err(_) => false, // Timeout = problemi di connessione
        }
    }

    /// NUOVO: Retry intelligente che usa connection health check
    pub fn smart_retry_operation<F, Fut, T>(
        state: &super::core::AppState,
        operation_name: &'static str,
        operation_factory: F,
    )
    where
        F: Fn(String, String) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = Result<T, Box<dyn std::error::Error + Send + Sync>>> + Send,
        T: Send + 'static,
    {
        if let Some(ref token) = state.token {
            let base = state.base.clone();
            let token = token.clone();
            let tx = state.ui_tx.clone();

            state.rt.spawn(async move {
                const MAX_RETRIES: u32 = 3;
                const BASE_DELAY: u64 = 1000; // 1 secondo

                for attempt in 1..=MAX_RETRIES {
                    // Prima verifica lo stato della connessione
                    if attempt > 1 && !Self::check_connection_health(&base, &token).await {
                        let delay = BASE_DELAY * (2_u64.pow(attempt - 1));
                        let _ = tx.send(UiEvent::Info(
                            format!("Problemi di connessione, retry {} in {}s...", attempt, delay / 1000)
                        ));
                        tokio::time::sleep(tokio::time::Duration::from_millis(delay)).await;
                        continue;
                    }

                    // Crea una nuova operation per ogni tentativo
                    let operation = operation_factory(base.clone(), token.clone());
                    match operation.await {
                        Ok(_result) => {
                            if attempt > 1 {
                                let _ = tx.send(UiEvent::Info(
                                    format!("{} completata dopo {} tentativi", operation_name, attempt)
                                ));
                            }
                            return; // Successo
                        }
                        Err(e) => {
                            if attempt == MAX_RETRIES {
                                let _ = tx.send(UiEvent::Error(
                                    format!("{} fallita dopo {} tentativi: {}", operation_name, MAX_RETRIES, e)
                                ));
                            } else {
                                let _ = tx.send(UiEvent::Info(
                                    format!("{} fallita ({}°), retry...", operation_name, attempt)
                                ));
                            }
                        }
                    }

                    // Delay prima del prossimo tentativo
                    if attempt < MAX_RETRIES {
                        let delay = BASE_DELAY * (2_u64.pow(attempt - 1));
                        tokio::time::sleep(tokio::time::Duration::from_millis(delay)).await;
                    }
                }
            });
        }
    }

    /// NUOVO: Caricamento progressivo dei messaggi (utile per conversazioni molto lunghe)
    pub fn load_messages_progressive(
        state: &super::core::AppState,
        cid: Uuid,
        batch_size: u32,
        offset: u32,
    ) {
        if let Some(ref token) = state.token {
            let base = state.base.clone();
            let token = token.clone();
            let tx = state.ui_tx.clone();

            state.rt.spawn(async move {
                let _ = tx.send(UiEvent::LoadingProgress(
                    format!("Caricando messaggi batch {} ({})", offset / batch_size + 1, batch_size)
                ));

                // Simula paginazione (da implementare nell'API)
                match Self::load_conversation_messages(&base, &token, cid).await {
                    Ok(mut messages) => {
                        // Simula offset e limit fino a quando l'API non supporta paginazione
                        let start = offset as usize;
                        let end = (start + batch_size as usize).min(messages.len());

                        if start < messages.len() {
                            messages = messages[start..end].to_vec();
                            let _ = tx.send(UiEvent::RefreshedMsgs(messages));
                        } else {
                            let _ = tx.send(UiEvent::Info("Nessun messaggio aggiuntivo da caricare".into()));
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(UiEvent::Error(format!("Caricamento progressivo fallito: {}", e)));
                    }
                }
            });
        }
    }

    /// DEPRECATO: Usa refresh_conversations_and_open invece
    #[deprecated(note = "Usa refresh_conversations_and_open con retry intelligente")]
    pub fn smart_refresh_with_new_conversation(state: &super::core::AppState, new_conversation_id: Uuid) {
        Self::refresh_conversations_and_open(state, new_conversation_id);
    }

    /// NUOVO: Cache warming per conversazioni frequenti
    pub fn warm_cache_for_frequent_conversations(
        state: &super::core::AppState,
        conversation_ids: Vec<Uuid>,
    ) {
        if conversation_ids.is_empty() {
            return;
        }

        if let Some(ref token) = state.token {
            let base = state.base.clone();
            let token = token.clone();
            let tx = state.ui_tx.clone();

            state.rt.spawn(async move {
                let _ = tx.send(UiEvent::Info(
                    format!("Preriscaldamento cache per {} conversazioni...", conversation_ids.len())
                ));

                // Carica in background senza bloccare l'UI
                let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(2)); // Bassa priorità
                let mut handles = vec![];

                for cid in conversation_ids {
                    let base_clone = base.clone();
                    let token_clone = token.clone();
                    let semaphore_clone = semaphore.clone();

                    let handle = tokio::spawn(async move {
                        let _permit = semaphore_clone.acquire().await.unwrap();

                        // Timeout più lungo per cache warming
                        match tokio::time::timeout(
                            tokio::time::Duration::from_secs(30),
                            Self::load_conversation_messages(&base_clone, &token_clone, cid)
                        ).await {
                            Ok(Ok(messages)) => Some((cid, messages)),
                            _ => None, // Ignora errori durante cache warming
                        }
                    });

                    handles.push(handle);
                }

                let mut warmed_count = 0;
                for handle in handles {
                    if let Ok(Some((_cid, _messages))) = handle.await {
                        warmed_count += 1;
                    }
                }

                if warmed_count > 0 {
                    let _ = tx.send(UiEvent::Info(
                        format!("Cache preriscaldata per {} conversazioni", warmed_count)
                    ));
                }
            });
        }
    }

    /// NUOVO: Ottimizzazione di memoria - rimuove messaggi vecchi da conversazioni inattive
    pub fn optimize_memory_usage(state: &mut super::core::AppState, max_messages_per_conversation: usize) {
        let current_cid = state.cid;
        let mut optimized_count = 0;

        for (cid, messages) in state.conversation_messages.iter_mut() {
            // Non ottimizzare la conversazione corrente
            if Some(*cid) == current_cid {
                continue;
            }

            if messages.len() > max_messages_per_conversation {
                // Mantieni solo gli ultimi N messaggi
                let keep_from = messages.len() - max_messages_per_conversation;
                *messages = messages.split_off(keep_from);
                optimized_count += 1;
            }
        }

        if optimized_count > 0 {
            info!("Ottimizzata memoria per {} conversazioni", optimized_count);
            let _ = state.ui_tx.send(UiEvent::Info(
                format!("Memoria ottimizzata ({} conversazioni)", optimized_count)
            ));
        }
    }
}