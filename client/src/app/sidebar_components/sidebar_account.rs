use eframe::egui;
use crate::models::{UiEvent, WsStatus};
use crate::state::AppState;

pub struct AccountSidebar;

impl AccountSidebar {
    pub fn new() -> Self {
        Self
    }

    pub fn show(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        ui.heading("⚙️Account");
        ui.separator();
        ui.add_space(8.0);

        if let Some(ref username) = state.token.as_ref().map(|_| &state.username) {
            self.show_user_info(ui, state, username);
            ui.add_space(12.0);

            self.show_configuration(ui, state);
            ui.add_space(12.0);

            self.show_statistics(ui, state);
            ui.add_space(12.0);

            self.show_logout_button(ui, state);
        }
    }

    fn show_user_info(&self, ui: &mut egui::Ui, state: &AppState, username: &str) {
        ui.group(|ui| {
            ui.label(egui::RichText::new("Informazioni Utente").strong());
            ui.separator();

            ui.horizontal(|ui| {
                ui.label("👤 Username:");
                ui.label(egui::RichText::new(username).strong());
            });

            if let Some(user_id) = state.user_id {
                ui.horizontal(|ui| {
                    ui.label("🆔 ID:");
                    ui.label(egui::RichText::new(&user_id.to_string()[..8]).code());
                });
            }
        });
    }

    fn show_configuration(&self, ui: &mut egui::Ui, state: &mut AppState) {
        ui.group(|ui| {
            ui.label(egui::RichText::new("Configurazione").strong());
            ui.separator();

            ui.horizontal(|ui| {
                ui.label("🌍 Server:");
                ui.add(egui::TextEdit::singleline(&mut state.base).desired_width(200.0));
            });

            ui.add_space(8.0);

            self.show_websocket_config(ui, state);
        });
    }

    fn show_websocket_config(&self, ui: &mut egui::Ui, state: &mut AppState) {
        ui.horizontal(|ui| {
            ui.label("🔌 WebSocket:");
            match state.ws_status {
                WsStatus::Connected => ui.colored_label(egui::Color32::GREEN, "Connesso"),
                WsStatus::Connecting => ui.colored_label(egui::Color32::YELLOW, "Connessione..."),
                WsStatus::Disconnected => ui.colored_label(egui::Color32::RED, "Disconnesso"),
            };

            if state.ws_status == WsStatus::Disconnected {
                if ui.small_button("🔄").on_hover_text("Riconnetti").clicked() {
                    state.request_ws_reconnect = true;
                }
            }
        });
    }

    fn show_statistics(&self, ui: &mut egui::Ui, state: &AppState) {
        if let Some(ref conversations) = state.conversations {
            ui.group(|ui| {
                ui.label(egui::RichText::new("Statistiche").strong());
                ui.separator();

                let groups = conversations.iter().filter(|c| c.kind == "group").count();
                let dms = conversations.iter().filter(|c| c.kind == "dm").count();
                let total_messages = state.conversation_messages.values().map(|msgs| msgs.len()).sum::<usize>();

                ui.horizontal(|ui| {
                    ui.label("👥 Gruppi:");
                    ui.label(egui::RichText::new(groups.to_string()).strong());
                });
                ui.horizontal(|ui| {
                    ui.label("💬 Chat private:");
                    ui.label(egui::RichText::new(dms.to_string()).strong());
                });
                ui.horizontal(|ui| {
                    ui.label("📝 Messaggi totali:");
                    ui.label(egui::RichText::new(total_messages.to_string()).strong());
                });
            });
        }
    }

    fn show_logout_button(&self, ui: &mut egui::Ui, state: &mut AppState) {
        if ui.button(egui::RichText::new("🚪 Logout").color(egui::Color32::WHITE))
            .on_hover_text("Esci dall'applicazione")
            .clicked() {
            if let Some(token) = &state.token {
                let base = state.base.clone();
                let token = token.clone();
                let tx = state.ui_tx.clone();
                state.rt.spawn(async move {
                    if let Err(e) = crate::api::auth::logout(&base, &token).await {
                        let _ = tx.send(UiEvent::Info(format!("logout note: {e}")));
                    }
                    let _ = tx.send(UiEvent::LoggedOut);
                });
            }
        }
    }
}