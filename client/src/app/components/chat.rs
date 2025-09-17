use crate::models::{UiEvent, WsStatus, ConversationDto, MessageDto};
use crate::state::AppState;
use crate::api;
use eframe::egui::{self, Frame, RichText, Stroke, TextEdit};
use uuid::Uuid;
use tracing::info;

pub fn panel(ui: &mut egui::Ui, s: &mut AppState) {
    ui.heading("💬 Chat");
    ui.separator();

    if s.token.is_none() {
        ui.colored_label(egui::Color32::RED, "Login richiesto");
        return;
    }
    let token = s.token.clone().unwrap();

    if let Some(cid) = s.cid {
        show_chat_interface(ui, s, cid, &token);
    } else {
        show_empty_state(ui);
    }
}

fn show_chat_interface(ui: &mut egui::Ui, s: &mut AppState, cid: Uuid, token: &str) {
    // Header conversazione
    show_conversation_header(ui, s);

    // Se gruppo, mostra opzioni invito
    if let Some(ref conversations) = s.conversations {
        if let Some(conv) = conversations.iter().find(|c| c.id == cid) {
            if conv.kind == "group" {
                show_invite_options(ui, s, cid, token);
            }
        }
    }

    ui.separator();

    // === Split manuale: messaggi (in alto, cresce) + input (in basso, fisso) ===
    let total_h = ui.available_height();
    let total_w = ui.available_width();
    let input_h: f32 = 60.0;

    // "cromatura" fra messaggi e input: spazio + separatore + spazio
    let chrome_h: f32 = 6.0 + 1.0 + 6.0;

    // L'area messaggi prende tutto lo spazio restante
    let messages_h = (total_h - input_h - chrome_h).max(120.0);

    // 1) Messaggi: occupano lo spazio superiore
    ui.allocate_ui_with_layout(
        egui::vec2(total_w, messages_h),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            // Frame opzionale per garantire minimo e isolare lo scroll
            Frame::none()
                .fill(ui.visuals().panel_fill) // stesso colore del pannello
                .show(ui, |ui| {
                    ui.set_min_height(messages_h);
                    show_messages_area(ui, s);
                });
        },
    );

    // Separatore "cromatura"
    ui.add_space(6.0);
    ui.separator();
    ui.add_space(6.0);

    // 2) Input: altezza fissa in basso
    show_input_area(ui, s, cid, token, input_h);
}

fn show_messages_area(ui: &mut egui::Ui, s: &AppState) {
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .stick_to_bottom(true)
        .show(ui, |ui| {
            for message in &s.messages {
                let row_width = ui.available_width();
                ui.allocate_ui_with_layout(
                    egui::vec2(row_width, 0.0),
                    egui::Layout::top_down(egui::Align::Min),
                    |row| {
                        show_message(row, s, message);
                    },
                );
                ui.add_space(6.0);
            }
        });
}

fn show_input_area(ui: &mut egui::Ui, s: &mut AppState, cid: Uuid, token: &str, height: f32) {
    // Contenitore con altezza fissa per la barra dei messaggi
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), height),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            // Campo di testo
            // Calcolo una larghezza "prudente" per lasciare spazio ai bottoni a destra
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
                send_message(s, cid, token);
            }

            // Pulsante invio
            if ui.button("📤").on_hover_text("Invia").clicked() {
                send_message(s, cid, token);
            }

            // Stato/azioni (a destra)
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if s.ws_status == WsStatus::Disconnected {
                    if ui
                        .button("🔌 Riconnetti")
                        .on_hover_text("Riprova a collegare il WebSocket")
                        .clicked()
                    {
                        s.request_ws_reconnect = true;
                    }
                }

                if s.is_loading && !s.is_initial_load_complete {
                    ui.add_space(8.0);
                    ui.spinner();
                    ui.label("Caricamento...");
                }
            });
        },
    );
}

fn show_conversation_header(ui: &mut egui::Ui, s: &AppState) {
    if let Some(ref conversations) = s.conversations {
        if let Some(cid) = s.cid {
            if let Some(conv) = conversations.iter().find(|c| c.id == cid) {
                let icon = match conv.kind.as_str() {
                    "group" => "👥",
                    "dm" => "💬",
                    _ => "👭",
                };
                ui.label(format!("{} {}", icon, conv.title));
            } else if s.is_dm_stub(cid) {
                // Se è uno stub, mostra il titolo dallo stato
                ui.label(format!("💬 {}", s.conv_title));
            }
        }
    } else if s.cid.is_some() && !s.conv_title.is_empty() {
        // Fallback per stub quando conversations non è ancora caricato
        ui.label(format!("💬 {}", s.conv_title));
    }
}

fn show_invite_options(ui: &mut egui::Ui, s: &mut AppState, cid: Uuid, token: &str) {
    Frame::group(ui.style())
        .fill(egui::Color32::from_rgb(255, 140, 60).linear_multiply(0.1))
        .stroke(Stroke::new(
            1.0,
            egui::Color32::from_rgb(255, 140, 60).linear_multiply(0.3),
        ))
        .inner_margin(egui::Margin::symmetric(12.0, 8.0))
        .rounding(egui::Rounding::same(6.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("🎫").size(16.0));
                ui.add_space(4.0);
                ui.label(
                    RichText::new("Invita membri")
                        .strong()
                        .color(egui::Color32::from_rgb(255, 140, 60)),
                );

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // Pulsante per generare token di invito
                    if ui
                        .button(RichText::new("🔮 Genera Token").color(egui::Color32::WHITE))
                        .on_hover_text("Crea un token di invito per questo gruppo")
                        .clicked()
                    {
                        let base = s.base.clone();
                        let token2 = token.to_string();
                        let tx = s.ui_tx.clone();

                        s.rt.spawn(async move {
                            match api::conversation::create_invite(&base, &token2, cid).await {
                                Ok(invite_token) => {
                                    let _ = tx.send(UiEvent::InviteCreated(invite_token));
                                }
                                Err(e) => {
                                    let _ = tx.send(UiEvent::Error(format!(
                                        "Creazione invito fallita: {}",
                                        e
                                    )));
                                }
                            }
                        });
                    }

                    ui.add_space(8.0);

                    // Mostra il token generato se disponibile
                    if let Some(ref invite_token) = s.last_created_invite {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new("Token:")
                                    .small()
                                    .color(egui::Color32::from_rgb(200, 120, 80)),
                            );
                            let mut token_text = invite_token.clone();
                            ui.add(
                                TextEdit::singleline(&mut token_text)
                                    .desired_width(120.0)
                                    .font(egui::TextStyle::Monospace),
                            );
                            if ui.small_button("📋").on_hover_text("Copia token").clicked() {
                                ui.output_mut(|o| o.copied_text = invite_token.clone());
                                let _ = s.ui_tx.send(UiEvent::Info("Token copiato!".into()));
                            }
                        });
                    }
                });
            });
        });

    ui.add_space(4.0);
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
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
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
                        ui.colored_label(egui::Color32::from_rgb(255, 220, 180), "✔✔");
                        ui.add_space(4.0);
                        let time = format_time(message.created_at);
                        ui.colored_label(egui::Color32::from_rgb(255, 200, 150), time);
                    });
                });
            });
    });
}

fn show_other_message(ui: &mut egui::Ui, message: &MessageDto) {
    ui.with_layout(egui::Layout::left_to_right(egui::Align::Min), |ui| {
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
    });
}

fn show_empty_state(ui: &mut egui::Ui) {
    ui.vertical_centered(|ui| {
        ui.add_space(100.0);
        ui.label("⚠ Nessuna conversazione selezionata");
        ui.add_space(10.0);
        ui.label("Vai alla scheda 'Conversazioni' per creare o selezionare una chat");
    });
}

fn send_message(s: &mut AppState, cid: Uuid, token: &str) {
    let content = s.input.trim().to_string();
    if content.is_empty() {
        return;
    }
    s.input.clear();

    // Messaggio ottimistico (appare subito)
    if let Some(user_id) = s.user_id {
        let optimistic_msg = MessageDto {
            id: Uuid::new_v4(),
            author_id: user_id,
            conversation_id: cid,
            author_username: s.username.clone(),
            content: content.clone(),
            created_at: chrono::Utc::now().timestamp(),
        };

        s.messages.push(optimistic_msg.clone());

        if let Some(msgs) = s.conversation_messages.get_mut(&cid) {
            msgs.push(optimistic_msg);
        }
    }

    // Usa WebSocket - PRIMA di rimuovere lo stub!
    // send_chat_message_ws ha bisogno dello stub per includere target_username
    s.send_chat_message_ws(content);

    // DOPO l'invio, se era uno stub DM, convertilo in conversazione reale
    // TODO: In futuro sostituire con sistema UUID temporaneo che riceve UUID reale dal server
    if s.dm_stubs.contains_key(&cid) {
        let target_username = s.dm_stubs.remove(&cid).unwrap();

        // Crea la conversazione reale e aggiungila alla lista
        let real_conversation = ConversationDto {
            id: cid,
            kind: "dm".to_string(),
            title: target_username.clone(),
            owner_id: s.user_id.unwrap_or(Uuid::nil()),
            created_at: chrono::Utc::now().timestamp(),
        };

        if let Some(ref mut conversations) = s.conversations {
            // Verifica che non esista già (safety check)
            if !conversations.iter().any(|c| c.id == cid) {
                conversations.insert(0, real_conversation);
            }
        } else {
            s.conversations = Some(vec![real_conversation]);
        }

        info!("DM stub converted to real conversation on message send");
    }
}

fn format_time(timestamp: i64) -> String {
    use chrono::{DateTime, Utc};
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