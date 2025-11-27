use crate::state::AppState;
use eframe::egui;
use egui::{Align2, RichText};

/// Mostra il popup di conferma eliminazione/uscita conversazione
pub fn show_delete_confirmation_popup(ctx: &egui::Context, state: &mut AppState) {
    let Some(pending) = state.pending_deletion.clone() else {
        return;
    };

    let conversation = pending.conversation;
    let mut confirm = false;
    let mut cancel = false;

    let is_stub = state.is_dm_stub(conversation.id);
    let is_owner = state.user_id.map_or(false, |uid| uid == conversation.owner_id);

    let (icon, title, main_message, detail_message, confirm_label) = if is_stub {
        (
            egui_remixicon::icons::DELETE_BIN_FILL,
            "Elimina chat locale",
            format!("Eliminare la chat privata \"{}\"?", conversation.title),
            "Si tratta di uno stub locale: verrà semplicemente rimosso dalla tua lista.",
            "Elimina"
        )
    } else if conversation.kind == "group" {
        if is_owner {
            (
                egui_remixicon::icons::DELETE_BIN_FILL,
                "Elimina gruppo",
                format!("Eliminare il gruppo \"{}\"?", conversation.title),
                "Attenzione: eliminando il gruppo, questo verrà rimosso per TUTTI i partecipanti. L'azione è irreversibile.",
                "Elimina per tutti"
            )
        } else {
            (
                egui_remixicon::icons::LOGOUT_BOX_LINE,
                "Esci dal gruppo",
                format!("Uscire dal gruppo \"{}\"?", conversation.title),
                "Uscirai dal gruppo e non potrai più vedere i messaggi. Potrai rientrare solo se verrai invitato nuovamente.",
                "Esci dal gruppo"
            )
        }
    } else {
        (
            egui_remixicon::icons::DELETE_BIN_FILL,
            "Elimina conversazione",
            format!("Eliminare la chat privata \"{}\"?", conversation.title),
            "L'eliminazione rimuoverà definitivamente la conversazione. L'azione è irreversibile.",
            "Elimina"
        )
    };

    egui::Window::new("")
        .id(egui::Id::new("delete_confirmation_popup"))
        .title_bar(false)
        .collapsible(false)
        .resizable(false)
        .fixed_size([450.0, 240.0])
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            // Titolo centrato
            ui.vertical_centered(|ui| {
                ui.add_space(20.0);
                ui.label(
                    RichText::new(format!("{} {}", icon, title))
                        .size(22.0)
                        .strong()
                        .color(egui::Color32::from_rgb(200, 100, 40))
                );
                ui.add_space(8.0);
                ui.label(
                    RichText::new(main_message)
                        .size(14.0)
                        .strong()
                );
                ui.add_space(6.0);
                ui.label(
                    RichText::new(detail_message)
                        .size(12.0)
                        .color(ui.visuals().weak_text_color())
                );
            });

            ui.add_space(30.0);

            // Bottoni ai lati
            egui::Frame::none()
                .inner_margin(egui::Margin::symmetric(20.0, 0.0))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let cancel_button = egui::Button::new(
                            RichText::new(format!("{} Annulla", egui_remixicon::icons::CLOSE_LINE))
                                .size(14.0)
                        )
                            .fill(ui.visuals().widgets.inactive.bg_fill)
                            .min_size(egui::vec2(140.0, 36.0));

                        if ui.add(cancel_button).clicked() {
                            cancel = true;
                        }

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let confirm_button = egui::Button::new(
                                RichText::new(format!("{} {}", icon, confirm_label))
                                    .size(14.0)
                                    .color(egui::Color32::WHITE)
                            )
                                .fill(egui::Color32::from_rgb(200, 100, 40))
                                .min_size(egui::vec2(160.0, 36.0));

                            if ui.add(confirm_button).clicked() {
                                confirm = true;
                            }
                        });
                    });
                });
        });

    if confirm {
        state.execute_pending_deletion();
    } else if cancel {
        state.cancel_delete_confirmation();
    }
}