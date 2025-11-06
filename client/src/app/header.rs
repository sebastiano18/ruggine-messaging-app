use eframe::egui;
use egui::{Align, Layout};
use crate::models::WsStatus;
use crate::state::AppState;
use crate::app::sidebar_components::sidebar_account::AccountSidebar;

pub struct HeaderManager {
    account_sidebar: AccountSidebar,
}

impl HeaderManager {
    pub fn new() -> Self {
        Self {
            account_sidebar: AccountSidebar::new(),
        }
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
                        // User info se autenticato
                        if state.is_authenticated() {

                            ui.add_space(8.0);

                            if state.is_loading && !state.is_initial_load_complete {
                                ui.label("Caricamento...");
                                ui.spinner();
                            }

                            // Stato WebSocket
                            self.show_websocket_status(ui, state);
                            ui.separator();

                            // Usa un bottone invisibile per avere il cursore corretto
                            let button = egui::Button::new(
                                egui::RichText::new(format!("{} {}", egui_remixicon::icons::USER_FILL, state.username)).size(18.0))
                                .fill(egui::Color32::from_rgb(255, 140, 60).linear_multiply(0.06));

                            if ui.add(button)
                                .on_hover_cursor(egui::CursorIcon::PointingHand)
                                .on_hover_text("Impostazioni account")
                                .clicked() 
                            {
                                state.show_account_modal = true;
                            }
                        }
                    });
                });
            });
    }

    fn show_app_title(&self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
                ui.label(egui::RichText::new(format!("{}", egui_remixicon::icons::CHAT_SMILE_FILL)).size(30.0).color(egui::Color32::from_rgb(200, 100, 40))); 
            ui.label(egui::RichText::new("Ruggine Chat").size(24.0).strong());
        });
    }

    fn show_websocket_status(&self, ui: &mut egui::Ui, state: &AppState) {
        match state.ws_status {
            WsStatus::Connected => {
                ui.label(
                    egui::RichText::new(egui_remixicon::icons::CHECKBOX_CIRCLE_FILL)
                        .size(20.0)
                        .color(egui::Color32::GREEN)
                ).on_hover_text("WebSocket connesso");
            },
            WsStatus::Connecting => {
                ui.label(
                    egui::RichText::new(egui_remixicon::icons::REFRESH_FILL)
                        .size(20.0)
                        .color(egui::Color32::YELLOW)
                ).on_hover_text("Connessione in corso...");
            },
            WsStatus::Disconnected => {
                ui.label(
                    egui::RichText::new(egui_remixicon::icons::CLOSE_CIRCLE_FILL)
                        .size(20.0)
                        .color(egui::Color32::RED)
                ).on_hover_text("WebSocket disconnesso");
            },
        }
    }

    pub fn show_account_popup_content(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        self.account_sidebar.show(ui, state);
    }
}