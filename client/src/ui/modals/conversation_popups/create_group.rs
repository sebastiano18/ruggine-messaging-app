use crate::state::AppState;
use eframe::egui;
use egui::{ RichText, Stroke, TextEdit};

pub fn show_create_group_popup(ctx: &egui::Context, state: &mut AppState) {
    if !state.show_create_group_modal {
        return;
    }

    let mut open = true;
    let mut should_close = false;

    egui::Window::new("")
        .id(egui::Id::new("create_group_popup"))
        .collapsible(false)
        .resizable(false)
        .title_bar(false)
        .fixed_size([500.0, 520.0])
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .open(&mut open)
        .show(ctx, |ui| {
            // Titolo centrato
            ui.vertical_centered(|ui| {
                ui.add_space(20.0);
                ui.label(
                    RichText::new(format!("{} Crea Nuovo Gruppo", egui_remixicon::icons::TEAM_FILL))
                        .size(24.0)
                        .strong()
                        .color(egui::Color32::from_rgb(200, 100, 40))
                );
            });

            ui.add_space(20.0);

            // Contenuto full width
            egui::Frame::none()
                .inner_margin(20.0)
                .show(ui, |ui| {
                    ui.set_min_width(460.0);

                    // Nome gruppo
                    ui.horizontal(|ui| {
                        ui.add_space(8.0);
                        ui.label(RichText::new(egui_remixicon::icons::EDIT_LINE).size(20.0));
                        ui.add_space(12.0);
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("Nome del gruppo").size(13.0).weak());
                                ui.label(RichText::new("*").size(14.0).color(ui.visuals().error_fg_color));
                            });

                            let available_width = ui.available_width() - 50.0;
                            let name_field = TextEdit::singleline(&mut state.create_group_popup.group_name)
                                .hint_text("Es: Team Alpha")
                                .desired_width(available_width);

                            let response = ui.add(name_field);

                            if state.create_group_popup.group_name.trim().is_empty() && response.changed() {
                                ui.painter().rect_stroke(
                                    response.rect,
                                    2.0,
                                    Stroke::new(1.5, ui.visuals().error_fg_color)
                                );
                            }
                        });
                    });

                    ui.add_space(20.0);

                    // Calcola se mostrare il bottone aggiungi
                    let search_text = state.create_group_popup.search_query.trim().to_string();
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
                        && !state.create_group_popup.selected_participants.contains(&search_text);

                    // Barra di ricerca con bottone aggiungi
                    ui.horizontal(|ui| {
                        ui.add_space(8.0);
                        ui.label(RichText::new(egui_remixicon::icons::SEARCH_LINE).size(20.0));
                        ui.add_space(12.0);
                        ui.vertical(|ui| {
                            ui.label(RichText::new("Cerca o aggiungi utenti").size(13.0).weak());

                            ui.horizontal(|ui| {
                                let available_width = ui.available_width() - 50.0;
                                let search_response = ui.add(
                                    TextEdit::singleline(&mut state.create_group_popup.search_query)
                                        .hint_text("Cerca nei contatti o scrivi username...")
                                        .desired_width(available_width - 80.0)
                                );

                                // Enter per aggiungere
                                if search_response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                                    if show_add_button && !state.is_checking_user() {
                                        state.create_group_popup.pending_user_verification = Some(search_text.clone());
                                        state.request_dm_creation(search_text.clone());
                                        state.create_group_popup.search_query.clear();
                                    }
                                }

                                // Mostra spinner se stiamo verificando
                                if state.is_checking_user() && state.create_group_popup.pending_user_verification.is_some() {
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
                                        state.create_group_popup.pending_user_verification = Some(search_text.clone());
                                        state.request_dm_creation(search_text.clone());
                                        state.create_group_popup.search_query.clear();
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

                        let search_lower = state.create_group_popup.search_query.trim().to_lowercase();
                        let filtered_dms: Vec<_> = dm_conversations
                            .iter()
                            .filter(|c| {
                                if search_lower.is_empty() {
                                    true
                                } else {
                                    c.title.to_lowercase().starts_with(&search_lower)
                                }
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
                                    .id_source("create_group_popup_available_contacts_scroll")
                                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
                                    .show(ui, |ui| {
                                        ui.set_min_width(ui.available_width());
                                        ui.set_min_height(fixed_height);

                                        if filtered_dms.is_empty() {
                                            if dm_conversations.is_empty() {
                                                ui.vertical_centered(|ui| {
                                                    ui.add_space(fixed_height / 2.0 - 20.0);
                                                    ui.label(
                                                        RichText::new("Nessun contatto disponibile")
                                                            .italics()
                                                            .color(egui::Color32::GRAY)
                                                    );
                                                    ui.add_space(4.0);
                                                    ui.label(
                                                        RichText::new("💡 Usa la barra di ricerca per aggiungere utenti")
                                                            .size(10.0)
                                                            .color(egui::Color32::from_rgb(160, 100, 60))
                                                    );
                                                });
                                            } else {
                                                ui.vertical_centered(|ui| {
                                                    ui.add_space(fixed_height / 2.0 - 10.0);
                                                    ui.label(
                                                        RichText::new("Nessun contatto trovato con questo nome")
                                                            .italics()
                                                            .color(egui::Color32::GRAY)
                                                    );
                                                });
                                            }
                                        } else {
                                            for conv in filtered_dms {
                                                let username = conv.title.clone();
                                                let is_selected = state.create_group_popup.selected_participants.contains(&username);

                                                ui.horizontal(|ui| {
                                                    ui.spacing_mut().item_spacing.x = 0.0;

                                                    let mut selected = is_selected;
                                                    if ui.checkbox(&mut selected, "").changed() {
                                                        if selected {
                                                            state.create_group_popup.selected_participants.insert(username.clone());
                                                        } else {
                                                            state.create_group_popup.selected_participants.remove(&username);
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

                    // Scroll area partecipanti selezionati
                    ui.label(
                        RichText::new("Partecipanti aggiunti")
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
                            let fixed_height = 100.0;

                            egui::ScrollArea::vertical()
                                .max_height(fixed_height)
                                .min_scrolled_height(fixed_height)
                                .id_source("create_group_popup_selected_participants_scroll")
                                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
                                .show(ui, |ui| {
                                    ui.set_min_width(ui.available_width());
                                    ui.set_min_height(fixed_height);

                                    if state.create_group_popup.selected_participants.is_empty() {
                                        ui.vertical_centered(|ui| {
                                            ui.add_space(fixed_height / 2.0 - 10.0);
                                            ui.label(
                                                RichText::new("Nessun partecipante aggiunto")
                                                    .italics()
                                                    .color(egui::Color32::GRAY)
                                            );
                                        });
                                    } else {
                                        let mut selected_list: Vec<String> = state.create_group_popup.selected_participants
                                            .iter()
                                            .cloned()
                                            .collect();
                                        selected_list.sort();

                                        ui.spacing_mut().item_spacing.y = 6.0;

                                        for username in selected_list {
                                            ui.horizontal(|ui| {
                                                ui.spacing_mut().item_spacing.x = 4.0;

                                                if ui.small_button(egui_remixicon::icons::CLOSE_LINE).clicked() {
                                                    state.create_group_popup.selected_participants.remove(&username);
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
                let can_create = !state.create_group_popup.group_name.trim().is_empty();

                ui.horizontal(|ui| {
                    let cancel_button = egui::Button::new(
                        RichText::new(format!("{} Annulla", egui_remixicon::icons::CLOSE_LINE))
                            .size(14.0)
                    )
                        .fill(ui.visuals().widgets.inactive.bg_fill)
                        .min_size(egui::vec2(130.0, 40.0));

                    if ui.add(cancel_button).clicked() {
                        state.create_group_popup.reset();
                        should_close = true;
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let create_button = egui::Button::new(
                            RichText::new(format!("{} Crea Gruppo", egui_remixicon::icons::CHAT_NEW_LINE))
                                .size(14.0)
                        )
                            .fill(if can_create {
                                egui::Color32::from_rgb(200, 100, 40)
                            } else {
                                ui.visuals().widgets.inactive.bg_fill
                            })
                            .min_size(egui::vec2(180.0, 40.0));

                        if ui.add_enabled(can_create, create_button).clicked() {
                            state.create_group_with_participants();
                            should_close = true;
                        }
                    });
                });
            });

        });

    if should_close {
        open = false;
    }

    state.show_create_group_modal = open;
}

