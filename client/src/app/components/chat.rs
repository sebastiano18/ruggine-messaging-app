use crate::models::{WsStatus, MessageDto};
use crate::state::AppState;
use eframe::egui::{self, Frame, RichText, TextEdit};
use uuid::Uuid;
use tracing::info;

pub fn panel(ui: &mut egui::Ui, s: &mut AppState) {
    ui.heading("💬 Chat");
    ui.separator();

    if s.token.is_none() {
        ui.colored_label(egui::Color32::RED, "Login richiesto");
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

    ui.separator();

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

    // Separatore tra messaggi e input
    ui.add_space(6.0);
    ui.separator();
    ui.add_space(6.0);

    // 2) Input area
    show_input_area(ui, s, cid, input_h);
}

fn show_input_area(ui: &mut egui::Ui, s: &mut AppState, cid: Uuid, height: f32) {
    // Contenitore con altezza fissa per la barra dei messaggi
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), height),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            // Campo di testo
            let mut input_width = ui.available_width();
            // spazio stimato per bottoni e stato a destra
            input_width = (input_width - 110.0).max(120.0);

            let input_response = ui.add(
                TextEdit::singleline(&mut s.input)
                    .hint_text("Scrivi un messaggio...")
                    .desired_width(input_width),
            );

            // Invio con Enter
            if input_response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                send_message(s, cid);
            }

            // Pulsante invio
            if ui.button("📤").on_hover_text("Invia").clicked() {
                send_message(s, cid);
            }

            // Stato/azioni (a destra)
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if s.ws_status == WsStatus::Disconnected {
                    if ui
                        .button("🔌 Riconnetti")
                        .on_hover_text("Riconnetti WebSocket")
                        .clicked()
                    {
                        s.request_ws_reconnect = true;
                    }
                } else if s.ws_status == WsStatus::Connecting {
                    ui.label("🔌 Connessione...");
                } else {
                    ui.label("🟢");
                }
            });
        },
    );
}

fn show_conversation_header(ui: &mut egui::Ui, s: &mut AppState, cid: Uuid) {
    ui.horizontal(|ui| {
        // Titolo conversazione
        if let Some(ref conversations) = s.conversations {
            if let Some(conv) = conversations.iter().find(|c| c.id == cid) {
                let icon = match conv.kind.as_str() {
                    "group" => "👥",
                    "dm" => "💬",
                    _ => "💭",
                };
                ui.label(format!("{} {}", icon, conv.title));

                // Se è un gruppo e l'utente è owner, mostra bottone +
                if conv.kind == "group" {
                    if let Some(user_id) = s.user_id {
                        if conv.owner_id == user_id {
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui
                                    .button(RichText::new("➕").size(18.0))
                                    .on_hover_text("Aggiungi membri al gruppo")
                                    .clicked()
                                {
                                    s.show_invite_popup = true;
                                }
                                
                                if ui
                                    .button(RichText::new("ℹ️").size(18.0))
                                    .on_hover_text("Mostra membri del gruppo")
                                    .clicked()
                                {
                                    s.show_members_popup = true;
                                    s.load_conversation_members(cid, s.token.clone().unwrap_or_default());
                                }
                            });
                        }
                    }
                }
            } else if s.is_dm_stub(cid) {
                // Se è uno stub, mostra il titolo dallo stato
                ui.label(format!("💬 {}", s.conv_title));
            }
        } else if s.cid.is_some() && !s.conv_title.is_empty() {
            // Fallback per stub quando conversations non è ancora caricato
            ui.label(format!("💬 {}", s.conv_title));
        }
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
    let mut close_popup = false;

    egui::Window::new("Aggiungi membri")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ui.ctx(), |ui| {
            ui.set_width(300.0);

            ui.label("Inserisci il nome utente del membro da aggiungere:");
            ui.add_space(8.0);

            let response = ui.add(
                egui::TextEdit::singleline(&mut s.invite_username_input)
                    .hint_text("username")
                    .desired_width(280.0),
            );

            // Focus automatico sul campo di testo
            if s.show_invite_popup {
                response.request_focus();
            }

            // Invio con Enter
            if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                let username = s.invite_username_input.trim().to_string();
                if !username.is_empty() {
                    // Invia invito via WebSocket
                    s.send_invite_user(cid, username);
                    s.invite_username_input.clear();
                    // Non chiudere il popup, permetti di aggiungere più membri
                }
            }

            ui.add_space(12.0);

            ui.horizontal(|ui| {
                if ui.button("✖ Chiudi").clicked() {
                    close_popup = true;
                }

                ui.add_space(8.0);

                if ui.button("➕ Aggiungi").clicked() {
                    let username = s.invite_username_input.trim().to_string();
                    if !username.is_empty() {
                        s.send_invite_user(cid, username);
                        s.invite_username_input.clear();
                    }
                }
            });
        });

    if close_popup {
        s.show_invite_popup = false;
        s.invite_username_input.clear();
    }
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
        ui.label("⚠ Nessuna conversazione selezionata");
        ui.add_space(10.0);
        ui.label("Vai alla scheda 'Conversazioni' per creare o selezionare una chat");
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