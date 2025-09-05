use eframe::egui;
use egui::{Align, Layout};
use crate::models::WsStatus;
use crate::state::AppState;

pub struct HeaderManager;

impl HeaderManager {
    pub fn new() -> Self {
        Self
    }

    pub fn show_header(&mut self, ctx: &egui::Context, state: &mut AppState) {
        egui::TopBottomPanel::top("header")
            .min_height(50.0)
            .show(ctx, |ui| {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    // Titolo app con icona
                    self.show_app_title(ui);

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        // Status indicators
                        self.show_websocket_status(ui, state);

                        // User info se autenticato
                        self.show_user_info(ui, state);
                    });
                });
                ui.add_space(4.0);
            });
    }

    fn show_app_title(&self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("🦀").size(24.0));
            ui.label(egui::RichText::new("Ruggine Chat").size(20.0).strong());
        });
    }

    fn show_websocket_status(&self, ui: &mut egui::Ui, state: &AppState) {
        match state.ws_status {
            WsStatus::Connected => {
                ui.colored_label(egui::Color32::GREEN, "🟢")
                    .on_hover_text("WebSocket connesso");
            },
            WsStatus::Connecting => {
                ui.colored_label(egui::Color32::YELLOW, "🟡")
                    .on_hover_text("Connessione in corso...");
            },
            WsStatus::Disconnected => {
                ui.colored_label(egui::Color32::RED, "🔴")
                    .on_hover_text("WebSocket disconnesso");
            },
        };
    }

    fn show_user_info(&self, ui: &mut egui::Ui, state: &AppState) {
        if let Some(ref username) = state.token.as_ref().map(|_| &state.username) {
            ui.separator();
            ui.label(format!("👤 {}", username));
            if state.is_loading && !state.is_initial_load_complete {
                ui.spinner();
                ui.label("Caricamento...");
            }
        }
    }
}