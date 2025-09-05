use crate::models::{MessageDto, UiEvent, WsStatus, Outgoing};
use crate::state::AppState;
use serde_json::json;

pub struct WebSocketManager;

impl WebSocketManager {
    pub fn new() -> Self {
        Self
    }

    pub fn ensure_ws_lifecycle(&mut self, state: &mut AppState) {
        // Gestione dell'invio di messaggi in uscita
        self.process_outgoing_messages(state);

        // Se l'utente non è autenticato, nessun WS
        if state.token.is_none() {
            state.ws_status = WsStatus::Disconnected;
            return;
        }

        // Se l'utente ha richiesto riconnessione manuale, forziamo
        if state.request_ws_reconnect && state.ws_status != WsStatus::Connecting {
            state.request_ws_reconnect = false;
            state.ws_status = WsStatus::Disconnected;
        }

        match state.ws_status {
            WsStatus::Disconnected => {
                self.start_websocket_connection(state);
            }
            WsStatus::Connecting | WsStatus::Connected => {
                // nulla da fare
            }
        }
    }

    fn process_outgoing_messages(&self, state: &mut AppState) {
        // Drena la coda dei messaggi in uscita
        while let Ok(outgoing) = state.ui_to_net_rx.try_recv() {
            if let Some(ref ws_ctrl) = state.ws_ctrl {
                let json_msg = match outgoing {
                    Outgoing::ChatMessage { cid, content } => {
                        json!({
                            "type": "chat_message",
                            "conversation_id": cid,
                            "content": content
                        }).to_string()
                    }
                    Outgoing::InviteUser { cid, username } => {
                        json!({
                            "type": "invite_user",
                            "conversation_id": cid,
                            "username": username
                        }).to_string()
                    }
                    Outgoing::Typing { cid, is_typing } => {
                        json!({
                            "type": "typing",
                            "conversation_id": cid,
                            "is_typing": is_typing
                        }).to_string()
                    }
                };

                if let Err(_) = ws_ctrl.outgoing_tx.send(json_msg) {
                    // WebSocket disconnesso, aggiorna stato
                    let _ = state.ui_tx.send(UiEvent::WsDisconnected);
                }
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

                    // Usa la nuova funzione bidirezionale
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

    fn handle_websocket_message(tx: &tokio::sync::mpsc::UnboundedSender<UiEvent>, msg: String) {
        // Parse del messaggio WebSocket in arrivo
        match serde_json::from_str::<MessageDto>(&msg) {
            Ok(message_dto) => {
                let _ = tx.send(UiEvent::WsIncoming(message_dto));
            }
            Err(_) => {
                // Se il parsing fallisce, prova a parsare come messaggio di sistema
                if let Ok(system_msg) = serde_json::from_str::<serde_json::Value>(&msg) {
                    if let Some(msg_type) = system_msg.get("type").and_then(|t| t.as_str()) {
                        match msg_type {
                            "error" => {
                                if let Some(error_msg) = system_msg.get("message").and_then(|m| m.as_str()) {
                                    let _ = tx.send(UiEvent::Error(error_msg.to_string()));
                                }
                            }
                            "info" => {
                                if let Some(info_msg) = system_msg.get("message").and_then(|m| m.as_str()) {
                                    let _ = tx.send(UiEvent::Info(info_msg.to_string()));
                                }
                            }
                            _ => {
                                let _ = tx.send(UiEvent::Info(format!("Messaggio WS: {}", msg)));
                            }
                        }
                    }
                } else {
                    // Fallback: crea un messaggio di sistema
                    let system_message = MessageDto {
                        id: uuid::Uuid::new_v4(),
                        author_id: uuid::Uuid::nil(),
                        conversation_id: uuid::Uuid::nil(),
                        author_username: "system".to_string(),
                        content: format!("Raw WS message: {}", msg),
                        created_at: chrono::Utc::now().timestamp(),
                    };
                    let _ = tx.send(UiEvent::WsIncoming(system_message));
                }
            }
        }
    }
}