use eframe::egui::{self, RichText, ScrollArea, Frame, Stroke, TextEdit};
use tokio::runtime::Handle;
use uuid::Uuid;
use crate::{state::{AppState, UiEvent}, net};

// -------------------------------------
// Costanti di stile
// -------------------------------------

const GAP: f32 = 8.0;
const CARD_INNER_PAD: f32 = 10.0;
const MAX_LIST_HEIGHT: f32 = 220.0;

// -------------------------------------
// Helpers di stile & UI
// -------------------------------------

fn section_card(ui: &mut egui::Ui, title_emoji: &str, title: &str, body: impl FnOnce(&mut egui::Ui)) {
    let visuals = ui.visuals().clone();
    Frame::group(ui.style())
        .fill(visuals.panel_fill.linear_multiply(0.15)) // piÃ¹ chiaro
        .stroke(Stroke::new(
            0.5,
            visuals.widgets.noninteractive.bg_stroke.color.linear_multiply(0.4),
        ))
        .inner_margin(egui::Margin::symmetric(CARD_INNER_PAD, CARD_INNER_PAD))
        .rounding(egui::Rounding::same(8.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(title_emoji).size(18.0));
                ui.add_space(4.0);
                ui.label(RichText::new(title).size(15.0).strong());
            });
            ui.separator();
            ui.add_space(GAP);
            body(ui);
        });
}



/// Riga clickabile con hover piÃ¹ sottile e meno scuro
fn entry_row(ui: &mut egui::Ui, icon: &str, title: &str) -> egui::Response {
    let row_h = 26.0;
    let row_w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(row_w, row_h), egui::Sense::click());

    if resp.hovered() {
        // Overlay piÃ¹ intelligente che si adatta al tema
        let is_dark_theme = ui.visuals().dark_mode;
        let overlay_color = if is_dark_theme {
            // Tema scuro: overlay bianco molto sottile
            egui::Color32::WHITE.linear_multiply(0.04)
        } else {
            // Tema chiaro: overlay scuro molto sottile  
            egui::Color32::BLACK.linear_multiply(0.03)
        };

        ui.painter()
            .rect_filled(
                rect.shrink2(egui::vec2(2.0, 2.0)),
                6.0,
                overlay_color
            );
    }

    ui.allocate_ui_at_rect(rect.shrink2(egui::vec2(8.0, 4.0)), |ui| {
        ui.horizontal(|ui| {
            ui.label(icon);
            ui.label(title);
        });
    });

    resp
}

fn labeled_text(ui: &mut egui::Ui, label: &str, text: &mut String, hint: &str, width: f32) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.add(TextEdit::singleline(text).desired_width(width).hint_text(hint));
    });
}

fn labeled_mono_text(ui: &mut egui::Ui, label: &str, text: &mut String, hint: &str, width: f32) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.add(
            TextEdit::singleline(text)
                .desired_width(width)
                .hint_text(hint)
                .font(egui::TextStyle::Monospace),
        );
    });
}

fn combo_conversations(ui: &mut egui::Ui, s: &mut AppState) {
    if let Some(ref conversations) = s.conversations {
        ui.horizontal(|ui| {
            ui.label("Gruppo da condividere:");
            egui::ComboBox::from_id_source("conv_combo_invite")
                .selected_text({
                    if let Some(cid) = s.cid {
                        if let Some(conv) = conversations.iter().find(|c| c.id == cid) {
                            format!(
                                "{} {}",
                                if conv.kind == "group" { "👥" } else { "💬" },
                                conv.title
                            )
                        } else {
                            "Selezionaâ€¦".to_string()
                        }
                    } else {
                        "Selezionaâ€¦".to_string()
                    }
                })
                .show_ui(ui, |ui| {
                    for conv in conversations {
                        let icon = if conv.kind == "group" { "👥" } else { "💬" };
                        let label = format!("{icon} {}", conv.title);
                        if ui
                            .selectable_value(&mut s.cid, Some(conv.id), label)
                            .clicked()
                        {
                            s.conv_title = conv.title.clone();
                        }
                    }
                });
        });
    } else {
        labeled_mono_text(
            ui,
            "Conversazione ID:",
            &mut s.invite_conversation_id,
            "UUID conversazione",
            180.0,
        );
    }
}

// -------------------------------------
// Pannello principale
// -------------------------------------

pub fn panel(ui: &mut egui::Ui, s: &mut AppState) {
    ui.heading(RichText::new("ðŸ”§ Gestione Gruppi & Inviti").size(18.0).strong());
    ui.add_space(GAP);

    if s.token.is_none() {
        ui.vertical_centered(|ui| {
            ui.colored_label(egui::Color32::RED, RichText::new("Login richiesto").size(14.0));
        });
        return;
    }
    let token = s.token.clone().unwrap();
    let rt_handle = s.rt.handle().clone(); // usare Handle per evitare borrow di s nelle closure

    // Layout responsivo: due colonne se câ€™Ã¨ spazio, altrimenti impila
    let wide = ui.available_width() > 720.0;

    if wide {
        ui.columns(2, |columns| {
            left_column(&mut columns[0], s, &token, &rt_handle);
            right_column(&mut columns[1], s, &token, &rt_handle);
        });
    } else {
        left_column(ui, s, &token, &rt_handle);
        ui.add_space(GAP * 2.0);
        right_column(ui, s, &token, &rt_handle);
    }

    ui.add_space(GAP * 2.0);
    conversations_section(ui, s, &token, &rt_handle);
}

// -------------------------------------
// Colonna sinistra: crea chat
// -------------------------------------

fn left_column(ui: &mut egui::Ui, s: &mut AppState, token: &str, rt: &Handle) {
    section_card(ui, "💬", "Crea Nuove Chat", |ui| {
        // Crea gruppo
        section_card(ui, "👥", "Nuovo Gruppo", |ui| {
            ui.label(
                RichText::new("Crea un gruppo di chat")
                    .small()
                    .color(egui::Color32::GRAY),
            );
            ui.add_space(GAP);
            labeled_text(ui, "Nome gruppo:", &mut s.group_name, "Es: Team Alpha", 200.0);

            ui.add_space(GAP);
            let can_create = !s.group_name.trim().is_empty();
            ui.add_enabled_ui(can_create, |ui| {
                if ui
                    .button(RichText::new("👥¨ Crea Gruppo").color(egui::Color32::WHITE))
                    .clicked()
                {
                    let base = s.base.clone();
                    let name = s.group_name.trim().to_string();
                    let tx = s.ui_tx.clone();
                    let token2 = token.to_string();
                    s.group_name.clear();

                    rt.spawn(async move {
                        match net::conversation::create_group(&base, &token2, &name).await {
                            Ok(cid) => {
                                let _ = tx.send(UiEvent::Opened(cid));
                                let _ = tx.send(UiEvent::Info("Gruppo creato con successo!".into()));
                            }
                            Err(e) => {
                                let _ = tx
                                    .send(UiEvent::Error(format!("Creazione gruppo fallita: {e}")));
                            }
                        }
                    });
                }
            });
            if !can_create {
                ui.label(
                    RichText::new("Inserisci un nome per creare il gruppo.")
                        .small()
                        .color(egui::Color32::GRAY),
                );
            }
        });

        ui.add_space(GAP * 1.5);

        // Chat privata (DM)
        section_card(ui, "ðŸ’¬", "Chat Privata", |ui| {
            ui.label(
                RichText::new("Inizia una conversazione 1-a-1")
                    .small()
                    .color(egui::Color32::GRAY),
            );
            ui.add_space(GAP);
            labeled_text(
                ui,
                "Destinatario:",
                &mut s.dm_user_username_input,
                "Username",
                200.0,
            );

            ui.add_space(GAP);
            let can_dm = !s.dm_user_username_input.trim().is_empty();
            ui.add_enabled_ui(can_dm, |ui| {
                if ui
                    .button(RichText::new("ðŸš€ Inizia Chat").color(egui::Color32::WHITE))
                    .clicked()
                {
                    let target_username = s.dm_user_username_input.trim().to_owned();
                    let base = s.base.clone();
                    let tx = s.ui_tx.clone();
                    let token2 = token.to_string();
                    s.dm_user_username_input.clear();

                    rt.spawn(async move {
                        match net::conversation::create_dm(&base, &token2, target_username).await {
                            Ok(cid) => {
                                let _ = tx.send(UiEvent::Opened(cid));
                                let _ = tx.send(UiEvent::Info("Chat privata avviata!".into()));
                            }
                            Err(e) => {
                                let _ = tx.send(UiEvent::Error(format!("Creazione DM fallita: {e}")));
                            }
                        }
                    });
                }
            });
            if !can_dm {
                ui.label(
                    RichText::new("Inserisci un username per avviare la chat.")
                        .small()
                        .color(egui::Color32::GRAY),
                );
            }
        });
    });
}

// -------------------------------------
// Colonna destra: inviti e join
// -------------------------------------

fn right_column(ui: &mut egui::Ui, s: &mut AppState, token: &str, rt: &Handle) {
    section_card(ui, "ðŸŽ«", "Sistema Inviti", |ui| {
        // Join con token
        section_card(ui, "ðŸ”—", "Unisciti a un Gruppo", |ui| {
            ui.label(
                RichText::new("Usa un token di invito")
                    .small()
                    .color(egui::Color32::GRAY),
            );
            ui.add_space(GAP);

            let invite_token = s.last_invite_token.get_or_insert_with(String::new);
            labeled_mono_text(ui, "Token:", invite_token, "abc123def456", 220.0);

            ui.add_space(GAP);
            let can_join = !invite_token.trim().is_empty();
            ui.add_enabled_ui(can_join, |ui| {
                if ui
                    .button(RichText::new("ðŸŽ¯ Unisciti").color(egui::Color32::YELLOW))
                    .clicked()
                {
                    let base = s.base.clone();
                    let token2 = token.to_string();
                    let tx = s.ui_tx.clone();
                    let token_input_clone = invite_token.trim().to_owned();

                    rt.spawn(async move {
                        match net::conversation::join_by_token(&base, &token2, &token_input_clone)
                            .await
                        {
                            Ok(cid) => {
                                let _ = tx.send(UiEvent::Opened(cid));
                                let _ = tx.send(UiEvent::Info("Ti sei unito al gruppo!".into()));
                            }
                            Err(e) => {
                                let _ = tx.send(UiEvent::Error(format!("Join fallito: {e}")));
                            }
                        }
                    });
                }
            });
            if !can_join {
                ui.label(
                    RichText::new("Inserisci un token valido per procedere.")
                        .small()
                        .color(egui::Color32::GRAY),
                );
            }
        });

        ui.add_space(GAP * 1.5);

        // Crea invito
        section_card(ui, "ðŸŽ", "Genera Invito", |ui| {
            ui.label(
                RichText::new("Invita altri nei tuoi gruppi")
                    .small()
                    .color(egui::Color32::GRAY),
            );
            ui.add_space(GAP);

            combo_conversations(ui, s);

            ui.add_space(GAP);
            let can_invite = s.cid.is_some() || !s.invite_conversation_id.trim().is_empty();
            ui.add_enabled_ui(can_invite, |ui| {
                if ui
                    .button(RichText::new("ðŸ”® Genera Token").color(egui::Color32::LIGHT_BLUE))
                    .clicked()
                {
                    let conv_uuid = if let Some(current_conv) = s.cid {
                        current_conv
                    } else {
                        match Uuid::parse_str(s.invite_conversation_id.trim()) {
                            Ok(u) => u,
                            Err(_) => {
                                let _ = s
                                    .ui_tx
                                    .send(UiEvent::Error("UUID conversazione non valido".into()));
                                return;
                            }
                        }
                    };

                    let base = s.base.clone();
                    let token2 = token.to_string();
                    let tx = s.ui_tx.clone();

                    rt.spawn(async move {
                        match net::conversation::create_invite(&base, &token2, conv_uuid).await {
                            Ok(invite_token) => {
                                let _ = tx.send(UiEvent::InviteCreated(invite_token));
                            }
                            Err(e) => {
                                let _ = tx
                                    .send(UiEvent::Error(format!("Creazione invito fallita: {e}")));
                            }
                        }
                    });
                }
            });
            if !can_invite {
                ui.label(
                    RichText::new("Seleziona un gruppo (o inserisci un UUID) per generare un invito.")
                        .small()
                        .color(egui::Color32::GRAY),
                );
            }

            if let Some(ref invite) = s.last_created_invite {
                ui.add_space(GAP);
                Frame::group(ui.style())
                    .fill(ui.visuals().extreme_bg_color.linear_multiply(0.6))
                    .inner_margin(egui::Margin::same(8.0))
                    .rounding(egui::Rounding::same(6.0))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new("ðŸŽ‰ Token generato:")
                                    .strong()
                                    .color(egui::Color32::LIGHT_GREEN),
                            );
                            let mut invite_text = invite.clone();
                            ui.add(
                                TextEdit::singleline(&mut invite_text)
                                    .desired_width(220.0)
                                    .font(egui::TextStyle::Monospace),
                            );
                            if ui.button(RichText::new("ðŸ“‹ Copia").small()).clicked() {
                                ui.output_mut(|o| o.copied_text = invite_text);
                                let _ = s
                                    .ui_tx
                                    .send(UiEvent::Info("Token copiato negli appunti!".into()));
                            }
                        });
                        ui.label(
                            RichText::new("Condividi questo token per invitare altri utenti.")
                                .small()
                                .color(egui::Color32::GRAY),
                        );
                    });
            }
        });
    });
}

// -------------------------------------
// Sezione conversazioni (lista) â€” borrow-safe
// -------------------------------------

fn conversations_section(ui: &mut egui::Ui, s: &mut AppState, token: &str, rt: &Handle) {
    ui.separator();
    ui.heading(RichText::new("ðŸ“Š Le Tue Conversazioni").size(16.0));
    ui.add_space(GAP);

    if let Some(ref conversations) = s.conversations {
        // === Snapshot dei dati necessari (evita borrow su s durante le closure) ===
        #[derive(Clone)]
        struct RowData {
            id: Uuid,
            title: String,
            kind: String,
        }

        let mut groups: Vec<RowData> = Vec::new();
        let mut dms: Vec<RowData> = Vec::new();
        for c in conversations {
            let row = RowData {
                id: c.id,
                title: c.title.clone(),
                kind: c.kind.clone(),
            };
            if c.kind == "group" {
                groups.push(row);
            } else if c.kind == "dm" {
                dms.push(row);
            }
        }

        // Titolo chat attiva (opzionale)
        let active_title: Option<String> = if let Some(cid) = s.cid {
            conversations.iter().find(|c| c.id == cid).map(|c| c.title.clone())
        } else {
            None
        };

        // Clona ciÃ² che serve per gli handler
        let tx_main = s.ui_tx.clone();
        let base_main = s.base.clone();
        let token_main = token.to_string();

        ScrollArea::vertical()
            .max_height(MAX_LIST_HEIGHT)
            .show(ui, |ui| {
                ui.columns(3, |columns| {
                    // --- COLONNA GRUPPI ---
                    section_card(&mut columns[0], "ðŸ‘¥", "Gruppi", |ui| {
                        if groups.is_empty() {
                            ui.label(RichText::new("Nessun gruppo").color(egui::Color32::GRAY));
                        } else {
                            for g in &groups {
                                let resp = entry_row(ui, "ðŸ‘¥", &g.title);
                                if resp.clicked() {
                                    let _ = tx_main.send(UiEvent::Opened(g.id));
                                }
                                ui.add_space(2.0);
                            }
                        }
                    });

                    // --- COLONNA DM ---
                    section_card(&mut columns[1], "ðŸ’¬", "Chat Private", |ui| {
                        if dms.is_empty() {
                            ui.label(RichText::new("Nessuna chat privata").color(egui::Color32::GRAY));
                        } else {
                            for d in &dms {
                                let resp = entry_row(ui, "ðŸ’¬", &d.title);
                                if resp.clicked() {
                                    let _ = tx_main.send(UiEvent::Opened(d.id));
                                }
                                ui.add_space(2.0);
                            }
                        }
                    });

                    // --- COLONNA AZIONI ---
                    section_card(&mut columns[2], "ðŸ”§", "Azioni Rapide", |ui| {
                        if ui.button("ðŸ”„ Ricarica Liste").clicked() {
                            let base = base_main.clone();
                            let token2 = token_main.clone();
                            let tx = tx_main.clone();
                            rt.spawn(async move {
                                match net::conversation::get_conversations(&base, &token2).await {
                                    Ok(conversations) => {
                                        let _ = tx.send(UiEvent::ConversationsLoaded(conversations));
                                    }
                                    Err(e) => {
                                        let _ = tx.send(UiEvent::Error(format!(
                                            "Caricamento conversazioni fallito: {e}"
                                        )));
                                    }
                                }
                            });
                        }

                        if let Some(title) = active_title.as_ref() {
                            ui.add_space(GAP);
                            ui.label(RichText::new("Chat attiva:").small());
                            ui.label(RichText::new(title).small().strong());
                        }
                    });
                });
            });
    } else {
        ui.vertical_centered(|ui| {
            ui.spinner();
            ui.label("Caricamento conversazioniâ€¦");
        });
    }
}