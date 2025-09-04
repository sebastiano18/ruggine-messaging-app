use eframe::egui::{self, TextEdit};
use uuid::Uuid;
use crate::{
    state::{AppState, UiEvent, WsStatus},
    net,
    models::MessageDto,
};

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
    show_conversation_header(ui, s);
    ui.separator();

    let total_height = ui.available_height();
    let input_height = 60.0;
    let messages_height = total_height - input_height;

    show_messages_area(ui, s, messages_height);
    show_input_area(ui, s, cid, token, input_height);
}

fn show_conversation_header(ui: &mut egui::Ui, s: &AppState) {
    if let Some(ref conversations) = s.conversations {
        if let Some(cid) = s.cid {
            if let Some(conv) = conversations.iter().find(|c| c.id == cid) {
                let icon = match conv.kind.as_str() {
                    "group" => "👥",
                    "dm" => "💬",
                    _ => "💭",
                };
                ui.label(format!("{} {}", icon, conv.title));
            }
        }
    }
}

fn show_messages_area(ui: &mut egui::Ui, s: &AppState, height: f32) {
    ui.allocate_ui_with_layout(
        [ui.available_width(), height].into(),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            egui::ScrollArea::vertical()
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    for message in &s.messages {
                        // Ogni messaggio ha una "riga" con larghezza fissata a tutta la linea
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
        },
    );
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

        // Larghezza “riga” reale e limiti bubble
        let row_w = ui.available_size_before_wrap().x;
        let hard_cap = (row_w * 0.60).clamp(220.0, 420.0); // tetto FINALE della bolla
        let inner_pad_x = 20.0; // 10 + 10 del Frame

        // Larghezza bubble = min(larghezza testo wrappato, tetto)
        let bw = bubble_width(ui, &message.content, hard_cap - inner_pad_x, inner_pad_x);

        egui::Frame::none()
            .fill(egui::Color32::from_rgb(70, 100, 200))
            .rounding(egui::Rounding::same(12.0))
            .inner_margin(egui::Margin::symmetric(10.0, 6.0))
            .show(ui, |ui| {
                // Imposta larghezza ESATTA della bolla
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
                        ui.colored_label(egui::Color32::from_rgb(180, 255, 180), "✓✓");
                        ui.add_space(4.0);
                        let time = format_time(message.created_at);
                        ui.colored_label(egui::Color32::from_rgb(220, 220, 220), time);
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

        egui::Frame::none()
            .fill(egui::Color32::from_rgb(60, 60, 60))
            .rounding(egui::Rounding::same(12.0))
            .inner_margin(egui::Margin::symmetric(10.0, 6.0))
            .show(ui, |ui| {
                ui.set_width(bw);

                ui.vertical(|ui| {
                    ui.colored_label(
                        egui::Color32::from_rgb(150, 255, 150),
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


fn show_input_area(ui: &mut egui::Ui, s: &mut AppState, cid: Uuid, token: &str, height: f32) {
    ui.allocate_ui_with_layout(
        [ui.available_width(), height].into(),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            ui.separator();
            ui.add_space(8.0);

            ui.horizontal(|ui| {
                let input_response = TextEdit::singleline(&mut s.input)
                    .hint_text("Scrivi un messaggio...")
                    .desired_width(ui.available_width() - 70.0)
                    .show(ui);

                if input_response.response.lost_focus()
                    && ui.input(|i| i.key_pressed(egui::Key::Enter))
                {
                    send_message(s, cid, token);
                }

                if ui.button("📤").clicked() {
                    send_message(s, cid, token);
                }
            });

            ui.horizontal(|ui| {
                if s.ws_status == WsStatus::Disconnected {
                    if ui.button("🔌 Riconnetti").clicked() {
                        s.request_ws_reconnect = true;
                    }
                }

                if s.is_loading && !s.is_initial_load_complete {
                    ui.spinner();
                    ui.label("Caricamento...");
                }
            });
        },
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

fn send_message(s: &mut AppState, cid: Uuid, token: &str) {
    let content = s.input.trim().to_string();
    if content.is_empty() {
        return;
    }
    s.input.clear();

    if let Some(user_id) = s.user_id {
        let optimistic_msg = MessageDto {
            id: Uuid::new_v4(),
            author_id: user_id,
            author_username: s.username.clone(),
            content: content.clone(),
            created_at: chrono::Utc::now().timestamp(),
        };

        s.messages.push(optimistic_msg.clone());

        if let Some(msgs) = s.conversation_messages.get_mut(&cid) {
            msgs.push(optimistic_msg);
        }
    }

    let base = s.base.clone();
    let token = token.to_string();
    let tx = s.ui_tx.clone();

    s.rt.spawn(async move {
        match net::chat::send_message(&base, &token, cid, &content).await {
            Ok(_) => {}
            Err(e) => {
                let _ = tx.send(UiEvent::Error(format!("Invio fallito: {e}")));
            }
        }
    });
}

fn format_time(timestamp: i64) -> String {
    use chrono::{DateTime, Utc};

    DateTime::from_timestamp(timestamp, 0)
        .map(|dt| dt.format("%H:%M").to_string())
        .unwrap_or_else(|| "??:??".to_string())
}

fn bubble_width(ui: &egui::Ui, text: &str, max_wrap: f32, pad_x: f32) -> f32 {
    // Misura la larghezza del testo wrappato a max_wrap, poi aggiunge il padding orizzontale
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
        job.wrap.max_width = max_wrap.max(1.0); // evita 0
        let galley = fonts.layout_job(job);
        (galley.rect.size().x + pad_x).clamp(96.0, max_wrap + pad_x) // min 96px, tetto max
    })
}
