use crate::state::AppState;
use eframe::egui;
use egui::{Align2, RichText, Stroke};

/// Mostra il popup per scegliere tra creare un gruppo o un DM
pub fn show_action_selection_popup(
    ctx: &egui::Context,
    state: &mut AppState,
    show_action_popup: &mut bool,
    show_create_dm_popup: &mut bool,
) {
    if !*show_action_popup {
        return;
    }

    let mut open = true;

    egui::Window::new("")
        .id(egui::Id::new("action_selection_popup"))
        .collapsible(false)
        .resizable(false)
        .title_bar(false)
        .fixed_size([450.0, 320.0])
        .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
        .open(&mut open)
        .show(ctx, |ui| {
            // Bottone X in alto a destra
            ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                let close_button = egui::Button::new(
                    RichText::new(egui_remixicon::icons::CLOSE_LINE)
                        .size(18.0)
                )
                    .frame(false);

                if ui.add(close_button).on_hover_text("Chiudi").clicked() {
                    *show_action_popup = false;
                }
            });

            ui.vertical_centered(|ui| {
                ui.add_space(5.0);
                ui.label(
                    RichText::new(format!("{} Nuova Conversazione", egui_remixicon::icons::CHAT_NEW_FILL))
                        .size(24.0)
                        .strong()
                        .color(egui::Color32::from_rgb(200, 100, 40))
                );
                ui.add_space(8.0);
                ui.label(
                    RichText::new("Scegli il tipo di conversazione da creare")
                        .size(13.0)
                        .color(ui.visuals().weak_text_color())
                );
            });

            ui.add_space(30.0);

            egui::Frame::none()
                .inner_margin(egui::Margin::symmetric(40.0, 0.0))
                .show(ui, |ui| {
                    let group_frame = egui::Frame::none()
                        .stroke(Stroke::new(1.0, egui::Color32::from_rgb(200, 100, 40).linear_multiply(0.4)))
                        .rounding(egui::Rounding::same(12.0))
                        .inner_margin(20.0);

                    let group_response = group_frame.show(ui, |ui| {
                        ui.set_min_width(340.0);
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(egui_remixicon::icons::TEAM_FILL)
                                    .size(32.0)
                                    .color(egui::Color32::from_rgb(200, 100, 40))
                            );
                            ui.add_space(16.0);
                            ui.vertical(|ui| {
                                ui.label(
                                    RichText::new("Crea Gruppo")
                                        .size(18.0)
                                        .strong()
                                        .color(egui::Color32::from_rgb(200, 100, 40))
                                );
                                ui.add_space(4.0);
                                ui.label(
                                    RichText::new("Avvia una conversazione di gruppo con più partecipanti")
                                        .size(12.0)
                                        .color(ui.visuals().weak_text_color())
                                );
                            });
                        });
                    });

                    let group_rect = group_response.response.rect;
                    let group_hovered = ui.rect_contains_pointer(group_rect);

                    if group_hovered {
                        ui.painter().rect_stroke(
                            group_rect,
                            12.0,
                            Stroke::new(2.0, egui::Color32::from_rgb(200, 100, 40))
                        );
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }

                    if group_response.response.interact(egui::Sense::click()).clicked() {
                        state.show_create_group_modal = true;
                        *show_action_popup = false;
                    }

                    ui.add_space(16.0);

                    let dm_frame = egui::Frame::none()
                        .stroke(Stroke::new(1.0, egui::Color32::from_rgb(200, 100, 40).linear_multiply(0.4)))
                        .rounding(egui::Rounding::same(12.0))
                        .inner_margin(20.0);

                    let dm_response = dm_frame.show(ui, |ui| {
                        ui.set_min_width(340.0);
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(egui_remixicon::icons::CHAT_1_FILL)
                                    .size(32.0)
                                    .color(egui::Color32::from_rgb(200, 100, 40))
                            );
                            ui.add_space(16.0);
                            ui.vertical(|ui| {
                                ui.label(
                                    RichText::new("Messaggio Privato")
                                        .size(18.0)
                                        .strong()
                                        .color(egui::Color32::from_rgb(200, 100, 40))
                                );
                                ui.add_space(4.0);
                                ui.label(
                                    RichText::new("Invia un messaggio diretto a un singolo utente")
                                        .size(12.0)
                                        .color(ui.visuals().weak_text_color())
                                );
                            });
                        });
                    });

                    let dm_rect = dm_response.response.rect;
                    let dm_hovered = ui.rect_contains_pointer(dm_rect);

                    if dm_hovered {
                        ui.painter().rect_stroke(
                            dm_rect,
                            12.0,
                            Stroke::new(2.0, egui::Color32::from_rgb(200, 100, 40))
                        );
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }

                    if dm_response.response.interact(egui::Sense::click()).clicked() {
                        *show_create_dm_popup = true;
                        *show_action_popup = false;
                    }
                });

            ui.add_space(20.0);
        });

    if !open {
        *show_action_popup = false;
    }
}
