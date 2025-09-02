use eframe::egui::{self, TextEdit};
use crate::{state::{AppState, UiEvent}, net};

pub fn panel(ui: &mut egui::Ui, s: &mut AppState) {
    ui.heading("Accesso");
    ui.separator();
    ui.label("Server base URL");
    ui.text_edit_singleline(&mut s.base);
    ui.add_space(8.0);

    ui.label("Username");
    ui.text_edit_singleline(&mut s.username);
    ui.label("Password");
    ui.add(TextEdit::singleline(&mut s.password).password(true));
    ui.add_space(8.0);

    if ui.button("Register + Login").clicked() {
        let base = s.base.clone();
        let u = s.username.clone();
        let p = s.password.clone();
        let tx = s.ui_tx.clone();
        s.rt.spawn(async move {
            if let Err(e) = net::auth::register(&base, &u, &p).await {
                let _ = tx.send(UiEvent::Info(format!("register note: {e}")));
            }
            match net::auth::login(&base, &u, &p).await {
                Ok(t) => { let _ = tx.send(UiEvent::Logged(t)); }
                Err(e) => { let _ = tx.send(UiEvent::Error(format!("login failed: {e}"))); }
            }
        });
    }
}
