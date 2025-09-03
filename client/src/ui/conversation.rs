use eframe::egui::{self, TextEdit, RichText};
use uuid::Uuid;
use crate::{state::{AppState, UiEvent}, net};

pub fn panel(ui: &mut egui::Ui, s: &mut AppState) {
    ui.heading("Conversazioni");
    ui.separator();

    if s.token.is_none() {
        ui.colored_label(egui::Color32::RED, "Login richiesto");
        return;
    }
    let token = s.token.clone().unwrap();

    // === Le mie conversazioni ===
    ui.heading("Le tue conversazioni");

    if ui.button("🔄 Ricarica conversazioni").clicked() {
        let base = s.base.clone();
        let token2 = token.clone();
        let tx = s.ui_tx.clone();
        s.rt.spawn(async move {
            match net::conversation::get_conversations(&base, &token2).await {
                Ok(conversations) => {
                    let _ = tx.send(UiEvent::ConversationsLoaded(conversations));
                }
                Err(e) => {
                    let _ = tx.send(UiEvent::Error(format!("Caricamento conversazioni fallito: {e}")));
                }
            }
        });
    }

    // Lista conversazioni
    if let Some(ref conversations) = s.conversations {
        for conv in conversations {
            ui.horizontal(|ui| {
                let label = match conv.kind.as_str() {
                    "group" => format!("👥 {}", conv.title),
                    "dm"    => format!("💬 {}", conv.title),
                    _       => conv.title.clone(),
                };

                if ui.button(&label).clicked() {
                    let _ = s.ui_tx.send(UiEvent::Opened(conv.id)); // conv.id: Uuid
                }
            });
        }
    }

    ui.separator();

    // === Crea un nuovo gruppo ===
    ui.heading("➕ Crea un nuovo gruppo");
    ui.text_edit_singleline(&mut s.group_name);
    if ui.button("Crea gruppo").clicked() {
        let base = s.base.clone();
        let name = s.group_name.clone();
        let tx = s.ui_tx.clone();
        let token2 = token.clone();
        s.rt.spawn(async move {
            match net::conversation::create_group(&base, &token2, &name).await {
                Ok(cid) => {
                    let _ = tx.send(UiEvent::Opened(cid)); // cid: Uuid
                }
                Err(e) => {
                    let _ = tx.send(UiEvent::Error(format!("Creazione gruppo fallita: {e}")));
                }
            }
        });
    }

    ui.separator();

    // === Inizia una DM ===
    ui.heading("💬 Inizia una chat privata");
    ui.horizontal(|ui| {
        ui.label("User ID (UUID):");
        ui.add(TextEdit::singleline(&mut s.dm_user_id_input));
    });

    if ui.button("Inizia DM").clicked() {
        // Valida subito l'UUID inserito
        let target_uuid = match Uuid::parse_str(&s.dm_user_id_input.trim()) {
            Ok(u) => u,
            Err(_) => {
                let _ = s.ui_tx.send(UiEvent::Error("UUID non valido".into()));
                return;
            }
        };

        let base = s.base.clone();
        let tx = s.ui_tx.clone();
        let token2 = token.clone();
        s.rt.spawn(async move {
            match net::conversation::create_dm(&base, &token2, target_uuid).await {
                Ok(cid) => {
                    let _ = tx.send(UiEvent::Opened(cid));
                }
                Err(e) => {
                    let _ = tx.send(UiEvent::Error(format!("Creazione DM fallita: {e}")));
                }
            }
        });
    }

    ui.separator();

    // === Join da token ===
    ui.heading("🎫 Join da token");
    let mut token_input = s.last_invite_token.clone().unwrap_or_default();
    ui.text_edit_singleline(&mut token_input);

    if ui.button(RichText::new("Join").strong()).clicked() {
        s.last_invite_token = Some(token_input.clone());

        let base = s.base.clone();
        let token2 = token.clone();
        let tx = s.ui_tx.clone();
        s.rt.spawn(async move {
            match net::conversation::join_by_token(&base, &token2, &token_input).await {
                Ok(cid) => {
                    let _ = tx.send(UiEvent::Opened(cid)); // cid: Uuid
                }
                Err(e) => {
                    let _ = tx.send(UiEvent::Error(format!("Join fallito: {e}")));
                }
            }
        });
    }
}
