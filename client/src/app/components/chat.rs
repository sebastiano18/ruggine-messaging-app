use crate::models::MessageDto;
use crate::state::AppState;
use eframe::egui::{self, Frame, RichText, TextEdit};
use uuid::Uuid;
use tracing::info;

use chrono::{DateTime, Local, Datelike, NaiveDate};
use crate::app::sidebar_components::conversation_popups;

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
        // Wrapper con margini laterali per tutta l'area messaggi
        ui.horizontal(|ui| {
            ui.add_space(40.0); // Margine sinistro - REGOLA QUESTO VALORE

            ui.vertical(|ui| {
                ui.set_width(ui.available_width() - 40.0); // Margine destro - REGOLA ANCHE QUESTO

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

        // Invio con Enter
        if input_response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            send_message(s, cid);
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
    // Header con componenti standard che si adattano al tema
    ui.horizontal(|ui| {
        ui.add_space(8.0);

        // Titolo conversazione con icone remix
        if let Some(ref conversations) = s.conversations {
            if let Some(conv) = conversations.iter().find(|c| c.id == cid) {
                let icon = match conv.kind.as_str() {
                    "group" => egui_remixicon::icons::TEAM_FILL,
                    "dm" => egui_remixicon::icons::MESSAGE_3_FILL,
                    _ => egui_remixicon::icons::CHAT_3_FILL,
                };

                // Icona con l'arancione dell'app come accento
                ui.label(
                    RichText::new(icon)
                        .size(20.0)
                        .color(egui::Color32::from_rgb(200, 100, 40))
                );

                ui.add_space(8.0);

                // Titolo con stile standard (si adatta al tema)
                ui.heading(&conv.title);

                // Se è un gruppo, mostra il bottone info a tutti
                if conv.kind == "group" {
                    if let Some(user_id) = s.user_id {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            // Bottone info membri - visibile a tutti
                            let info_btn = egui::Button::new(
                                RichText::new(egui_remixicon::icons::USER_LINE)
                                    .size(18.0)
                            )
                                .frame(false);

                            if ui.add(info_btn)
                                .on_hover_text("Mostra membri del gruppo")
                                .clicked()
                            {
                                s.show_members_popup = true;
                                // I membri verranno caricati se necessario quando si apre la popup
                            }

                            // Bottone aggiungi - solo per l'owner
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
            } else if s.is_dm_stub(cid) {
                ui.label(
                    RichText::new(egui_remixicon::icons::MESSAGE_3_FILL)
                        .size(20.0)
                        .color(egui::Color32::from_rgb(200, 100, 40))
                );
                ui.add_space(8.0);
                ui.heading(&s.conv_title);
            }
        } else if s.cid.is_some() && !s.conv_title.is_empty() {
            ui.label(
                RichText::new(egui_remixicon::icons::MESSAGE_3_FILL)
                    .size(20.0)
                    .color(egui::Color32::from_rgb(200, 100, 40))
            );
            ui.add_space(8.0);
            ui.heading(&s.conv_title);
        }

        ui.add_space(8.0);
    });

    ui.separator();

    // Popup per invitare membri (gestito centralmente)
    conversation_popups::show_invite_popup(ui.ctx(), s, cid);

    // Popup per visualizzare membri
    if s.show_members_popup {
        show_members_popup(ui, s, cid);
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

            // 2. Calcoliamo la larghezza della bolla in base al contenuto.
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
                                Some(true) => ("✔✔", egui::Color32::from_rgb(255, 220, 180)),
                                Some(false) => ("❌", egui::Color32::from_rgb(255, 80, 80)),
                                None => ("✔", egui::Color32::from_rgb(200, 200, 200)),
                            };
                            ui.colored_label(status_color, status_icon);
                            ui.add_space(4.0);
                            let time = format_time(message.created_at);
                            ui.colored_label(egui::Color32::from_rgb(255, 200, 150), time);
                        });
                    });
                });

            // Menu contestuale al click destro sulla bolla
            bubble.response.context_menu(|ui| {
                if ui.button(
                    RichText::new(format!("{} Elimina", egui_remixicon::icons::DELETE_BIN_LINE))
                        .size(14.0)
                ).clicked() {
                    s.delete_message(message.id);
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

    // Se la lista è vuota per questa conversazione e non stiamo già caricando, richiedi i membri
    if !s.members_list.contains_key(&cid) && !s.is_loading_members {
        if let Some(token) = s.token.clone() {
            s.load_conversation_members(cid, token);
        }
    }

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
                        // Ottieni la lista dei membri per questa conversazione specifica
                        let members = s.members_list.get(&cid);

                        if members.is_none() || members.unwrap().is_empty() {
                            ui.label("Nessun membro trovato.");
                        } else {
                            // Trova l'owner del gruppo e l'utente corrente
                            let owner_id = s.conversations
                                .as_ref()
                                .and_then(|convs| convs.iter().find(|c| c.id == cid))
                                .map(|conv| conv.owner_id);

                            let current_user_id = s.user_id;
                            let is_owner = Some(current_user_id) == owner_id.map(Some);

                            for member in members.unwrap() {
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