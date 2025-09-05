use crate::models::{MessageDto, UiEvent, WsStatus, Outgoing};
use crate::state::AppState;
use serde_json::Value;
use uuid::Uuid;

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
            state.ws_status = WsStatus::Disconnected;
            return;
        }

        // 3) gestione riconnessione manuale
        if state.request_ws_reconnect && state.ws_status != WsStatus::Connecting {
            state.request_ws_reconnect = false;
            state.ws_status = WsStatus::Disconnected;
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
                // ❗ Mantengo l'encoding MANUALE come avevi (conversation_id, ecc.)
                // così siamo compatibili con il server anche se Outgoing ha campi diversi lato client.
                let json_msg = match outgoing {
                    Outgoing::ChatMessage { cid, content } => {
                        serde_json::json!({
                            "type": "chat_message",
                            "conversation_id": cid,
                            "content": content
                        }).to_string()
                    }
                    Outgoing::InviteUser { cid, username } => {
                        serde_json::json!({
                            "type": "invite_user",
                            "conversation_id": cid,
                            "username": username
                        }).to_string()
                    }
                    Outgoing::Typing { cid, is_typing } => {
                        serde_json::json!({
                            "type": "typing",
                            "conversation_id": cid,
                            "is_typing": is_typing
                        }).to_string()
                    }
                };

                if let Err(_) = ws_ctrl.outgoing_tx.send(json_msg) {
                    // canale WS chiuso
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

                    // handler bidirezionale: per ogni messaggio testo in ingresso chiama il parser tipizzato
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
        // 1) prova parse diretto nel DTO che manda il server
        if let Ok(dto) = serde_json::from_str::<MessageDto>(&msg) {
            let _ = tx.send(UiEvent::WsIncoming(dto));
            return;
        }

        // 2) fallback: accetta sia "cid" sia "conversation_id"
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&msg) {
            if let (Some(content), Some(conv)) =
                (v.get("content").and_then(|s| s.as_str()),
                 v.get("conversation_id").or_else(|| v.get("cid")).and_then(|s| s.as_str()))
            {
                let dto = MessageDto {
                    id: v.get("id").and_then(|s| s.as_str()).and_then(|s| uuid::Uuid::parse_str(s).ok()).unwrap_or_else(uuid::Uuid::new_v4),
                    author_id: v.get("author_id").and_then(|s| s.as_str()).and_then(|s| uuid::Uuid::parse_str(s).ok()).unwrap_or(uuid::Uuid::nil()),
                    author_username: v.get("author_username").and_then(|s| s.as_str()).unwrap_or("unknown").to_string(),
                    conversation_id: uuid::Uuid::parse_str(conv).unwrap_or(uuid::Uuid::nil()),
                    content: content.to_string(),
                    created_at: v.get("created_at").and_then(|n| n.as_i64()).unwrap_or(chrono::Utc::now().timestamp()),
                };
                let _ = tx.send(UiEvent::WsIncoming(dto));
                return;
            }
        }

        let _ = tx.send(UiEvent::Error("WS: payload non riconosciuto".into()));
    }

}
