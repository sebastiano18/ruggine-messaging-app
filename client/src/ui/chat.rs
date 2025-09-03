use eframe::egui::{self, TextEdit};
use uuid::Uuid;
use crate::{state::{AppState, UiEvent, WsStatus}, net};

pub fn panel(ui: &mut egui::Ui, s: &mut AppState) {
    ui.heading("💬 Chat");
    ui.separator();

    if s.token.is_none() {
        ui.colored_label(egui::Color32::RED, "Login richiesto");
        return;
    }
    let token = s.token.clone().unwrap();

    // Selezione/Mostra conversazione corrente
    if let Some(cid) = s.cid {
        // Info conversazione corrente
        if let Some(ref conversations) = s.conversations {
            if let Some(current_conv) = conversations.iter().find(|c| c.id == cid) {
                let icon = match current_conv.kind.as_str() {
                    "group" => "👥",
                    "dm" => "💬",
                    _ => "💭",
                };
                ui.label(format!("{} {}", icon, current_conv.title));
                ui.separator();
            }
        }

        // Solo stato WS (la connessione è gestita in app.rs)
        match s.ws_status {
            WsStatus::Disconnected => {
                ui.colored_label(egui::Color32::RED, "⚫ WebSocket disconnesso");
            }
            WsStatus::Connecting => {
                ui.colored_label(egui::Color32::YELLOW, "🟡 WebSocket connessione...");
            }
            WsStatus::Connected => {
                ui.colored_label(egui::Color32::GREEN, "🟢 WebSocket connesso");
            }
        }

        if ui.button("🔄 Ricarica messaggi").clicked() {
            let base = s.base.clone();
            let token2 = token.clone();
            let tx = s.ui_tx.clone();
            let cid2 = cid;
            s.rt.spawn(async move {
                match net::chat::get_messages(&base, &token2, cid2).await {
                    Ok(list) => {
                        let msgs = list.into_iter()
                            .map(|m| format!("[{}] {}", m.author_id, m.content))
                            .collect();
                        let _ = tx.send(UiEvent::RefreshedMsgs(msgs));
                    }
                    Err(e) => {
                        let _ = tx.send(UiEvent::Error(format!("Caricamento messaggi fallito: {e}")));
                    }
                }
            });
        }
        ui.separator();

        // Area messaggi
        egui::ScrollArea::vertical().stick_to_bottom(true).show(ui, |ui| {
            for line in &s.messages {
                ui.label(line);
            }
        });

        ui.separator();

        // Input messaggio
        ui.horizontal(|ui| {
            let resp = TextEdit::singleline(&mut s.input)
                .hint_text("Scrivi un messaggio...")
                .desired_width(f32::INFINITY)
                .show(ui);
            if resp.response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                send_now(s, cid, &token);
            }
            if ui.button("📤 Invia").clicked() {
                send_now(s, cid, &token);
            }
        });

        if s.ws_status == WsStatus::Disconnected {
            if ui.button("🔌 Riconnetti WS").clicked() {
                s.request_ws_reconnect = true; // flag letto in app.rs
            }
        }
    } else {
        ui.label("❌ Nessuna conversazione selezionata.");
        ui.label("Vai alla scheda 'Conversazioni' per creare o selezionare una chat.");
    }
}

fn send_now(s: &mut AppState, cid: Uuid, token: &str) {
    let body = s.input.trim().to_string();
    if body.is_empty() { return; }
    s.input.clear();

    // Feedback immediato
    s.messages.push(format!("Tu: {body}"));

    let base = s.base.clone();
    let token = token.to_string();
    let tx = s.ui_tx.clone();
    s.rt.spawn(async move {
        match net::chat::send_message(&base, &token, cid, &body).await {
            Ok(_) => {
                let _ = tx.send(UiEvent::Info("Messaggio inviato ✅".into()));
            }
            Err(e) => {
                let _ = tx.send(UiEvent::Error(format!("Invio fallito: {e}")));
            }
        }
    });
}
