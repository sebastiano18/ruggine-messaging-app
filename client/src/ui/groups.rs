use eframe::egui::{self, TextEdit};
use crate::{state::{AppState, UiEvent, Page}, net};

pub fn panel(ui: &mut egui::Ui, s: &mut AppState) {
    ui.heading("Gruppi");
    ui.separator();

    if s.token.is_none() {
        ui.colored_label(egui::Color32::RED, "Login richiesto");
        return;
    }
    let token = s.token.clone().unwrap();

    ui.label("Nome nuovo gruppo");
    ui.text_edit_singleline(&mut s.group_name);

    if ui.button("Crea gruppo").clicked() {
        let base = s.base.clone();
        let name = s.group_name.clone();
        let tx = s.ui_tx.clone();
        let token2 = token.clone();
        s.rt.spawn(async move {
            match net::groups::create_group(&base, &token2, &name).await {
                Ok(cid) => { let _ = tx.send(UiEvent::Opened(cid)); }
                Err(e)  => { let _ = tx.send(UiEvent::Error(format!("create group failed: {e}"))); }
            }
        });
    }

    ui.add_space(10.0);
    ui.separator();
    ui.heading("Inviti");
    ui.label("Per invitare: genera un token e spediscilo. Chi lo riceve usa 'Join da token'.");

    // (Semplice) Per generare invito serve l'id del group, ma noi abbiamo il cid della conversazione del gruppo.
    // In questo scheletro assumiamo che 'cid' coincida col gruppo creato adesso (v. create_group ritorna cid).
    if let Some(cid) = s.cid {
        if ui.button("Genera invito per questo gruppo").clicked() {
            let base = s.base.clone();
            let token2 = token.clone();
            let tx = s.ui_tx.clone();

            // Nota: se ti serve realmente il group_id separato dal cid, adegua l'API lato server o conserva l'id al momento della creazione.
            // Qui, per semplicità, supponiamo che group_id == cid (oppure adatta con una mappa lato server).
            s.rt.spawn(async move {
                match net::groups::create_invite(&base, &token2, cid).await {
                    Ok(t) => { let _ = tx.send(UiEvent::Info(format!("Invite token: {t}"))); }
                    Err(e) => { let _ = tx.send(UiEvent::Error(format!("create invite failed: {e}"))); }
                }
            });
        }
    } else {
        ui.label("Crea o apri un gruppo per generare un invito.");
    }

    ui.add_space(10.0);
    ui.separator();
    ui.heading("Join da token");
    static mut TOKEN_IN: String = String::new();
    // evitare static mut in prod; qui per brevità usiamo un local mut ogni frame:
    let mut token_input = String::new();
    ui.add(TextEdit::singleline(&mut token_input).hint_text("Incolla token invito..."));
    if ui.button("Join").clicked() {
        let base = s.base.clone();
        let token2 = token.clone();
        let tx = s.ui_tx.clone();
        s.rt.spawn(async move {
            match net::groups::join_by_token(&base, &token2, &token_input).await {
                Ok(cid) => { let _ = tx.send(UiEvent::Opened(cid)); }
                Err(e)  => { let _ = tx.send(UiEvent::Error(format!("join failed: {e}"))); }
            }
        });
    }

    ui.add_space(10.0);
    if ui.button("Vai alla Chat").clicked() { s.page = Page::Chat; }
}
