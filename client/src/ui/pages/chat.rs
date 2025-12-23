use crate::models::MessageDto;
use crate::state::AppState;
use eframe::egui::{self, Frame, RichText, TextEdit};
use uuid::Uuid;
use tracing::{debug, info};

use chrono::{DateTime, Local, NaiveDate};
use crate::app::events::utils::move_conversation_to_top;
use crate::ui::modals::conversation_popups;

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

    // Mostra il popup di conferma eliminazione messaggio se necessario
    if s.pending_message_deletion.is_some() {
        show_delete_message_confirmation(ui, s);
    }

    // Mostra il popup di conferma espulsione membro se necessario
    if s.pending_member_kick.is_some() {
        show_kick_member_confirmation(ui, s);
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
        // Wrapper con margini laterali per tutta l'area messaggi
        ui.horizontal(|ui| {
            ui.add_space(40.0); // Margine sinistro - REGOLA QUESTO VALORE

            ui.vertical(|ui| {
                ui.set_width(ui.available_width() - 40.0); // Margine destro - REGOLA ANCHE QUESTO

                // Se abbiamo solo 0-1 messaggi (vuoto o solo anteprima)
                if s.messages.is_empty() && s.is_loading_more {
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

                // Clona i messaggi per evitare problemi di borrow
                let messages = s.messages.clone();
                let message_count = messages.len();

                // Variabile per tenere traccia dell'ultima data visualizzata
                let mut last_date: Option<NaiveDate> = None;

                // Renderizza tutti i messaggi, cercando l'ancora
                for i in 0..message_count {
                    let message = &messages[i];

                    // Controlla se dobbiamo mostrare un separatore di data
                    if let Some(current_date) = get_date_from_timestamp(message.created_at) {
                        let should_show_separator = match last_date {
                            None => true,
                            Some(prev_date) => prev_date != current_date,
                        };

                        if should_show_separator {
                            show_date_separator(ui, message.created_at);
                            ui.add_space(8.0);
                        }

                        last_date = Some(current_date);
                    }

                    // Se questo è il messaggio ancora e abbiamo appena caricato nuovi messaggi
                    if message_count > last_message_count && Some(message.id) == anchor_message_id {
                        // Scrolla a questo messaggio
                        ui.scroll_to_cursor(Some(egui::Align::TOP));
                        // Reset ancora
                        ui.data_mut(|d| d.insert_temp(anchor_state_id, None::<Uuid>));
                    }

                    show_message(ui, s, message);

                    if i < message_count - 1 {
                        ui.add_space(6.0);
                    }
                }
            }); // fine vertical
        }); // fine horizontal (margini)
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

fn show_input_area(ui: &mut egui::Ui, s: &mut AppState, cid: Uuid, _height: f32) {
    ui.separator();
    ui.add_space(4.0);

    // Area input con componenti standard
    ui.horizontal(|ui| {
        ui.add_space(40.0); // Margine sinistro - UGUALE a quello dei messaggi

        // Campo di testo standard
        let input_width = ui.available_width() - 90.0;

        let input_response = ui.add(
            TextEdit::singleline(&mut s.input)
                .hint_text("Scrivi un messaggio...")
                .desired_width(input_width)
        );

        // Invio con Enter e mantieni il focus
        if input_response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            send_message(s, cid);
            input_response.request_focus();
        }

        ui.add_space(8.0);

        // Pulsante invio con l'arancione dell'app
        let send_btn = egui::Button::new(
            RichText::new(egui_remixicon::icons::SEND_PLANE_FILL)
                .size(18.0)
                .color(egui::Color32::WHITE)
        )
            .fill(egui::Color32::from_rgb(200, 100, 40))
            .min_size(egui::vec2(40.0, 32.0));

        if ui.add(send_btn)
            .on_hover_text("Invia messaggio")
            .clicked()
        {
            send_message(s, cid);
        }

        ui.add_space(40.0); // Margine destro - UGUALE a quello dei messaggi
    });

    ui.add_space(4.0);
}


fn show_conversation_header(ui: &mut egui::Ui, s: &mut AppState, cid: Uuid) {
    ui.horizontal(|ui| {
        ui.add_space(8.0);

        // Titolo conversazione con icone remix
        if let Some(ref conversations) = s.conversations {
            if let Some(conv) = conversations.iter().find(|c| c.id == cid) {
                // Conversazione esistente
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
                ui.heading(&conv.title);

                // Bottoni per gruppi
                if conv.kind == "group" {
                    if let Some(user_id) = s.user_id {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let info_btn = egui::Button::new(
                                RichText::new(egui_remixicon::icons::INFORMATION_LINE)
                                    .size(18.0)
                            )
                                .frame(false);

                            if ui.add(info_btn)
                                .on_hover_text("Informazioni Gruppo")
                                .clicked()
                            {
                                s.show_group_info_popup = true;
                            }

                            if conv.owner_id == user_id {
                                ui.add_space(4.0);

                                let add_btn = egui::Button::new(
                                    RichText::new(egui_remixicon::icons::USER_ADD_LINE)
                                        .size(18.0)
                                )
                                    .frame(false);

                                if ui.add(add_btn)
                                    .on_hover_text("Aggiungi membri al gruppo")
                                    .clicked()
                                {
                                    s.show_invite_popup = true;
                                }
                            }
                        });
                    }
                }
            } else {
                // Conversazione non trovata = DM temporanea
                // Mostra header usando conv_title
                ui.label(
                    RichText::new(egui_remixicon::icons::MESSAGE_3_FILL)
                        .size(20.0)
                        .color(egui::Color32::from_rgb(200, 100, 40))
                );
                ui.add_space(8.0);
                ui.heading(&s.conv_title);
            }
        } else {
            // Caso conversations vuoto (non dovrebbe succedere dopo login)
            if s.cid.is_some() && !s.conv_title.is_empty() {
                ui.label(
                    RichText::new(egui_remixicon::icons::MESSAGE_3_FILL)
                        .size(20.0)
                        .color(egui::Color32::from_rgb(200, 100, 40))
                );
                ui.add_space(8.0);
                ui.heading(&s.conv_title);
            }
        }

        ui.add_space(8.0);
    });

    ui.separator();

    conversation_popups::show_invite_popup(ui.ctx(), s, cid);

    if s.show_group_info_popup {
        show_group_info_popup(ui, s, cid);
    }
}




fn show_message(ui: &mut egui::Ui, s: &mut AppState, message: &MessageDto) {
    if message.is_system_message() {
        // Center system messages across the full chat width
        let full_w = ui.available_width();
        ui.allocate_ui_with_layout(
            egui::vec2(full_w, ui.spacing().interact_size.y),
            egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
            |ui| {
                ui.label(
                    RichText::new(&message.content)
                        .color(egui::Color32::GRAY)
                        .italics(),
                );
            },
        );
        ui.add_space(4.0);
    } else {
        let is_my_message = s.user_id.map_or(false, |uid| uid == message.author_id);
        if is_my_message {
            show_my_message(ui, s, message);
        } else {
            show_other_message(ui, message);
        }
    }
}

fn show_my_message(ui: &mut egui::Ui, s: &mut AppState, message: &MessageDto) {
    // Utilizziamo un layout orizzontale che occupa l'intera larghezza.
    ui.horizontal(|ui| {
        // Creiamo una sezione allineata a destra all'interno della riga orizzontale.
        ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
            // 1. Aggiungiamo il padding a destra dello schermo (10px).
            ui.add_space(10.0);

            // 2. Calcoliamo la larghezza della bolla e la disegniamo.
            // Questo calcolo ora avviene in un contesto di layout stabile.
            let row_w = ui.available_width();
            let max_bubble_width = (row_w * 0.70).clamp(220.0, 500.0);
            let inner_pad_x = 20.0;
            let optimal_width = bubble_width(ui, &message.content, max_bubble_width - inner_pad_x, inner_pad_x);

            let bubble = Frame::none()
                .fill(egui::Color32::from_rgb(200, 100, 40))
                .rounding(egui::Rounding::same(12.0))
                .inner_margin(egui::Margin::symmetric(10.0, 6.0))
                .show(ui, |ui| {
                    ui.set_width(optimal_width); // Impostiamo la larghezza calcolata
                    ui.vertical(|ui| {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(&message.content).color(egui::Color32::WHITE),
                            )
                                .wrap(true),
                        );
                        ui.add_space(3.0);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let (status_icon, status_color) = match message.is_confirmed {
                                Some(true) => (egui_remixicon::icons::CHECK_DOUBLE_LINE, egui::Color32::from_rgb(255, 220, 180)),
                                Some(false) => (egui_remixicon::icons::CLOSE_CIRCLE_LINE, egui::Color32::from_rgb(255, 80, 80)),
                                None => (egui_remixicon::icons::TIME_LINE, egui::Color32::from_rgb(200, 200, 200)),
                            };
                            ui.label(
                                RichText::new(status_icon)
                                    .size(14.0)
                                    .color(status_color)
                            );
                            ui.add_space(4.0);
                            let time = format_time(message.created_at);
                            ui.colored_label(egui::Color32::from_rgb(255, 200, 150), time);
                        });
                    });
                });

            // Menu contestuale al click destro sulla bolla
            bubble.response.context_menu(|ui| {
                // Se il messaggio ha fallito l'invio (is_confirmed == Some(false))
                let is_failed = message.is_confirmed == Some(false);

                if is_failed {
                    // Opzione "Riprova invio" per messaggi falliti
                    if ui.button(
                        RichText::new(format!("{} Riprova invio", egui_remixicon::icons::REFRESH_LINE))
                            .size(14.0)
                            .color(egui::Color32::from_rgb(255, 180, 100))
                    ).clicked() {
                        // Recupera il contenuto del messaggio e il cid
                        let content = message.content.clone();
                        let cid = message.conversation_id;

                        // Genera nuovo client_msg_id per il reinvio
                        let new_client_msg_id = Uuid::new_v4().to_string();

                        // Crea nuovo messaggio ottimistico
                        if let Some(user_id) = s.user_id {
                            let retry_msg = MessageDto::optimistic_message(
                                user_id,
                                s.username.clone(),
                                cid,
                                content.clone(),
                                new_client_msg_id.clone(),
                            );

                            // Salva nei pending per tracking conferma
                            s.pending_confirmations.insert(new_client_msg_id.clone(), retry_msg.clone());

                            // Rimuovi il messaggio fallito dalla UI
                            s.messages.retain(|m| m.id != message.id);
                            if let Some(msgs) = s.conversation_messages.get_mut(&cid) {
                                msgs.retain(|m| m.id != message.id);
                            }

                            // Aggiungi il nuovo messaggio
                            s.messages.push(retry_msg.clone());
                            if let Some(msgs) = s.conversation_messages.get_mut(&cid) {
                                msgs.push(retry_msg);
                            }

                            // Invia via WebSocket
                            s.send_chat_message_ws(content, Some(new_client_msg_id));
                        }

                        ui.close_menu();
                    }

                    ui.separator();
                }

                // Opzione "Elimina" sempre presente
                if ui.button(
                    RichText::new(format!("{} Elimina", egui_remixicon::icons::DELETE_BIN_LINE))
                        .size(14.0)
                ).clicked() {
                    s.pending_message_deletion = Some(message.id);
                    ui.close_menu();
                }
            });

            // Questo corregge un bug di layout in egui dove il layout da destra a sinistra
            // non riserva correttamente lo spazio verticale per il contenuto wrappato.
            ui.add_space(bubble.response.rect.height());
            ui.allocate_rect(bubble.response.rect, egui::Sense::hover());
        });
    });
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

    //Controlla se è un cid temporaneo (non in dm_stubs né in conversations)
    let is_temp_dm = !s.dm_stubs.contains_key(&cid)
        && s.conversations.as_ref().map(|convs| !convs.iter().any(|c| c.id == cid)).unwrap_or(true)
        && !s.conv_title.is_empty();

    // Se è temporaneo, crea lo stub ORA
    if is_temp_dm {
        let target_username = s.conv_title.clone();

        info!("Converting temporary DM {} to stub for first message to {}", cid, target_username);

        // Ora salva lo stub
        s.add_dm_stub(cid, target_username.clone());

        // Crea ConversationDto stub
        let stub_conversation = crate::models::ConversationDto {
            id: cid,
            kind: "dm".to_string(),
            title: target_username.clone(),
            owner_id: s.user_id.unwrap_or(Uuid::nil()),
            created_at: chrono::Utc::now().timestamp(),
            last_read_sequence: 0,
            last_activity: chrono::Utc::now().timestamp(),
            last_msg_seq: 0,
        };

        // Aggiungi a conversations
        if let Some(ref mut convs) = s.conversations {
            convs.insert(0, stub_conversation);
        } else {
            s.conversations = Some(vec![stub_conversation]);
        }

        s.conversation_messages.insert(cid, Vec::new());

        info!("DM stub {} created and added on first message", cid);
    }

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

        s.pending_confirmations.insert(client_msg_id.clone(), optimistic_msg.clone());
        info!("Added pending confirmation for client_id: {}", client_msg_id);

        s.messages.push(optimistic_msg.clone());

        s.conversation_messages
            .entry(cid)
            .or_insert_with(Vec::new)
            .push(optimistic_msg.clone());

        if let Some(ref mut convs) = s.conversations {
            if let Some(conv) = convs.iter_mut().find(|c| c.id == cid) {
                conv.last_activity = optimistic_msg.created_at;
                debug!("Optimistically updated conversation {} last_activity", cid);
            }
        }
    }

    s.send_chat_message_ws(content, Some(client_msg_id));

    let is_dm_stub = s.dm_stubs.contains_key(&cid);
    if is_dm_stub {
        info!("Message sent to DM stub {}, waiting for server confirmation", cid);
    }

    move_conversation_to_top(s, cid);
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

fn show_delete_message_confirmation(ui: &mut egui::Ui, s: &mut AppState) {
    let mut should_delete = false;
    let mut should_cancel = false;

    egui::Window::new("")
        .id(egui::Id::new("delete_message_confirmation"))
        .title_bar(false)
        .collapsible(false)
        .resizable(false)
        .fixed_size([400.0, 180.0])
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ui.ctx(), |ui| {
            // Titolo centrato
            ui.vertical_centered(|ui| {
                ui.add_space(20.0);
                ui.label(
                    RichText::new(format!("{} Conferma eliminazione", egui_remixicon::icons::DELETE_BIN_FILL))
                        .size(22.0)
                        .strong()
                        .color(egui::Color32::from_rgb(200, 100, 40))
                );
                ui.add_space(8.0);
                ui.label(
                    RichText::new("Vuoi confermare l'eliminazione del messaggio?")
                        .size(13.0)
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
                            should_cancel = true;
                        }

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let delete_button = egui::Button::new(
                                RichText::new(format!("{} Elimina", egui_remixicon::icons::DELETE_BIN_LINE))
                                    .size(14.0)
                                    .color(egui::Color32::WHITE)
                            )
                                .fill(egui::Color32::from_rgb(200, 100, 40))
                                .min_size(egui::vec2(140.0, 36.0));

                            if ui.add(delete_button).clicked() {
                                should_delete = true;
                            }
                        });
                    });
                });
        });

    if should_delete {
        if let Some(message_id) = s.pending_message_deletion.take() {
            s.delete_message(message_id);
        }
    } else if should_cancel {
        s.pending_message_deletion = None;
    }
}

fn show_kick_member_confirmation(ui: &mut egui::Ui, s: &mut AppState) {
    let username = s.pending_member_kick.as_ref().map(|(_, _, name)| name.clone());

    let mut action = None;  // ← Una sola variabile per catturare l'azione

    egui::Window::new("")
        .id(egui::Id::new("kick_member_confirmation"))
        .title_bar(false)
        .collapsible(false)
        .resizable(false)
        .fixed_size([400.0, 200.0])
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ui.ctx(), |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(20.0);
                ui.label(
                    RichText::new(format!("{} Conferma espulsione", egui_remixicon::icons::USER_UNFOLLOW_LINE))
                        .size(22.0)
                        .strong()
                        .color(egui::Color32::from_rgb(200, 100, 40))
                );
                ui.add_space(8.0);
                if let Some(ref name) = username {
                    ui.label(
                        RichText::new(format!("Vuoi espellere {} dal gruppo?", name))
                            .size(13.0)
                            .color(ui.visuals().weak_text_color())
                    );
                }
            });

            ui.add_space(30.0);

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
                            action = Some(false);  // ← Annulla
                        }

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let kick_button = egui::Button::new(
                                RichText::new(format!("{} Espelli", egui_remixicon::icons::USER_UNFOLLOW_LINE))
                                    .size(14.0)
                                    .color(egui::Color32::WHITE)
                            )
                                .fill(egui::Color32::from_rgb(180, 50, 50))
                                .min_size(egui::vec2(140.0, 36.0));

                            if ui.add(kick_button).clicked() {
                                action = Some(true);  // ← Espelli
                            }
                        });
                    });
                });
        });

    // Esegui l'azione DOPO il closure
    match action {
        Some(true) => {
            // Espelli
            if let Some((cid, user_id, username)) = s.pending_member_kick.take() {
                tracing::info!("🟢 Espellendo {} (cid={}, user_id={})", username, cid, user_id);

                match s.ui_to_net_tx.try_send(crate::models::Outgoing::RemoveMember { cid, user_id }) {
                    Ok(_) => tracing::info!("✅ RemoveMember inviato al canale"),
                    Err(e) => tracing::error!("❌ Errore invio RemoveMember: {}", e),
                }
            }
        }
        Some(false) => {
            // Annulla
            tracing::info!("Espulsione annullata");
            s.pending_member_kick = None;
        }
        None => {
            // Nessun bottone cliccato
        }
    }
}

fn show_group_info_popup(ui: &mut egui::Ui, s: &mut AppState, cid: Uuid) {
    let mut close_popup = false;

    // Ottieni info del gruppo
    let group_info = s.conversations
        .as_ref()
        .and_then(|convs| convs.iter().find(|c| c.id == cid))
        .map(|conv| (conv.title.clone(), conv.created_at, conv.owner_id));

    let (group_title, created_at, owner_id) = group_info
        .unwrap_or(("Gruppo".to_string(), 0, Uuid::nil()));

    egui::Window::new("")
        .id(egui::Id::new("group_info_popup"))
        .title_bar(false)
        .collapsible(false)
        .resizable(false)
        .fixed_size([450.0, 600.0])
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ui.ctx(), |ui| {
            // Bottone X in alto a destra
            ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                let close_button = egui::Button::new(
                    RichText::new(egui_remixicon::icons::CLOSE_LINE)
                        .size(18.0)
                )
                    .frame(false);

                if ui.add(close_button).on_hover_text("Chiudi").clicked() {
                    close_popup = true;
                }
            });

            // Titolo centrato
            ui.vertical_centered(|ui| {
                ui.add_space(5.0);
                ui.label(
                    RichText::new(egui_remixicon::icons::TEAM_FILL)
                        .size(40.0)
                        .color(egui::Color32::from_rgb(200, 100, 40))
                );
                ui.add_space(8.0);
                ui.label(
                    RichText::new(&group_title)
                        .size(24.0)
                        .strong()
                        .color(egui::Color32::from_rgb(200, 100, 40))
                );
            });

            ui.add_space(16.0);

            // Info gruppo (data creazione)
            egui::Frame::none()
                .inner_margin(egui::Margin::symmetric(20.0, 0.0))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(egui_remixicon::icons::CALENDAR_LINE)
                                .size(16.0)
                                .color(egui::Color32::from_rgb(200, 100, 40))
                        );
                        ui.add_space(8.0);
                        ui.label(
                            RichText::new("Creato il:")
                                .size(13.0)
                                .color(ui.visuals().weak_text_color())
                        );
                        ui.add_space(4.0);

                        // Formatta la data
                        let date_str = if created_at > 0 {
                            let datetime = chrono::DateTime::from_timestamp(created_at, 0)
                                .map(|dt| dt.format("%d/%m/%Y alle %H:%M").to_string())
                                .unwrap_or_else(|| "Data sconosciuta".to_string());
                            datetime
                        } else {
                            "Data sconosciuta".to_string()
                        };

                        ui.label(
                            RichText::new(date_str)
                                .size(13.0)
                        );
                    });
                });

            ui.add_space(16.0);
            ui.separator();
            ui.add_space(12.0);

            // Sezione membri
            ui.horizontal(|ui| {
                ui.add_space(20.0);
                ui.label(
                    RichText::new(format!("{} Membri", egui_remixicon::icons::GROUP_LINE))
                        .size(16.0)
                        .strong()
                        .color(egui::Color32::from_rgb(200, 100, 40))
                );

                // Conta membri
                if let Some(members) = s.members_list.get(&cid) {
                    ui.label(
                        RichText::new(format!("({})", members.len()))
                            .size(14.0)
                            .color(ui.visuals().weak_text_color())
                    );
                }
            });

            ui.add_space(12.0);

            if s.is_loading_members {
                ui.vertical_centered(|ui| {
                    ui.add_space(80.0);
                    ui.spinner();
                    ui.add_space(8.0);
                    ui.label("Caricamento membri...");
                });
            } else {
                let current_user_id = s.user_id;
                let is_owner = match (Some(owner_id), current_user_id) {
                    (Some(oid), Some(uid)) if oid != Uuid::nil() => oid == uid,
                    _ => false,
                };

                // Verifica se l'utente è un partecipante (non owner)
                let is_participant = if let Some(uid) = current_user_id {
                    s.members_list.get(&cid)
                        .map(|members| {
                            members.iter().any(|m| {
                                m.user_id == uid && owner_id != m.user_id
                            })
                        })
                        .unwrap_or(false)
                } else {
                    false
                };

                egui::Frame::none()
                    .inner_margin(egui::Margin::symmetric(20.0, 0.0))
                    .show(ui, |ui| {
                        egui::ScrollArea::vertical()
                            .max_height(280.0)
                            .show(ui, |ui| {
                                let members = s.members_list.get(&cid);

                                if members.is_none() || members.unwrap().is_empty() {
                                    ui.vertical_centered(|ui| {
                                        ui.add_space(100.0);
                                        ui.label(
                                            RichText::new("Nessun membro trovato")
                                                .italics()
                                                .color(ui.visuals().weak_text_color())
                                        );
                                    });
                                } else {
                                    for member in members.unwrap() {
                                        ui.horizontal(|ui| {
                                            ui.label(
                                                RichText::new(egui_remixicon::icons::USER_LINE)
                                                    .size(16.0)
                                                    .color(egui::Color32::from_rgb(200, 100, 40))
                                            );
                                            ui.add_space(8.0);
                                            ui.label(RichText::new(&member.username).size(14.0));

                                            if owner_id == member.user_id {
                                                ui.label(
                                                    RichText::new(egui_remixicon::icons::VIP_CROWN_FILL)
                                                        .size(16.0)
                                                        .color(egui::Color32::from_rgb(255, 215, 0))
                                                );
                                            }

                                            ui.label(
                                                RichText::new(format!("({})", member.role))
                                                    .size(12.0)
                                                    .color(ui.visuals().weak_text_color())
                                            );

                                            if is_owner && owner_id != member.user_id {
                                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                                    let kick_button = egui::Button::new(
                                                        RichText::new(format!("{} Espelli", egui_remixicon::icons::USER_UNFOLLOW_LINE))
                                                            .size(12.0)
                                                            .color(egui::Color32::WHITE)
                                                    )
                                                        .fill(egui::Color32::from_rgb(180, 50, 50))
                                                        .min_size(egui::vec2(80.0, 24.0));

                                                    if ui.add(kick_button)
                                                        .on_hover_text("Rimuovi questo membro dal gruppo")
                                                        .clicked()
                                                    {
                                                        s.pending_member_kick = Some((cid, member.user_id, member.username.clone()));
                                                    }
                                                });
                                            }
                                        });
                                        ui.add_space(8.0);
                                    }
                                }
                            });
                    });

                ui.add_space(20.0);

                // Bottone azione in base al ruolo - centrato
                ui.vertical_centered(|ui| {
                    if is_owner {
                        // Bottone elimina gruppo per owner
                        let delete_btn = egui::Button::new(
                            RichText::new(format!("{} Elimina Gruppo", egui_remixicon::icons::DELETE_BIN_FILL))
                                .size(13.0)
                                .color(egui::Color32::WHITE)
                        )
                            .fill(egui::Color32::from_rgb(180, 50, 50))
                            .min_size(egui::vec2(200.0, 36.0));

                        if ui.add(delete_btn)
                            .on_hover_text("Elimina definitivamente questo gruppo")
                            .clicked()
                        {
                            if let Some(conv) = s.conversations.as_ref()
                                .and_then(|convs| convs.iter().find(|c| c.id == cid))
                                .cloned()
                            {
                                s.request_delete_confirmation(&conv);
                            }
                        }
                    } else if is_participant {
                        // Bottone esci dal gruppo per partecipanti
                        let leave_btn = egui::Button::new(
                            RichText::new(format!("{} Esci dal Gruppo", egui_remixicon::icons::LOGOUT_BOX_R_LINE))
                                .size(13.0)
                                .color(egui::Color32::WHITE)
                        )
                            .fill(egui::Color32::from_rgb(200, 100, 40))
                            .min_size(egui::vec2(200.0, 36.0));

                        if ui.add(leave_btn)
                            .on_hover_text("Abbandona questo gruppo")
                            .clicked()
                        {
                            if let Some(conv) = s.conversations.as_ref()
                                .and_then(|convs| convs.iter().find(|c| c.id == cid))
                                .cloned()
                            {
                                s.request_delete_confirmation(&conv);
                            }
                        }
                    }
                });
            }

            ui.add_space(10.0);
        });

    if close_popup {
        s.show_group_info_popup = false;
    }
}
// === Funzioni per i separatori di data ===

/// Estrae la data (NaiveDate) da un timestamp
fn get_date_from_timestamp(timestamp: i64) -> Option<NaiveDate> {
    DateTime::from_timestamp(timestamp, 0).map(|dt| {
        let local_dt = dt.with_timezone(&Local);
        local_dt.date_naive()
    })
}

/// Mostra un separatore con la data formattata
fn show_date_separator(ui: &mut egui::Ui, timestamp: i64) {
    let date_text = format_date_label(timestamp);

    ui.add_space(6.0);

    // Testo centrato semplice
    ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
        ui.label(
            RichText::new(date_text)
                .size(12.0)
                .color(egui::Color32::from_rgb(140, 140, 140))
        );
    });

    ui.add_space(6.0);
}

/// Formatta la data come "Oggi", "Ieri" o "gg/mm/aa"
fn format_date_label(timestamp: i64) -> String {
    let dt = match DateTime::from_timestamp(timestamp, 0) {
        Some(dt) => dt.with_timezone(&Local),
        None => return "Data sconosciuta".to_string(),
    };

    let now = Local::now();
    let today = now.date_naive();
    let msg_date = dt.date_naive();

    let days_diff = (today - msg_date).num_days();

    match days_diff {
        0 => "Oggi".to_string(),
        1 => "Ieri".to_string(),
        _ => dt.format("%d/%m/%y").to_string(),
    }
}


pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
    panel(ui, state);
}