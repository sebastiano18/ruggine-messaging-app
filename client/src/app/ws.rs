use crate::models::{MessageDto, UiEvent, WsStatus};
use crate::state::AppState;

pub struct WebSocketManager;

impl WebSocketManager {
    pub fn new() -> Self {
        Self
    }

    pub fn ensure_ws_lifecycle(&mut self, state: &mut AppState) {
        // Se l'utente non è autenticato, nessun WS
        if state.token.is_none() {
            state.ws_status = WsStatus::Disconnected;
            return;
        }

        // Se l'utente ha richiesto riconnessione manuale, forziamo
        if state.request_ws_reconnect && state.ws_status != WsStatus::Connecting {
            state.request_ws_reconnect = false;
            state.ws_status = WsStatus::Disconnected; // forza ramo sotto
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

    fn start_websocket_connection(&mut self, state: &mut AppState) {
        // avvia connessione WS globale
        let base = state.base.clone();
        let token = state.token.clone().unwrap();
        let tx = state.ui_tx.clone();

        state.ws_status = WsStatus::Connecting;

        state.rt.spawn(async move {
            match crate::net::ws::connect(&base, &token).await {
                Ok(mut ws) => {
                    if let Err(e) = crate::net::ws::subscribe(&mut ws).await {
                        let _ = tx.send(UiEvent::WsError(format!("WS subscribe fallito: {e}")));
                        return;
                    }
                    let _ = tx.send(UiEvent::WsConnected);
                    let tx_reader = tx.clone();

                    let ctrl = crate::net::ws::spawn_reader_and_pinger(ws, move |msg| {
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
        // Parse the incoming WebSocket message as MessageDto
        match serde_json::from_str::<MessageDto>(&msg) {
            Ok(message_dto) => {
                let _ = tx.send(UiEvent::WsIncoming(message_dto));
            }
            Err(e) => {
                // If parsing fails, create a system message with the raw content
                let system_message = MessageDto {
                    id: uuid::Uuid::new_v4(),
                    author_id: uuid::Uuid::nil(),
                    author_username: "system".to_string(),
                    content: format!("Raw WS message: {}", msg),
                    created_at: chrono::Utc::now().timestamp(),
                };
                let _ = tx.send(UiEvent::WsIncoming(system_message));

                // Also send error notification
                let _ = tx.send(UiEvent::Error(
                    format!("Failed to parse WS message: {}", e)
                ));
            }
        }
    }
}