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



/// Mostra il popup per invitare utenti a un gruppo
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