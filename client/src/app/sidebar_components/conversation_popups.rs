use crate::models::UiEvent;
use crate::state::AppState;
use eframe::egui;
use egui::{Align2, RichText, Stroke, TextEdit};

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
            // Titolo centrato
            ui.vertical_centered(|ui| {
                ui.add_space(20.0);
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

            // Container per i due bottoni
            egui::Frame::none()
                .inner_margin(egui::Margin::symmetric(40.0, 0.0))
                .show(ui, |ui| {
                    // Card Crea Gruppo
                    let group_frame = egui::Frame::none()
                        .fill(egui::Color32::from_rgb(255, 140, 60).linear_multiply(0.08))
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

                    // Card Messaggio Privato
                    let dm_frame = egui::Frame::none()
                        .fill(egui::Color32::from_rgb(255, 140, 60).linear_multiply(0.08))
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

            // Padding in basso
            ui.add_space(20.0);
        });

    if !open {
        *show_action_popup = false;
    }
}

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
            // Titolo centrato
            ui.vertical_centered(|ui| {
                ui.add_space(20.0);
                ui.label(
                    RichText::new(format!("{} Nuovo Messaggio Privato", egui_remixicon::icons::CHAT_1_FILL))
                        .size(24.0)
                        .strong()
                        .color(egui::Color32::from_rgb(200, 100, 40))
                );
            });

            ui.add_space(30.0);

            // Contenuto
            egui::Frame::none()
                .inner_margin(egui::Margin::symmetric(30.0, 0.0))
                .show(ui, |ui| {
                    // Campo username
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

                            let response = ui.add(username_field);

                            if dm_username.trim().is_empty() && response.changed() {
                                ui.painter().rect_stroke(
                                    response.rect,
                                    2.0,
                                    Stroke::new(1.5, ui.visuals().error_fg_color)
                                );
                            }
                        });
                    });

                    ui.add_space(30.0);

                    // Bottoni
                    ui.horizontal(|ui| {
                        ui.add_space(8.0);

                        // Bottone Annulla
                        let cancel_btn = egui::Button::new(
                            RichText::new("Annulla")
                                .size(14.0)
                        )
                            .min_size(egui::vec2(120.0, 36.0))
                            .rounding(egui::Rounding::same(8.0));

                        if ui.add(cancel_btn).clicked() {
                            dm_username.clear();
                            *show_create_dm_popup = false;
                        }

                        ui.add_space(ui.available_width() - 140.0);

                        // Bottone Crea
                        let can_create = !dm_username.trim().is_empty();
                        let create_btn = egui::Button::new(
                            RichText::new(format!("{} Invia", egui_remixicon::icons::SEND_PLANE_FILL))
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
                            // Invia l'evento per creare il DM
                            let username = dm_username.trim().to_string();

                            if let Some(token) = &state.token {
                                let base = state.base.clone();
                                let token = token.clone();
                                let tx = state.ui_tx.clone();

                                state.rt.spawn(async move {
                                    match crate::api::conversation::create_dm(&base, &token, username.clone()).await {
                                        Ok(cid) => {
                                            let _ = tx.send(UiEvent::DmStubCreated(cid, username.clone()));
                                            let _ = tx.send(UiEvent::Opened(cid));
                                        }
                                        Err(e) => {
                                            let _ = tx.send(UiEvent::Error(format!("Errore creazione DM: {}", e)));
                                        }
                                    }
                                });
                            }

                            dm_username.clear();
                            *show_create_dm_popup = false;
                        }
                    });
                });
        });

    if !open {
        dm_username.clear();
        *show_create_dm_popup = false;
    }
}

/// Mostra il popup di conferma eliminazione/uscita conversazione
pub fn show_delete_confirmation_popup(ctx: &egui::Context, state: &mut AppState) {
    let Some(pending) = state.pending_deletion.clone() else {
        return;
    };

    let conversation = pending.conversation;
    let mut open = true;
    let mut confirm = false;
    let mut cancel = false;

    let is_stub = state.is_dm_stub(conversation.id);
    let is_owner = state.user_id.map_or(false, |uid| uid == conversation.owner_id);

    let (title, main_message, detail_message, confirm_label) = if is_stub {
        (
            "Conferma eliminazione",
            format!("Sei sicuro di voler eliminare la chat privata \"{}\"?", conversation.title),
            "Si tratta di uno stub locale: verrà semplicemente rimosso dalla tua lista.",
            "Elimina"
        )
    } else if conversation.kind == "group" {
        if is_owner {
            (
                "Conferma eliminazione gruppo",
                format!("Sei sicuro di voler eliminare il gruppo \"{}\"?", conversation.title),
                "Attenzione: eliminando il gruppo, questo verrà rimosso per TUTTI i partecipanti. L'azione è irreversibile.",
                "Elimina per tutti"
            )
        } else {
            (
                "Conferma uscita dal gruppo",
                format!("Sei sicuro di voler uscire dal gruppo \"{}\"?", conversation.title),
                "Uscirai dal gruppo e non potrai più vedere i messaggi. Potrai rientrare solo se verrai invitato nuovamente.",
                "Esci dal gruppo"
            )
        }
    } else {
        (
            "Conferma eliminazione",
            format!("Sei sicuro di voler eliminare la chat privata \"{}\"?", conversation.title),
            "L'eliminazione rimuoverà definitivamente la conversazione. L'azione è irreversibile.",
            "Elimina"
        )
    };

    egui::Window::new(title)
        .id(egui::Id::new("delete_confirmation_popup"))
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
        .open(&mut open)
        .show(ctx, |ui| {
            ui.set_width(400.0);
            ui.vertical(|ui| {
                ui.add_space(10.0);
                ui.label(RichText::new(main_message).strong());
                ui.add_space(8.0);
                ui.label(
                    RichText::new(detail_message)
                        .size(12.0)
                        .color(egui::Color32::from_rgb(210, 200, 200)),
                );
                ui.add_space(20.0);

                ui.horizontal(|ui| {
                    ui.add_space(50.0);
                    if ui.button("Annulla").clicked() {
                        cancel = true;
                    }

                    let confirm_button = egui::Button::new(
                        RichText::new(confirm_label).color(egui::Color32::WHITE),
                    )
                        .fill(egui::Color32::from_rgb(200, 100, 40));

                    ui.add_space(80.0);

                    if ui.add(confirm_button).clicked() {
                        confirm = true;
                    }
                });

                ui.add_space(10.0);
            });
        });

    if confirm {
        state.execute_pending_deletion();
    } else if cancel || !open {
        state.cancel_delete_confirmation();
    }
}

/// Mostra il popup per creare un nuovo gruppo
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
        .fixed_size([500.0, 600.0])
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
                                    if show_add_button {
                                        state.create_group_popup.selected_participants.insert(search_text.clone());
                                        state.create_group_popup.search_query.clear();
                                    }
                                }

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
                                    state.create_group_popup.selected_participants.insert(search_text.clone());
                                    state.create_group_popup.search_query.clear();
                                }
                            });

                            // Info sotto la barra di ricerca
                            if show_add_button {
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
                                let fixed_height = 140.0;

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
                            let fixed_height = 140.0;

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
                            RichText::new(format!("{} Crea Gruppo", egui_remixicon::icons::CHECK_LINE))
                                .size(14.0)
                        )
                            .fill(if can_create {
                                egui::Color32::from_rgb(200, 100, 40)
                            } else {
                                ui.visuals().widgets.inactive.bg_fill
                            })
                            .min_size(egui::vec2(180.0, 40.0));

                        if ui.add_enabled(can_create, create_button).clicked() {
                            create_group_with_participants(state);
                            should_close = true;
                        }
                    });
                });
            });

            ui.add_space(20.0);
        });

    if should_close {
        open = false;
    }

    state.show_create_group_modal = open;
}

/// Helper per creare un gruppo con partecipanti
fn create_group_with_participants(state: &mut AppState) {
    use crate::models::{ConversationDto, MessageDto, Outgoing, Page};
    use uuid::Uuid;

    let group_name = state.create_group_popup.group_name.trim().to_string();
    let participants: Vec<String> = state
        .create_group_popup
        .selected_participants
        .iter()
        .cloned()
        .collect();

    tracing::info!(
        "Creating group '{}' with {} participants via WebSocket: {:?}",
        group_name,
        participants.len(),
        participants
    );

    // Crea stub per il gruppo
    let stub_id = Uuid::new_v4();

    let stub_conversation = ConversationDto {
        id: stub_id,
        kind: "group".to_string(),
        title: group_name.clone(),
        owner_id: state.user_id.unwrap_or(Uuid::nil()),
        created_at: chrono::Utc::now().timestamp(),
        last_read_sequence: 0,
        last_activity: chrono::Utc::now().timestamp(),
        last_msg_seq: 0,
    };

    // Aggiungi stub alla lista conversazioni
    if let Some(ref mut convs) = state.conversations {
        convs.insert(0, stub_conversation);
    }

    // Traccia lo stub
    state.group_stubs.insert(stub_id, group_name.clone());

    // Apri il gruppo stub
    state.cid = Some(stub_id);
    state.page = Page::Chat;
    state.conv_title = group_name.clone();

    // Messaggio di sistema nello stub
    let system_msg =
        MessageDto::system_message(format!("Creazione gruppo '{}' in corso...", group_name));
    state.conversation_messages
        .entry(stub_id)
        .or_insert_with(Vec::new)
        .push(system_msg.clone());
    state.messages = vec![system_msg];

    // Invia al server
    let outgoing = Outgoing::CreateGroupWithParticipants {
        group_name,
        participant_usernames: participants,
        client_temp_id: Some(stub_id.to_string()),
    };

    if let Err(e) = state.ui_to_net_tx.try_send(outgoing) {
        // Cleanup in caso di errore
        if let Some(ref mut convs) = state.conversations {
            convs.retain(|c| c.id != stub_id);
        }
        state.group_stubs.remove(&stub_id);
        state.conversation_messages.remove(&stub_id);
        state.messages.clear();
        state.cid = None;
        state.page = Page::Conversations;

        let _ = state
            .ui_tx
            .send(UiEvent::Error(format!("Impossibile creare gruppo: {}", e)));
        return;
    }

    tracing::info!("Created group stub {} and opened it", stub_id);
    state.create_group_popup.reset();
}