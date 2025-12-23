use eframe::egui;
use crate::state::AppState;

/// Modal per la conferma eliminazione account (design originale)
pub struct DeleteAccountModal;

impl DeleteAccountModal {
    pub fn new() -> Self {
        Self
    }

    pub fn show_modal(&mut self, ctx: &egui::Context, state: &mut AppState) {
        if !state.confirm_delete_account {
            return;
        }

        egui::Window::new("")
            .id(egui::Id::new("delete_account_confirmation_popup"))
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .fixed_size([480.0, 260.0])
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                // Titolo centrato
                ui.vertical_centered(|ui| {
                    ui.add_space(20.0);
                    ui.label(
                        egui::RichText::new(format!("{} Elimina Account", egui_remixicon::icons::ALERT_FILL))
                            .size(24.0)
                            .strong()
                            .color(egui::Color32::from_rgb(220, 60, 60))
                    );
                    ui.add_space(12.0);
                    ui.label(
                        egui::RichText::new("Sei sicuro di voler eliminare il tuo account?")
                            .size(15.0)
                            .strong()
                    );
                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new("Questa azione è irreversibile. Tutti i tuoi dati verranno eliminati permanentemente.")
                            .size(13.0)
                            .color(ui.visuals().weak_text_color())
                    );
                });

                ui.add_space(30.0);

                // Bottoni: Annulla a sinistra, Elimina a destra
                ui.horizontal(|ui| {
                    // Annulla (a sinistra)
                    let cancel_btn = egui::Button::new(
                        egui::RichText::new(format!("{} Annulla", egui_remixicon::icons::CLOSE_LINE))
                            .size(14.0)
                    )
                        .fill(ui.visuals().widgets.inactive.bg_fill)
                        .min_size(egui::vec2(150.0, 40.0));

                    if ui.add(cancel_btn).clicked() {
                        let _ = state.ui_tx.send(crate::models::UiEvent::DeleteAccountCancel);
                    }

                    // Spazio flessibile che spinge il secondo bottone a destra
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // Conferma eliminazione (a destra)
                        let confirm_btn = egui::Button::new(
                            egui::RichText::new(format!("{} Elimina Account", egui_remixicon::icons::DELETE_BIN_FILL))
                                .size(14.0)
                                .color(egui::Color32::WHITE)
                        )
                            .fill(egui::Color32::from_rgb(200, 50, 50))
                            .min_size(egui::vec2(180.0, 40.0));

                        if ui.add(confirm_btn).clicked() {
                            let _ = state.ui_tx.send(crate::models::UiEvent::DeleteAccountConfirm);
                        }
                    });
                });
            });
    }
}