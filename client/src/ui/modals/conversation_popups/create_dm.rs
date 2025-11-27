use crate::models::UiEvent;
use crate::state::AppState;
use eframe::egui;
use egui::{Align2, RichText, TextEdit};

/// Mostra il popup per creare un nuovo DM
pub fn show_create_dm_popup(
    ctx: &egui::Context,
    state: &mut AppState,
    show_create_dm_popup: &mut bool,
    dm_username: &mut String,
) {
    if !*show_create_dm_popup {
        return;
    }

    let mut open = true;

    egui::Window::new("")
        .id(egui::Id::new("create_dm_popup"))
        .collapsible(false)
        .resizable(false)
        .title_bar(false)
        .fixed_size([450.0, 280.0])
        .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
        .open(&mut open)
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(20.0);
                ui.label(
                    RichText::new(format!("{} Nuova Conversazione Privata", egui_remixicon::icons::CHAT_1_FILL))
                        .size(24.0)
                        .strong()
                        .color(egui::Color32::from_rgb(200, 100, 40))
                );
            });

            ui.add_space(30.0);

            egui::Frame::none()
                .inner_margin(egui::Margin::symmetric(30.0, 0.0))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.add_space(8.0);
                        ui.label(RichText::new(egui_remixicon::icons::USER_FILL).size(20.0).color(egui::Color32::from_rgb(200, 100, 40)));
                        ui.add_space(12.0);
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("Username destinatario").size(13.0).weak());
                                ui.label(RichText::new("*").size(14.0).color(ui.visuals().error_fg_color));
                            });

                            let available_width = ui.available_width() - 40.0;
                            let username_field = TextEdit::singleline(dm_username)
                                .hint_text("Inserisci username...")
                                .desired_width(available_width);

                            ui.add(username_field);
                        });
                    });

                    ui.add_space(30.0);

                    ui.horizontal(|ui| {
                        ui.add_space(8.0);

                        // Bottone Annulla
                        let cancel_btn = egui::Button::new(
                            RichText::new(format!("{} Annulla", egui_remixicon::icons::CLOSE_LINE))
                                .size(14.0)
                        )
                            .min_size(egui::vec2(120.0, 36.0))
                            .rounding(egui::Rounding::same(8.0));

                        if ui.add(cancel_btn).clicked() {
                            dm_username.clear();
                            *show_create_dm_popup = false;
                        }

                        ui.add_space(ui.available_width() - 140.0);

                        // Se stiamo verificando l'utente, mostra spinner
                        if state.is_checking_user() {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label(
                                    RichText::new(format!(
                                        "Verifica '{}'...",
                                        state.pending_user_check.as_ref().unwrap_or(&String::new())
                                    ))
                                        .size(14.0)
                                        .color(egui::Color32::from_rgb(200, 100, 40))
                                );
                            });
                        } else {
                            // Bottone Crea
                            let can_create = !dm_username.trim().is_empty();
                            let create_btn = egui::Button::new(
                                RichText::new(format!("{} Crea", egui_remixicon::icons::CHAT_NEW_LINE))
                                    .size(14.0)
                                    .color(if can_create {
                                        egui::Color32::WHITE
                                    } else {
                                        egui::Color32::GRAY
                                    })
                            )
                                .fill(if can_create {
                                    egui::Color32::from_rgb(200, 100, 40)
                                } else {
                                    egui::Color32::from_gray(100)
                                })
                                .min_size(egui::vec2(140.0, 36.0))
                                .rounding(egui::Rounding::same(8.0));

                            if ui.add_enabled(can_create, create_btn).clicked() {
                                let username = dm_username.trim().to_string();

                                // Controlla se esiste già una conversazione DM con questo utente (case-insensitive)
                                let existing_dm = state.conversations.as_ref().and_then(|convs| {
                                    convs.iter().find(|conv| {
                                        conv.kind == "dm" && conv.title.to_lowercase() == username.to_lowercase()
                                    })
                                });

                                if let Some(existing_conv) = existing_dm {
                                    // Apri la conversazione esistente
                                    let _ = state.ui_tx.send(UiEvent::Opened(existing_conv.id));
                                    dm_username.clear();
                                    *show_create_dm_popup = false;
                                } else {
                                    // Usa request_dm_creation che valida l'utente via WebSocket
                                    // Lo stub verrà creato automaticamente se l'utente esiste
                                    state.request_dm_creation(username);
                                    dm_username.clear();
                                    *show_create_dm_popup = false;
                                }
                            }
                        }
                    });
                });
        });

    if !open {
        dm_username.clear();
        *show_create_dm_popup = false;
    }
}
