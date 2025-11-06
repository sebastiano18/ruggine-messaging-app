use crate::models::{WsStatus, MessageDto};
use crate::state::AppState;
use eframe::egui::{self, Frame, RichText, TextEdit};
use uuid::Uuid;
use tracing::info;

pub fn panel(ui: &mut egui::Ui, s: &mut AppState) {
    if s.token.is_none() {
        ui.vertical_centered(|ui| {
            ui.add_space(100.0);
            ui.label(
                RichText::new(egui_remixicon::icons::LOCK_LINE)
                    .size(48.0)
                    .color(egui::Color32::from_rgb(200, 100, 40))
            );
            ui.add_space(10.0);
            ui.label(
                RichText::new("Login richiesto")
                    .size(16.0)
                    .color(egui::Color32::GRAY)
            );
        });
        return;
    }

    if let Some(cid) = s.cid {
        show_chat_interface(ui, s, cid);
    } else {
        show_empty_state(ui);
    }
}

fn show_chat_interface(ui: &mut egui::Ui, s: &mut AppState, cid: Uuid) {
    // Header conversazione con bottone invita
    show_conversation_header(ui, s, cid);

    ui.add_space(2.0);

    // === Split manuale: messaggi (in alto, cresce) + input (in basso, fisso) ===
    let total_h = ui.available_height();
    let input_h: f32 = 60.0;
    let separator_space: f32 = 13.0;
    let messages_h = (total_h - input_h - separator_space).max(120.0);

    // Stati persistenti
    let anchor_state_id = ui.id().with("anchor_state").with(cid);
    let last_offset_id = ui.id().with("last_offset").with(cid);
    let messages_count_id = ui.id().with("msg_count").with(cid);

    // Recupera l'ancora salvata
    let anchor_message_id: Option<Uuid> = ui.data_mut(|d|
        d.get_temp(anchor_state_id).unwrap_or(None)
    );

    // Recupera il numero di messaggi dall'ultimo frame
    let last_message_count: usize = ui.data_mut(|d|
        d.get_temp(messages_count_id).unwrap_or(0)
    );

    // 1) Area messaggi con ScrollArea
    let scroll = egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .stick_to_bottom(true)
        .max_height(messages_h)
        .id_source("chat_messages_scroll");

    let output = scroll.show(ui, |ui| {
        // Se abbiamo solo 0-1 messaggi (vuoto o solo anteprima)
        if s.messages.len() <= 1 && *s.has_more_messages.get(&cid).unwrap_or(&true) {
            if !s.is_loading_more {
                s.load_older_messages();
            }

            ui.vertical_centered(|ui| {
                ui.add_space(messages_h / 2.0 - 20.0);
                ui.spinner();
                ui.label("Caricamento chat...");
            });
            return;
        }

        // Se non ci sono messaggi, chat vuota
        if s.messages.is_empty() {
            ui.vertical_centered(|ui| {
                ui.add_space(messages_h / 2.0 - 40.0);
                ui.label("— Chat vuota —");
                ui.add_space(10.0);
                ui.label("Invia il primo messaggio per iniziare!");
            });
            return;
        }

        // === Mostra i messaggi ===

        // Indicatore inizio conversazione
        if !*s.has_more_messages.get(&cid).unwrap_or(&true) {
            ui.vertical_centered(|ui| {
                ui.label("— Inizio conversazione —");
            });
            ui.add_space(10.0);
        }

        // Spinner se sta caricando
        if s.is_loading_more {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Caricamento messaggi precedenti...");
            });
            ui.separator();
        }

        // Renderizza tutti i messaggi, cercando l'ancora
        for (i, message) in s.messages.iter().enumerate() {
            // Se questo è il messaggio ancora e abbiamo appena caricato nuovi messaggi
            if s.messages.len() > last_message_count && Some(message.id) == anchor_message_id {
                // Scrolla a questo messaggio
                ui.scroll_to_cursor(Some(egui::Align::TOP));
                // Reset ancora
                ui.data_mut(|d| d.insert_temp(anchor_state_id, None::<Uuid>));
            }

            show_message(ui, s, message);

            if i < s.messages.len() - 1 {
                ui.add_space(6.0);
            }
        }
    });

    // Salva il numero di messaggi corrente
    ui.data_mut(|d| d.insert_temp(messages_count_id, s.messages.len()));

    // === Gestione scroll per trigger fetch ===

    // Recupera l'ultimo offset
    let last_offset: f32 = ui.data_mut(|d|
        d.get_temp(last_offset_id).unwrap_or(f32::MAX)
    );

    // Detecta quando sei nei 3/4 superiori
    let trigger_threshold = messages_h * 0.75;
    let near_top = output.state.offset.y <= trigger_threshold;

    // Controlla TUTTI i modi di scrollare
    let scroll_delta = ui.input(|i| i.smooth_scroll_delta.y);
    let scrolling_up_with_wheel = scroll_delta > 0.0;

    // Controlla se l'offset è diminuito (scroll verso l'alto in qualsiasi modo)
    let scrolled_up = output.state.offset.y < last_offset - 5.0; // 5px di tolleranza

    // Salva l'offset corrente
    ui.data_mut(|d| d.insert_temp(last_offset_id, output.state.offset.y));

    // Triggera se: sei nei 3/4 superiori E hai scrollato verso l'alto
    if near_top && (scrolling_up_with_wheel || scrolled_up) && !s.is_loading_more && !s.messages.is_empty() {
        if *s.has_more_messages.get(&cid).unwrap_or(&true) {
            // Trova quale messaggio salvare come ancora
            // Prendiamo il terzo messaggio visibile per sicurezza
            if s.messages.len() > 3 {
                if let Some(anchor_msg) = s.messages.get(3) {
                    ui.data_mut(|d| d.insert_temp(anchor_state_id, Some(anchor_msg.id)));
                }
            } else if let Some(first_msg) = s.messages.first() {
                ui.data_mut(|d| d.insert_temp(anchor_state_id, Some(first_msg.id)));
            }

            s.load_older_messages();
        }
    }

    // Spazio tra messaggi e input
    ui.add_space(8.0);

    // 2) Input area
    show_input_area(ui, s, cid, input_h);
}

fn show_input_area(ui: &mut egui::Ui, s: &mut AppState, cid: Uuid, height: f32) {
    // Frame moderno per l'input
    egui::Frame::none()
        .fill(egui::Color32::from_rgb(40, 40, 42))
        .inner_margin(egui::Margin::symmetric(16.0, 10.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                // Campo di testo con stile moderno
                let input_width = ui.available_width() - 90.0;

                let input_response = ui.add(
                    TextEdit::singleline(&mut s.input)
                        .hint_text("Scrivi un messaggio...")
                        .desired_width(input_width)
                        .frame(true)
                );

                // Invio con Enter
                if input_response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    send_message(s, cid);
                }

                ui.add_space(8.0);

                // Pulsante invio moderno
                let send_btn = egui::Button::new(
                    RichText::new(egui_remixicon::icons::SEND_PLANE_FILL)
                        .size(20.0)
                        .color(egui::Color32::WHITE)
                )
                    .fill(egui::Color32::from_rgb(200, 100, 40))
                    .min_size(egui::vec2(42.0, 36.0))
                    .rounding(8.0);

                if ui.add(send_btn)
                    .on_hover_text("Invia messaggio")
                    .clicked()
                {
                    send_message(s, cid);
                }

                ui.add_space(8.0);

                // Stato WebSocket
                match s.ws_status {
                    WsStatus::Disconnected => {
                        let reconnect_btn = egui::Button::new(
                            RichText::new(egui_remixicon::icons::REFRESH_LINE)
                                .size(18.0)
                        )
                            .fill(egui::Color32::from_rgb(220, 60, 60))
                            .min_size(egui::vec2(36.0, 36.0))
                            .rounding(8.0);

                        if ui.add(reconnect_btn)
                            .on_hover_text("Riconnetti WebSocket")
                            .clicked()
                        {
                            s.request_ws_reconnect = true;
                        }
                    }
                    WsStatus::Connecting => {
                        ui.add(egui::Spinner::new().size(16.0));
                    }
                    WsStatus::Connected => {
                        ui.label(
                            RichText::new(egui_remixicon::icons::CHECKBOX_CIRCLE_FILL)
                                .size(18.0)
                                .color(egui::Color32::from_rgb(100, 200, 100))
                        );
                    }
                }
            });
        });
}

fn show_conversation_header(ui: &mut egui::Ui, s: &mut AppState, cid: Uuid) {
    // Frame moderno per l'header
    egui::Frame::none()
        .fill(egui::Color32::from_rgb(40, 40, 42))
        .inner_margin(egui::Margin::symmetric(16.0, 12.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                // Titolo conversazione con icone remix
                if let Some(ref conversations) = s.conversations {
                    if let Some(conv) = conversations.iter().find(|c| c.id == cid) {
                        let icon = match conv.kind.as_str() {
                            "group" => egui_remixicon::icons::TEAM_FILL,
                            "dm" => egui_remixicon::icons::MESSAGE_3_FILL,
                            _ => egui_remixicon::icons::CHAT_3_FILL,
                        };

                        ui.label(
                            RichText::new(icon)
                                .size(20.0)
                                .color(egui::Color32::from_rgb(200, 100, 40))
                        );

                        ui.add_space(8.0);

                        ui.label(
                            RichText::new(&conv.title)
                                .size(16.0)
                                .strong()
                                .color(egui::Color32::WHITE)
                        );

                        // Se è un gruppo e l'utente è owner, mostra bottoni
                        if conv.kind == "group" {
                            if let Some(user_id) = s.user_id {
                                if conv.owner_id == user_id {
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        // Bottone info membri
                                        let info_btn = egui::Button::new(
                                            RichText::new(egui_remixicon::icons::USER_LINE)
                                                .size(18.0)
                                        )
                                            .fill(egui::Color32::TRANSPARENT)
                                            .stroke(egui::Stroke::NONE);

                                        if ui.add(info_btn)
                                            .on_hover_text("Mostra membri del gruppo")
                                            .clicked()
                                        {
                                            s.show_members_popup = true;
                                            s.load_conversation_members(cid, s.token.clone().unwrap_or_default());
                                        }

                                        ui.add_space(4.0);

                                        // Bottone aggiungi
                                        let add_btn = egui::Button::new(
                                            RichText::new(egui_remixicon::icons::USER_ADD_LINE)
                                                .size(18.0)
                                        )
                                            .fill(egui::Color32::TRANSPARENT)
                                            .stroke(egui::Stroke::NONE);

                                        if ui.add(add_btn)
                                            .on_hover_text("Aggiungi membri al gruppo")
                                            .clicked()
                                        {
                                            s.show_invite_popup = true;
                                        }
                                    });
                                }
                            }
                        }
                    } else if s.is_dm_stub(cid) {
                        ui.label(
                            RichText::new(egui_remixicon::icons::MESSAGE_3_FILL)
                                .size(20.0)
                                .color(egui::Color32::from_rgb(200, 100, 40))
                        );
                        ui.add_space(8.0);
                        ui.label(
                            RichText::new(&s.conv_title)
                                .size(16.0)
                                .strong()
                                .color(egui::Color32::WHITE)
                        );
                    }
                } else if s.cid.is_some() && !s.conv_title.is_empty() {
                    ui.label(
                        RichText::new(egui_remixicon::icons::MESSAGE_3_FILL)
                            .size(20.0)
                            .color(egui::Color32::from_rgb(200, 100, 40))
                    );
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new(&s.conv_title)
                            .size(16.0)
                            .strong()
                            .color(egui::Color32::WHITE)
                    );
                }
            });
        });

    // Popup per invitare membri (solo se attivo)
    if s.show_invite_popup {
        show_invite_popup(ui, s, cid);
    }

    // Popup per visualizzare membri
    if s.show_members_popup {
        show_members_popup(ui, s, cid);
    }
}


fn show_invite_popup(ui: &mut egui::Ui, s: &mut AppState, cid: Uuid) {
    let mut open = s.show_invite_popup;
    let mut should_close = false;

    egui::Window::new("")
        .collapsible(false)
        .resizable(false)
        .title_bar(false)
        .fixed_size([500.0, 600.0])
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .open(&mut open)
        .show(ui.ctx(), |ui| {
            // Titolo centrato
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new(format!("{} Aggiungi Membri al Gruppo", egui_remixicon::icons::USER_ADD_FILL))
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

                    // Calcola se mostrare il bottone aggiungi
                    let search_text = s.invite_popup.search_query.trim().to_string();
                    let dm_conversations: Vec<_> = s.conversations
                        .as_ref()
                        .map(|convs| convs.iter().filter(|c| c.kind == "dm").collect())
                        .unwrap_or_default();

                    let search_lower = search_text.to_lowercase();
                    let found_in_contacts = dm_conversations
                        .iter()
                        .any(|c| c.title.to_lowercase().starts_with(&search_lower));

                    let show_add_button = !search_text.is_empty()
                        && !found_in_contacts
                        && !s.invite_popup.selected_users.contains(&search_text);

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
                                    TextEdit::singleline(&mut s.invite_popup.search_query)
                                        .hint_text("Cerca nei contatti o scrivi username...")
                                        .desired_width(available_width - 80.0)
                                );

                                // Enter per aggiungere
                                if search_response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                                    if show_add_button {
                                        s.invite_popup.selected_users.insert(search_text.clone());
                                        s.invite_popup.search_query.clear();
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
                                    s.invite_popup.selected_users.insert(search_text.clone());
                                    s.invite_popup.search_query.clear();
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
                    if let Some(ref conversations) = s.conversations {
                        let dm_conversations: Vec<_> = conversations
                            .iter()
                            .filter(|c| c.kind == "dm")
                            .collect();

                        let search_lower = s.invite_popup.search_query.trim().to_lowercase();
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
                            .stroke(egui::Stroke::new(1.0, egui::Color32::from_gray(60)))
                            .inner_margin(10.0)
                            .rounding(6.0)
                            .show(ui, |ui| {
                                let fixed_height = 140.0;

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
                                                let is_selected = s.invite_popup.selected_users.contains(&username);

                                                ui.horizontal(|ui| {
                                                    ui.spacing_mut().item_spacing.x = 0.0;

                                                    let mut selected = is_selected;
                                                    if ui.checkbox(&mut selected, "").changed() {
                                                        if selected {
                                                            s.invite_popup.selected_users.insert(username.clone());
                                                        } else {
                                                            s.invite_popup.selected_users.remove(&username);
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
                        .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(200, 100, 40)))
                        .inner_margin(10.0)
                        .rounding(6.0)
                        .show(ui, |ui| {
                            let fixed_height = 120.0;

                            egui::ScrollArea::vertical()
                                .max_height(fixed_height)
                                .min_scrolled_height(fixed_height)
                                .id_source("selected_users_scroll")
                                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
                                .show(ui, |ui| {
                                    ui.set_min_width(ui.available_width());
                                    ui.set_min_height(fixed_height);

                                    if s.invite_popup.selected_users.is_empty() {
                                        ui.vertical_centered(|ui| {
                                            ui.add_space(fixed_height / 2.0 - 10.0);
                                            ui.label(
                                                RichText::new("Nessun utente selezionato")
                                                    .italics()
                                                    .color(egui::Color32::GRAY)
                                            );
                                        });
                                    } else {
                                        let mut selected_list: Vec<String> = s.invite_popup.selected_users
                                            .iter()
                                            .cloned()
                                            .collect();
                                        selected_list.sort();

                                        ui.spacing_mut().item_spacing.y = 6.0;

                                        for username in selected_list {
                                            ui.horizontal(|ui| {
                                                ui.spacing_mut().item_spacing.x = 4.0;

                                                if ui.small_button(egui_remixicon::icons::CLOSE_LINE).clicked() {
                                                    s.invite_popup.selected_users.remove(&username);
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
                let can_invite = !s.invite_popup.selected_users.is_empty();

                ui.horizontal(|ui| {
                    let cancel_button = egui::Button::new(
                        RichText::new(format!("{} Annulla", egui_remixicon::icons::CLOSE_LINE))
                            .size(14.0)
                    )
                        .fill(ui.visuals().widgets.inactive.bg_fill)
                        .min_size(egui::vec2(130.0, 40.0));

                    if ui.add(cancel_button).clicked() {
                        s.invite_popup.reset();
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
                            let users: Vec<String> = s.invite_popup.selected_users.iter().cloned().collect();
                            s.send_invite_users(cid, users);
                            s.invite_popup.reset();
                            should_close = true;
                        }
                    });
                });
            });
        });

    if should_close {
        open = false;
    }

    s.show_invite_popup = open;
}

fn show_message(ui: &mut egui::Ui, s: &AppState, message: &MessageDto) {
    if message.author_username == "system" {
        ui.horizontal(|ui| {
            ui.add_space(ui.available_width() * 0.3);
            ui.colored_label(egui::Color32::GRAY, &message.content);
        });
    } else {
        let is_my_message = s.user_id.map_or(false, |uid| uid == message.author_id);
        if is_my_message {
            show_my_message(ui, message);
        } else {
            show_other_message(ui, message);
        }
    }
}

fn show_my_message(ui: &mut egui::Ui, message: &MessageDto) {
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), ui.spacing().interact_size.y),
        egui::Layout::right_to_left(egui::Align::TOP),
        |ui| {
            ui.add_space(8.0);

            let row_w = ui.available_size_before_wrap().x;
            let hard_cap = (row_w * 0.60).clamp(220.0, 420.0);
            let inner_pad_x = 20.0;
            let bw = bubble_width(ui, &message.content, hard_cap - inner_pad_x, inner_pad_x);

            Frame::none()
                .fill(egui::Color32::from_rgb(200, 100, 40))
                .rounding(egui::Rounding::same(12.0))
                .inner_margin(egui::Margin::symmetric(10.0, 6.0))
                .show(ui, |ui| {
                    ui.set_width(bw);

                    ui.vertical(|ui| {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(&message.content).color(egui::Color32::WHITE),
                            )
                                .wrap(true),
                        );
                        ui.add_space(3.0);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            // Stato conferma con gestione fallimento
                            let (status_icon, status_color) = match message.is_confirmed {
                                Some(true) => {
                                    // Confermato - doppia spunta arancione
                                    ("✔✔", egui::Color32::from_rgb(255, 220, 180))
                                }
                                Some(false) => {
                                    // Fallito - X rossa
                                    ("❌", egui::Color32::from_rgb(255, 80, 80))
                                }
                                None => {
                                    // Appena inviato, in attesa - spunta singola grigia
                                    ("✔", egui::Color32::from_rgb(200, 200, 200))
                                }
                            };

                            ui.colored_label(status_color, status_icon);
                            ui.add_space(4.0);
                            let time = format_time(message.created_at);
                            ui.colored_label(egui::Color32::from_rgb(255, 200, 150), time);
                        });
                    });
                });
        }
    );
}

fn show_other_message(ui: &mut egui::Ui, message: &MessageDto) {
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), ui.spacing().interact_size.y),
        egui::Layout::left_to_right(egui::Align::TOP),
        |ui| {
            ui.add_space(8.0);

            let row_w = ui.available_size_before_wrap().x;
            let hard_cap = (row_w * 0.60).clamp(220.0, 420.0);
            let inner_pad_x = 20.0;
            let bw = bubble_width(ui, &message.content, hard_cap - inner_pad_x, inner_pad_x);

            Frame::none()
                .fill(egui::Color32::from_rgb(60, 60, 60))
                .rounding(egui::Rounding::same(12.0))
                .inner_margin(egui::Margin::symmetric(10.0, 6.0))
                .show(ui, |ui| {
                    ui.set_width(bw);

                    ui.vertical(|ui| {
                        ui.colored_label(
                            egui::Color32::from_rgb(255, 180, 100),
                            &message.author_username,
                        );
                        ui.add_space(2.0);
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(&message.content).color(egui::Color32::WHITE),
                            )
                                .wrap(true),
                        );
                        ui.add_space(3.0);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let time = format_time(message.created_at);
                            ui.colored_label(egui::Color32::from_rgb(180, 180, 180), time);
                        });
                    });
                });
        }
    );
}

fn show_empty_state(ui: &mut egui::Ui) {
    ui.vertical_centered(|ui| {
        ui.add_space(100.0);

        ui.label(
            RichText::new(egui_remixicon::icons::CHAT_3_LINE)
                .size(64.0)
                .color(egui::Color32::from_rgb(100, 100, 100))
        );

        ui.add_space(20.0);

        ui.label(
            RichText::new("Nessuna conversazione selezionata")
                .size(18.0)
                .color(egui::Color32::from_rgb(180, 180, 180))
        );

        ui.add_space(10.0);

        ui.label(
            RichText::new("Seleziona una chat dalla sidebar o crea una nuova conversazione")
                .size(13.0)
                .color(egui::Color32::from_rgb(120, 120, 120))
        );
    });
}

fn send_message(s: &mut AppState, cid: Uuid) {
    let content = s.input.trim().to_string();
    if content.is_empty() {
        return;
    }
    s.input.clear();

    // Genera client_msg_id per tracking
    let client_msg_id = Uuid::new_v4().to_string();
    info!("Sending message with client_msg_id: {}", client_msg_id);

    // Messaggio ottimistico con tracking
    if let Some(user_id) = s.user_id {
        let optimistic_msg = MessageDto::optimistic_message(
            user_id,
            s.username.clone(),
            cid,
            content.clone(),
            client_msg_id.clone(),
        );

        // IMPORTANTE: Salva nei pending per tracking conferma
        s.pending_confirmations.insert(client_msg_id.clone(), optimistic_msg.clone());
        info!("Added pending confirmation for client_id: {}", client_msg_id);

        // Aggiungi alla UI
        s.messages.push(optimistic_msg.clone());

        // Aggiungi alla cache
        if let Some(msgs) = s.conversation_messages.get_mut(&cid) {
            msgs.push(optimistic_msg);
        }
    }

    // Controlla se è un DM stub PRIMA di inviare
    let is_dm_stub = s.dm_stubs.contains_key(&cid);

    // Usa WebSocket con client_msg_id
    s.send_chat_message_ws(content, Some(client_msg_id));

    // NON rimuovere lo stub qui - aspetta la conferma dal server
    if is_dm_stub {
        info!("Message sent to DM stub {}, waiting for server confirmation", cid);
        // Lo stub verrà rimosso quando riceveremo conversation_confirmation dal server
    }
}

fn format_time(timestamp: i64) -> String {
    use chrono::DateTime;
    DateTime::from_timestamp(timestamp, 0)
        .map(|dt| dt.format("%H:%M").to_string())
        .unwrap_or_else(|| "??:??".to_string())
}

fn bubble_width(ui: &egui::Ui, text: &str, max_wrap: f32, pad_x: f32) -> f32 {
    ui.fonts(|fonts| {
        let mut job = egui::text::LayoutJob::default();
        job.append(
            text,
            0.0,
            egui::TextFormat {
                font_id: egui::TextStyle::Body.resolve(ui.style()),
                color: egui::Color32::WHITE,
                ..Default::default()
            },
        );
        job.wrap.max_width = max_wrap.max(1.0);
        let galley = fonts.layout_job(job);
        (galley.rect.size().x + pad_x).clamp(96.0, max_wrap + pad_x)
    })
}

fn show_members_popup(ui: &mut egui::Ui, s: &mut AppState, cid: Uuid) {
    let mut close_popup = false;
    let mut member_to_kick: Option<Uuid> = None;

    egui::Window::new("Membri del gruppo")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ui.ctx(), |ui| {
            ui.set_width(400.0);
            ui.set_height(400.0);

            if s.is_loading_members {
                ui.centered_and_justified(|ui| {
                    ui.spinner();
                    ui.label("Caricamento membri...");
                });
            } else {
                egui::ScrollArea::vertical()
                    .max_height(320.0)
                    .show(ui, |ui| {
                        if s.members_list.is_empty() {
                            ui.label("Nessun membro trovato.");
                        } else {
                            // Trova l'owner del gruppo e l'utente corrente
                            let owner_id = s.conversations
                                .as_ref()
                                .and_then(|convs| convs.iter().find(|c| c.id == cid))
                                .map(|conv| conv.owner_id);

                            let current_user_id = s.user_id;
                            let is_owner = Some(current_user_id) == owner_id.map(Some);

                            for member in &s.members_list {
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(&member.username).size(14.0));

                                    // Se questo membro è l'owner, mostra la corona
                                    if Some(member.user_id) == owner_id {
                                        ui.label(RichText::new("👑").size(14.0));
                                    }

                                    // Mostra il ruolo
                                    ui.label(
                                        RichText::new(format!("({})", member.role))
                                            .size(12.0)
                                            .color(egui::Color32::GRAY)
                                    );

                                    // Bottone espelli: solo se l'utente corrente è owner,
                                    // il membro non è l'owner stesso
                                    if is_owner && Some(member.user_id) != owner_id {
                                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                            if ui.button(RichText::new("🗑️ Espelli").color(egui::Color32::RED))
                                                .on_hover_text("Rimuovi questo membro dal gruppo")
                                                .clicked()
                                            {
                                                member_to_kick = Some(member.user_id);
                                            }
                                        });
                                    }
                                });
                                ui.add_space(4.0);
                            }
                        }
                    });
            }

            ui.add_space(12.0);

            // Pulsante Chiudi
            if ui.button("Chiudi").clicked() {
                close_popup = true;
            }
        });

    if close_popup {
        s.show_members_popup = false;
    }

    // Se c'è un membro da espellere, chiamiamo la funzione
    if let Some(user_id) = member_to_kick {
        s.kick_member(cid, user_id);
    }
}