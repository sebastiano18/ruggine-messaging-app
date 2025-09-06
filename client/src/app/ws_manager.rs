use crate::state::AppState;
use serde_json::Value;
use uuid::Uuid;
use tracing::{warn, error, debug};
use crate::models::{MessageDto, Outgoing, UiEvent, WsStatus};

pub struct WebSocketManager;

impl WebSocketManager {
    pub fn new() -> Self {
        Self
    }

    pub fn ensure_ws_lifecycle(&mut self, state: &mut AppState) {
        // 1) drena la coda dei messaggi UI -> WS
        self.process_outgoing_messages(state);

        // 2) se non autenticato, niente WS
        if state.token.is_none() {
            if state.ws_status != WsStatus::Disconnected {
                state.ws_status = WsStatus::Disconnected;
                // Chiudi connessione esistente se presente
                if let Some(ctrl) = state.ws_ctrl.take() {
                    let _ = ctrl.shutdown.send(());
                }
            }
            return;
        }

        // 3) gestione riconnessione manuale
        if state.request_ws_reconnect && state.ws_status != WsStatus::Connecting {
            state.request_ws_reconnect = false;
            state.ws_status = WsStatus::Disconnected;
            // Chiudi connessione esistente
            if let Some(ctrl) = state.ws_ctrl.take() {
                let _ = ctrl.shutdown.send(());
            }
        }

        // 4) avvio/tenuta connessione
        match state.ws_status {
            WsStatus::Disconnected => self.start_websocket_connection(state),
            WsStatus::Connecting | WsStatus::Connected => { /* no-op */ }
        }
    }

    fn process_outgoing_messages(&self, state: &mut AppState) {
        // Drena la coda UI -> rete
        while let Ok(outgoing) = state.ui_to_net_rx.try_recv() {
            if let Some(ref ws_ctrl) = state.ws_ctrl {
                // Serializza con conversione field names compatibili con server
                let json_msg = match outgoing {
                    Outgoing::ChatMessage { cid, content } => {
                        serde_json::json!({
                            "type": "chat_message",
                            "cid": cid,  // Usa "cid" per compatibilità server
                            "content": content
                        }).to_string()
                    }
                    Outgoing::InviteUser { cid, username } => {
                        serde_json::json!({
                            "type": "invite_user",
                            "cid": cid,
                            "username": username
                        }).to_string()
                    }
                    Outgoing::Typing { cid, is_typing } => {
                        serde_json::json!({
                            "type": "typing",
                            "cid": cid,
                            "is_typing": is_typing
                        }).to_string()
                    }
                };

                debug!("Sending WebSocket message: {}", json_msg);

                if let Err(_) = ws_ctrl.outgoing_tx.send(json_msg) {
                    warn!("WebSocket channel closed, marking as disconnected");
                    let _ = state.ui_tx.send(UiEvent::WsDisconnected);
                }
            } else {
                warn!("Attempted to send message but WebSocket not connected");
            }
        }
    }

    fn start_websocket_connection(&mut self, state: &mut AppState) {
        let base = state.base.clone();
        let token = state.token.clone().unwrap();
        let tx = state.ui_tx.clone();

        state.ws_status = WsStatus::Connecting;

        state.rt.spawn(async move {
            match crate::api::ws::connect(&base, &token).await {
                Ok(mut ws) => {
                    if let Err(e) = crate::api::ws::subscribe(&mut ws).await {
                        let _ = tx.send(UiEvent::WsError(format!("WS subscribe fallito: {e}")));
                        return;
                    }
                    let _ = tx.send(UiEvent::WsConnected);
                    let tx_reader = tx.clone();

                    // Handler bidirezionale con gestione errori migliorata
                    let ctrl = crate::api::ws::spawn_bidirectional_handler(ws, move |msg| {
                        Self::handle_websocket_message(&tx_reader, msg);
                    });

                    let _ = tx.send(UiEvent::WsControlReady(ctrl));
                }
                Err(e) => {
                    let _ = tx.send(UiEvent::WsError(format!("WS connect fallito: {e}")));
                }
            }
        });
    }

    // CORREZIONE PRINCIPALE: parsing robusto e gestione di tutti i tipi di messaggio
    fn handle_websocket_message(tx: &tokio::sync::mpsc::UnboundedSender<UiEvent>, msg: String) {
        debug!("Received WebSocket message: {}", msg);

        // Prima prova a parsare come JSON generale
        let parsed_value: Value = match serde_json::from_str(&msg) {
            Ok(v) => v,
            Err(e) => {
                error!("Failed to parse WebSocket message as JSON: {} - Message: {}", e, msg);
                let _ = tx.send(UiEvent::Error("Messaggio WebSocket malformato".into()));
                return;
            }
        };

        let msg_type = parsed_value.get("type")
            .and_then(|t| t.as_str())
            .unwrap_or("unknown");

        match msg_type {
            "chat_message" => {
                Self::handle_chat_message(tx, &parsed_value);
            }
            "typing" => {
                // Gestisci indicatori di scrittura se necessario
                debug!("Received typing indicator: {:?}", parsed_value);
            }
            "error" => {
                let error_msg = parsed_value.get("message")
                    .and_then(|m| m.as_str())
                    .unwrap_or("Errore WebSocket sconosciuto");
                let _ = tx.send(UiEvent::Error(format!("Server: {}", error_msg)));
            }
            "heartbeat_ack" | "server_heartbeat" => {
                // Heartbeat dal server, nessuna azione necessaria
                debug!("Received heartbeat from server");
            }
            "system" => {
                let system_msg = parsed_value.get("message")
                    .and_then(|m| m.as_str())
                    .unwrap_or("Messaggio di sistema");
                let _ = tx.send(UiEvent::Info(format!("Sistema: {}", system_msg)));
            }
            _ => {
                // Fallback: prova a interpretare come messaggio di chat
                Self::handle_chat_message(tx, &parsed_value);
            }
        }
    }

    fn handle_chat_message(tx: &tokio::sync::mpsc::UnboundedSender<UiEvent>, value: &Value) {
        // Estrai campi del messaggio con fallback robusti
        let id = value.get("id")
            .and_then(|v| v.as_str())
            .and_then(|s| Uuid::parse_str(s).ok())
            .unwrap_or_else(Uuid::new_v4);

        let author_id = value.get("author_id")
            .and_then(|v| v.as_str())
            .and_then(|s| Uuid::parse_str(s).ok())
            .unwrap_or(Uuid::nil());

        let author_username = value.get("author_username")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        // Supporta sia "cid" che "conversation_id"
        let conversation_id = value.get("cid")
            .or_else(|| value.get("conversation_id"))
            .and_then(|v| v.as_str())
            .and_then(|s| Uuid::parse_str(s).ok());

        let content = value.get("content")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let created_at = value.get("created_at")
            .and_then(|v| v.as_i64())
            .unwrap_or_else(|| chrono::Utc::now().timestamp());

        // Valida che abbiamo i campi essenziali
        match (conversation_id, content) {
            (Some(conv_id), Some(content)) => {
                let dto = MessageDto {
                    id,
                    author_id,
                    author_username,
                    conversation_id: conv_id,
                    content,
                    created_at,
                };

                debug!("Parsed message DTO: {:?}", dto);
                let _ = tx.send(UiEvent::WsIncoming(dto));
            }
            _ => {
                warn!("Incomplete message data in WebSocket payload: {:?}", value);
                let _ = tx.send(UiEvent::Error("Messaggio WebSocket incompleto".into()));
            }
        }
    }
}