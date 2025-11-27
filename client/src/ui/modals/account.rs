use eframe::egui;
use crate::app::events::sequence_handler::SequenceHandler;
use crate::models::{UiEvent, WsStatus};
use crate::state::AppState;

/// Modal completo per la gestione dell'account
/// Include: info utente, statistiche, configurazione, sincronizzazione, logout, eliminazione
pub struct AccountModal;

impl AccountModal {
    pub fn new() -> Self {
        Self
    }

    /// Mostra il modal account completo con window wrapper
    pub fn show_modal(&mut self, ctx: &egui::Context, state: &mut AppState) {
        if !state.show_account_modal {
            return;
        }

        let mut open = true;
        let mut should_close = false;

        egui::Window::new("")
            .id(egui::Id::new("account_modal_popup"))
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .fixed_size([500.0, 580.0])
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .open(&mut open)
            .show(ctx, |ui| {
                // Bottone X in alto a destra
                ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                    let close_button = egui::Button::new(
                        egui::RichText::new(egui_remixicon::icons::CLOSE_LINE).size(20.0)
                    ).frame(false);

                    if ui.add(close_button).on_hover_text("Chiudi").clicked() {
                        should_close = true;
                    }
                });

                // Titolo centrato
                ui.vertical_centered(|ui| {
                    ui.add_space(5.0);
                    ui.label(
                        egui::RichText::new(format!("{} Impostazioni Account", egui_remixicon::icons::SETTINGS_4_FILL))
                            .size(26.0)
                            .strong()
                            .color(egui::Color32::from_rgb(200, 100, 40))
                    );
                });

                ui.add_space(20.0);

                // Contenuto del modal
                egui::Frame::none()
                    .inner_margin(egui::Margin::symmetric(40.0, 0.0))
                    .show(ui, |ui| {
                        self.show_content(ui, state);
                    });

                ui.add_space(25.0);

                // Bottoni azione
                self.show_action_buttons(ui, state);
            });

        if should_close {
            open = false;
        }

        state.show_account_modal = open;
    }

    /// Mostra il contenuto principale del modal
    fn show_content(&self, ui: &mut egui::Ui, state: &mut AppState) {
        // Informazioni utente
        self.show_user_info(ui, state);
        ui.add_space(20.0);

        // Statistiche
        self.show_statistics(ui, state);
        ui.add_space(20.0);

        // Configurazione server
        self.show_server_config(ui, state);
        ui.add_space(20.0);

        // Stato WebSocket
        self.show_websocket_status(ui, state);
        ui.add_space(20.0);

        // Stato sincronizzazione
        self.show_sync_status(ui, state);
    }

    fn show_user_info(&self, ui: &mut egui::Ui, state: &AppState) {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(egui_remixicon::icons::USER_FILL)
                    .size(32.0)
                    .color(egui::Color32::from_rgb(200, 100, 40))
            );
            ui.add_space(16.0);
            ui.vertical(|ui| {
                ui.label(
                    egui::RichText::new("Username")
                        .size(12.0)
                        .color(ui.visuals().weak_text_color())
                );
                ui.label(
                    egui::RichText::new(&state.username)
                        .size(18.0)
                        .strong()
                );
            });
        });
    }

    fn show_statistics(&self, ui: &mut egui::Ui, state: &AppState) {
        if let Some(ref conversations) = state.conversations {
            let groups = conversations.iter().filter(|c| c.kind == "group").count();
            let dms = conversations.iter().filter(|c| c.kind == "dm").count();

            ui.horizontal(|ui| {
                // Gruppi
                ui.label(
                    egui::RichText::new(egui_remixicon::icons::TEAM_FILL)
                        .size(28.0)
                        .color(egui::Color32::from_rgb(200, 100, 40))
                );
                ui.add_space(12.0);
                ui.vertical(|ui| {
                    ui.label(
                        egui::RichText::new("Gruppi")
                            .size(12.0)
                            .color(ui.visuals().weak_text_color())
                    );
                    ui.label(
                        egui::RichText::new(groups.to_string())
                            .size(20.0)
                            .strong()
                    );
                });

                ui.add_space(40.0);

                // Chat private
                ui.label(
                    egui::RichText::new(egui_remixicon::icons::CHAT_1_FILL)
                        .size(28.0)
                        .color(egui::Color32::from_rgb(200, 100, 40))
                );
                ui.add_space(12.0);
                ui.vertical(|ui| {
                    ui.label(
                        egui::RichText::new("Chat Private")
                            .size(12.0)
                            .color(ui.visuals().weak_text_color())
                    );
                    ui.label(
                        egui::RichText::new(dms.to_string())
                            .size(20.0)
                            .strong()
                    );
                });
            });
        }
    }

    fn show_server_config(&self, ui: &mut egui::Ui, state: &AppState) {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(egui_remixicon::icons::GLOBAL_FILL)
                    .size(28.0)
                    .color(egui::Color32::from_rgb(200, 100, 40))
            );
            ui.add_space(16.0);
            ui.vertical(|ui| {
                ui.label(
                    egui::RichText::new("Server")
                        .size(12.0)
                        .color(ui.visuals().weak_text_color())
                );
                ui.label(
                    egui::RichText::new(&state.base)
                        .size(14.0)
                        .font(egui::FontId::monospace(14.0))
                );
            });
        });
    }

    fn show_websocket_status(&self, ui: &mut egui::Ui, state: &mut AppState) {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(egui_remixicon::icons::WIFI_FILL)
                    .size(28.0)
                    .color(egui::Color32::from_rgb(200, 100, 40))
            );
            ui.add_space(16.0);
            ui.vertical(|ui| {
                ui.label(
                    egui::RichText::new("WebSocket")
                        .size(12.0)
                        .color(ui.visuals().weak_text_color())
                );
                ui.horizontal(|ui| {
                    match state.ws_status {
                        WsStatus::Connected => {
                            ui.label(
                                egui::RichText::new(egui_remixicon::icons::CHECKBOX_CIRCLE_FILL)
                                    .size(18.0)
                                    .color(egui::Color32::GREEN)
                            );
                            ui.label(egui::RichText::new("Connesso").size(14.0));
                        },
                        WsStatus::Connecting => {
                            ui.label(
                                egui::RichText::new(egui_remixicon::icons::REFRESH_FILL)
                                    .size(18.0)
                                    .color(egui::Color32::YELLOW)
                            );
                            ui.label(egui::RichText::new("Connessione...").size(14.0));
                        },
                        WsStatus::Disconnected => {
                            ui.label(
                                egui::RichText::new(egui_remixicon::icons::CLOSE_CIRCLE_FILL)
                                    .size(18.0)
                                    .color(egui::Color32::RED)
                            );
                            ui.label(egui::RichText::new("Disconnesso").size(14.0));
                            ui.add_space(8.0);
                            if ui.small_button(egui_remixicon::icons::REFRESH_LINE)
                                .on_hover_text("Riconnetti")
                                .clicked()
                            {
                                state.request_ws_reconnect = true;
                            }
                        },
                    }
                });
            });
        });
    }

    fn show_sync_status(&self, ui: &mut egui::Ui, state: &AppState) {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(egui_remixicon::icons::REFRESH_FILL)
                    .size(28.0)
                    .color(egui::Color32::from_rgb(200, 100, 40))
            );
            ui.add_space(16.0);
            ui.vertical(|ui| {
                ui.label(
                    egui::RichText::new("Sincronizzazione")
                        .size(12.0)
                        .color(ui.visuals().weak_text_color())
                );

                let health = SequenceHandler::get_sequence_health(&state);
                let health_color = if health > 0.9 {
                    egui::Color32::GREEN
                } else if health > 0.7 {
                    egui::Color32::YELLOW
                } else {
                    egui::Color32::RED
                };

                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(format!("{:.0}%", health * 100.0))
                            .size(16.0)
                            .strong()
                            .color(health_color)
                    );
                    ui.label(
                        egui::RichText::new("salute")
                            .size(12.0)
                            .color(ui.visuals().weak_text_color())
                    );
                });

                if state.user_sequence_confirmed > 0 {
                    ui.label(
                        egui::RichText::new(format!("Ultimo evento: #{}", state.user_sequence_confirmed))
                            .size(11.0)
                            .color(ui.visuals().weak_text_color())
                    );
                }

                // Mostra gap se presente
                let user_gap = if state.user_sequence_received > state.user_sequence_confirmed {
                    state.user_sequence_received - state.user_sequence_confirmed
                } else {
                    0
                };
                if user_gap > 0 {
                    ui.label(
                        egui::RichText::new(format!("⚠ Gap: {} eventi", user_gap))
                            .size(11.0)
                            .color(egui::Color32::YELLOW)
                    );
                }
            });
        });
    }

    fn show_action_buttons(&self, ui: &mut egui::Ui, state: &mut AppState) {
        egui::Frame::none()
            .inner_margin(egui::Margin::symmetric(40.0, 0.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    // Logout a sinistra
                    let logout_btn = egui::Button::new(
                        egui::RichText::new(format!("{} Logout", egui_remixicon::icons::LOGOUT_BOX_R_FILL))
                            .size(14.0)
                            .color(egui::Color32::WHITE)
                    )
                    .fill(egui::Color32::from_rgb(200, 100, 40))
                    .min_size(egui::vec2(180.0, 40.0));

                    if ui.add(logout_btn).clicked() {
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

                    // Elimina account a destra
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let delete_btn = egui::Button::new(
                            egui::RichText::new(format!("{} Elimina Account", egui_remixicon::icons::DELETE_BIN_FILL))
                                .size(14.0)
                                .color(egui::Color32::WHITE)
                        )
                        .fill(egui::Color32::from_rgb(180, 50, 50))
                        .min_size(egui::vec2(180.0, 40.0));

                        if ui.add(delete_btn).clicked() {
                            let _ = state.ui_tx.send(UiEvent::DeleteAccountStart);
                        }
                    });
                });
            });
    }
}
