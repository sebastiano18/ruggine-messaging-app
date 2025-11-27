use crate::state::AppState;
use eframe::egui;
use egui::{Align2, RichText, Stroke, TextEdit};

pub fn show_invite_popup(
    ctx: &egui::Context,
    state: &mut AppState,
    cid: uuid::Uuid,
) {
    if !state.show_invite_popup {
        return;
    }

    let mut open = true;
    let mut should_close = false;

    egui::Window::new("")
        .id(egui::Id::new("invite_popup"))
        .collapsible(false)
        .resizable(false)
        .title_bar(false)
        .fixed_size([500.0, 520.0])
        .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
        .open(&mut open)
        .show(ctx, |ui| {
            // Titolo centrato
            ui.vertical_centered(|ui| {
                ui.add_space(20.0);
                ui.label(
                    RichText::new(format!("{} Aggiungi Membri al Gruppo", egui_remixicon::icons::USER_ADD_FILL))
                        .size(24.0)
                        .strong()
                        .color(egui::Color32::from_rgb(200, 100, 40))
                );
                ui.add_space(8.0);
                ui.label(
                    RichText::new("Seleziona gli utenti da invitare al gruppo")
                        .size(13.0)
                        .color(ui.visuals().weak_text_color())
                );
            });

            ui.add_space(20.0);

            // Contenuto con margini
            egui::Frame::none()
                .inner_margin(egui::Margin::symmetric(30.0, 0.0))
                .show(ui, |ui| {
                    // Recupera i membri già presenti nel gruppo
                    let existing_members: std::collections::HashSet<String> = state.members_list
                        .get(&cid)
                        .map(|members| {
                            members.iter()
                                .map(|m| m.username.clone())
                                .collect()
                        })
                        .unwrap_or_default();

                    // Calcola se mostrare il bottone aggiungi
                    let search_text = state.invite_popup.search_query.trim().to_string();
                    let dm_conversations: Vec<_> = state.conversations
                        .as_ref()
                        .map(|convs| convs.iter().filter(|c| c.kind == "dm").collect())
                        .unwrap_or_default();

                    let search_lower = search_text.to_lowercase();
                    let found_in_contacts = dm_conversations
                        .iter()
                        .any(|c| c.title.to_lowercase().starts_with(&search_lower));

                    let show_add_button = !search_text.is_empty()
                        && !found_in_contacts
                        && !state.invite_popup.selected_users.contains(&search_text)
                        && !existing_members.contains(&search_text); // Non mostrare se già membro

                    // Barra di ricerca con bottone aggiungi
                    ui.horizontal(|ui| {
                        ui.add_space(8.0);
                        ui.label(RichText::new(egui_remixicon::icons::SEARCH_LINE).size(20.0));
                        ui.add_space(12.0);
                        ui.vertical(|ui| {
                            ui.label(RichText::new("Cerca o aggiungi utenti").size(13.0).weak());

                            ui.horizontal(|ui| {
                                let available_width = ui.available_width();
                                let search_response = ui.add(
                                    TextEdit::singleline(&mut state.invite_popup.search_query)
                                        .hint_text("Cerca nei contatti o scrivi username...")
                                        .desired_width(available_width - 90.0)
                                );

                                // Enter per aggiungere
                                if search_response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                                    if show_add_button && !state.is_checking_user() {
                                        state.invite_popup.pending_user_verification = Some(search_text.clone());
                                        state.request_dm_creation(search_text.clone());
                                        state.invite_popup.search_query.clear();
                                    }
                                }

                                // Mostra spinner se stiamo verificando
                                if state.is_checking_user() && state.invite_popup.pending_user_verification.is_some() {
                                    ui.horizontal(|ui| {
                                        ui.spinner();
                                        ui.label(
                                            RichText::new("Verifica...")
                                                .size(12.0)
                                                .color(egui::Color32::from_rgb(200, 100, 40))
                                        );
                                    });
                                } else {
                                    // Bottone aggiungi
                                    let add_button = egui::Button::new(
                                        RichText::new(format!("{} Aggiungi", egui_remixicon::icons::ADD_LINE))
                                            .size(12.0)
                                    )
                                        .fill(if show_add_button {
                                            egui::Color32::from_rgb(200, 100, 40)
                                        } else {
                                            egui::Color32::TRANSPARENT
                                        })
                                        .min_size(egui::vec2(70.0, 24.0));

                                    let button_response = ui.add_enabled(show_add_button, add_button);

                                    if !show_add_button {
                                        button_response.surrender_focus();
                                    }

                                    if show_add_button && button_response.clicked() {
                                        state.invite_popup.pending_user_verification = Some(search_text.clone());
                                        state.request_dm_creation(search_text.clone());
                                        state.invite_popup.search_query.clear();
                                    }
                                }
                            });

                            // Info sotto la barra di ricerca
                            if show_add_button && !state.is_checking_user() {
                                ui.add_space(3.0);
                                ui.label(
                                    RichText::new(format!("💡 '{}' non trovato nei contatti, clicca Aggiungi o premi Invio", search_text))
                                        .size(10.0)
                                        .color(egui::Color32::from_rgb(160, 100, 60))
                                );
                            }
                        });
                    });

                    ui.add_space(16.0);

                    // Scroll area contatti disponibili
                    if let Some(ref conversations) = state.conversations {
                        let dm_conversations: Vec<_> = conversations
                            .iter()
                            .filter(|c| c.kind == "dm")
                            .collect();

                        let search_lower = state.invite_popup.search_query.trim().to_lowercase();

                        // Filtra i DM escludendo i membri già presenti nel gruppo
                        let filtered_dms: Vec<_> = dm_conversations
                            .iter()
                            .filter(|c| {
                                let username = &c.title;
                                let matches_search = if search_lower.is_empty() {
                                    true
                                } else {
                                    username.to_lowercase().starts_with(&search_lower)
                                };

                                // Mostra solo se matcha la ricerca E non è già membro del gruppo
                                matches_search && !existing_members.contains(username)
                            })
                            .collect();

                        ui.label(
                            RichText::new("I tuoi contatti")
                                .size(13.0)
                                .strong()
                                .color(egui::Color32::GRAY)
                        );
                        ui.add_space(6.0);

                        egui::Frame::none()
                            .stroke(Stroke::new(1.0, egui::Color32::from_gray(60)))
                            .inner_margin(10.0)
                            .rounding(6.0)
                            .show(ui, |ui| {
                                let fixed_height = 100.0;

                                egui::ScrollArea::vertical()
                                    .max_height(fixed_height)
                                    .min_scrolled_height(fixed_height)
                                    .id_source("invite_contacts_scroll")
                                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
                                    .show(ui, |ui| {
                                        ui.set_min_width(ui.available_width());
                                        ui.set_min_height(fixed_height);

                                        if filtered_dms.is_empty() {
                                            if dm_conversations.is_empty() {
                                                ui.vertical_centered(|ui| {
                                                    ui.add_space(fixed_height / 2.0 - 20.0);
                                                    ui.label(
                                                        RichText::new("Nessun contatto trovato")
                                                            .italics()
                                                            .color(egui::Color32::GRAY)
                                                    );
                                                    ui.add_space(4.0);
                                                    ui.label(
                                                        RichText::new("Scrivi un username e premi Aggiungi")
                                                            .size(11.0)
                                                            .color(egui::Color32::GRAY)
                                                    );
                                                });
                                            } else if !existing_members.is_empty() && dm_conversations.len() <= existing_members.len() {
                                                // Tutti i contatti sono già membri
                                                ui.vertical_centered(|ui| {
                                                    ui.add_space(fixed_height / 2.0 - 10.0);
                                                    ui.label(
                                                        RichText::new("Tutti i contatti sono già membri del gruppo")
                                                            .italics()
                                                            .color(egui::Color32::GRAY)
                                                    );
                                                });
                                            } else {
                                                // Nessun risultato per la ricerca
                                                ui.vertical_centered(|ui| {
                                                    ui.add_space(fixed_height / 2.0 - 10.0);
                                                    ui.label(
                                                        RichText::new("Nessun contatto corrisponde alla ricerca")
                                                            .italics()
                                                            .color(egui::Color32::GRAY)
                                                    );
                                                });
                                            }
                                        } else {
                                            ui.spacing_mut().item_spacing.y = 6.0;

                                            for dm in filtered_dms {
                                                let username = &dm.title;
                                                let is_selected = state.invite_popup.selected_users.contains(username);

                                                ui.horizontal(|ui| {
                                                    let mut selected = is_selected;
                                                    if ui.checkbox(&mut selected, "").changed() {
                                                        if selected {
                                                            state.invite_popup.selected_users.insert(username.clone());
                                                        } else {
                                                            state.invite_popup.selected_users.remove(username);
                                                        }
                                                    }

                                                    ui.add_space(4.0);

                                                    if is_selected {
                                                        egui::Frame::none()
                                                            .fill(egui::Color32::from_rgb(200, 100, 40))
                                                            .inner_margin(egui::Margin::symmetric(8.0, 4.0))
                                                            .rounding(4.0)
                                                            .show(ui, |ui| {
                                                                ui.label(
                                                                    RichText::new(format!("{} {}", egui_remixicon::icons::USER_LINE, username))
                                                                        .size(14.0)
                                                                        .color(egui::Color32::WHITE)
                                                                );
                                                            });
                                                    } else {
                                                        ui.label(
                                                            RichText::new(format!("{} {}", egui_remixicon::icons::USER_LINE, username))
                                                                .size(14.0)
                                                        );
                                                    }
                                                });
                                            }
                                        }
                                    });
                            });
                    }

                    ui.add_space(16.0);

                    // Scroll area utenti selezionati
                    ui.label(
                        RichText::new("Utenti da invitare")
                            .size(13.0)
                            .strong()
                            .color(egui::Color32::from_rgb(200, 100, 40))
                    );
                    ui.add_space(6.0);

                    egui::Frame::none()
                        .stroke(Stroke::new(1.0, egui::Color32::from_rgb(200, 100, 40)))
                        .inner_margin(10.0)
                        .rounding(6.0)
                        .show(ui, |ui| {
                            let fixed_height = 90.0;

                            egui::ScrollArea::vertical()
                                .max_height(fixed_height)
                                .min_scrolled_height(fixed_height)
                                .id_source("selected_users_scroll")
                                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
                                .show(ui, |ui| {
                                    ui.set_min_width(ui.available_width());
                                    ui.set_min_height(fixed_height);

                                    if state.invite_popup.selected_users.is_empty() {
                                        ui.vertical_centered(|ui| {
                                            ui.add_space(fixed_height / 2.0 - 10.0);
                                            ui.label(
                                                RichText::new("Nessun utente selezionato")
                                                    .italics()
                                                    .color(egui::Color32::GRAY)
                                            );
                                        });
                                    } else {
                                        let mut selected_list: Vec<String> = state.invite_popup.selected_users
                                            .iter()
                                            .cloned()
                                            .collect();
                                        selected_list.sort();

                                        ui.spacing_mut().item_spacing.y = 6.0;

                                        for username in selected_list {
                                            ui.horizontal(|ui| {
                                                ui.spacing_mut().item_spacing.x = 4.0;

                                                if ui.small_button(egui_remixicon::icons::CLOSE_LINE).clicked() {
                                                    state.invite_popup.selected_users.remove(&username);
                                                }

                                                egui::Frame::none()
                                                    .fill(egui::Color32::from_rgb(200, 100, 40))
                                                    .inner_margin(egui::Margin::symmetric(8.0, 4.0))
                                                    .rounding(4.0)
                                                    .show(ui, |ui| {
                                                        ui.label(
                                                            RichText::new(format!("{} {}", egui_remixicon::icons::USER_LINE, username))
                                                                .size(14.0)
                                                                .color(egui::Color32::WHITE)
                                                        );
                                                    });
                                            });
                                        }
                                    }
                                });
                        });
                });

            ui.add_space(16.0);

            // Bottoni centrati
            ui.vertical_centered(|ui| {
                let can_invite = !state.invite_popup.selected_users.is_empty();

                ui.horizontal(|ui| {
                    let cancel_button = egui::Button::new(
                        RichText::new(format!("{} Annulla", egui_remixicon::icons::CLOSE_LINE))
                            .size(14.0)
                    )
                        .fill(ui.visuals().widgets.inactive.bg_fill)
                        .min_size(egui::vec2(130.0, 40.0));

                    if ui.add(cancel_button).clicked() {
                        state.invite_popup.reset();
                        should_close = true;
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let invite_button = egui::Button::new(
                            RichText::new(format!("{} Invita Utenti", egui_remixicon::icons::MAIL_SEND_LINE))
                                .size(14.0)
                        )
                            .fill(if can_invite {
                                egui::Color32::from_rgb(200, 100, 40)
                            } else {
                                ui.visuals().widgets.inactive.bg_fill
                            })
                            .min_size(egui::vec2(180.0, 40.0));

                        if ui.add_enabled(can_invite, invite_button).clicked() {
                            let users: Vec<String> = state.invite_popup.selected_users.iter().cloned().collect();
                            state.send_invite_users(cid, users);
                            state.invite_popup.reset();
                            should_close = true;
                        }
                    });
                });
            });

        });

    if should_close {
        open = false;
    }

    state.show_invite_popup = open;
}