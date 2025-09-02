use eframe::egui::{self, TextEdit};
use futures::StreamExt;
use crate::{state::{AppState, UiEvent}, net};

pub fn panel(ui: &mut egui::Ui, s: &mut AppState) {
    ui.heading("Chat");
    ui.separator();

    if s.token.is_none() { ui.colored_label(egui::Color32::RED, "Login richiesto"); return; }
    let token = s.token.clone().unwrap();

    // Selezione/Mostra conversazione corrente
    if let Some(cid) = s.cid {
        if ui.button("Ricarica messaggi").clicked() {
            let base = s.base.clone(); let token2 = token.clone();
            let tx = s.ui_tx.clone();
            s.rt.spawn(async move {
                match net::chat::get_messages(&base, &token2, cid).await {
                    Ok(list) => {
                        let msgs = list.into_iter().map(|m| format!("[{}] {}", m.author_id, m.body)).collect();
                        let _ = tx.send(UiEvent::RefreshedMsgs(msgs));
                    }
                    Err(e) => { let _ = tx.send(UiEvent::Error(format!("get messages failed: {e}"))); }
                }
            });
        }
        ui.separator();

        egui::ScrollArea::vertical().stick_to_bottom(true).show(ui, |ui| {
            for line in &s.messages { ui.label(line); }
        });

        ui.separator();
        ui.horizontal(|ui| {
            let resp = TextEdit::singleline(&mut s.input)
                .hint_text("Scrivi…")
                .desired_width(f32::INFINITY)
                .show(ui);
            if resp.response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                send_now(s, cid, &token);
            }
            if ui.button("Invia").clicked() {
                send_now(s, cid, &token);
            }
        });

        // WS connect (una volta sola) — per semplicità, avvialo quando si entra in Chat e non è connesso
        // Qui facciamo un attach best-effort quando premi "Ricarica messaggi": puoi spostarlo dove preferisci.
        if ui.button("Connetti WS").clicked() {
            let base = s.base.clone();
            let token2 = token.clone();
            let tx = s.ui_tx.clone();
            s.rt.spawn(async move {
                match net::ws::connect(&base, &token2).await {
                    Ok(mut ws) => {
                        let _ = net::ws::subscribe(&mut ws, &[cid]).await;
                        let tx2 = tx.clone();
                        tokio::spawn(async move {
                            while let Some(Ok(msg)) = ws.next().await {
                                if let tokio_tungstenite::tungstenite::Message::Text(t) = msg {
                                    let _ = tx2.send(UiEvent::WsIncoming(t));
                                }
                            }
                        });
                        let _ = tx.send(UiEvent::WsConnected);
                    }
                    Err(e) => { let _ = tx.send(UiEvent::Error(format!("ws connect failed: {e}"))); }
                }
            });
        }
    } else {
        ui.label("Nessuna conversazione selezionata. Crea/entra in un gruppo nella scheda Gruppi.");
    }
}

fn send_now(s: &mut AppState, cid: i64, token: &str) {
    let body = s.input.trim().to_string();
    if body.is_empty() { return; }
    s.input.clear();
    s.messages.push(format!("me: {body}"));

    let base = s.base.clone();
    let token = token.to_string();
    let tx = s.ui_tx.clone();
    s.rt.spawn(async move {
        match net::chat::send_message(&base, &token, cid, &body).await {
            Ok(_) => { let _ = tx.send(UiEvent::Info("POST /messages -> 200".into())); }
            Err(e) => { let _ = tx.send(UiEvent::Error(format!("send failed: {e}"))); }
        }
    });
}
