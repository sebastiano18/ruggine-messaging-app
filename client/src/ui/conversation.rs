use eframe::egui::{self, TextEdit, RichText, ScrollArea};
use uuid::Uuid;
use crate::{state::{AppState, UiEvent}, net};

pub fn panel(ui: &mut egui::Ui, s: &mut AppState) {
    ui.heading(RichText::new("💬 Conversazioni").size(18.0).strong());
    ui.add_space(8.0);

    if s.token.is_none() {
        ui.centered_and_justified(|ui| {
            ui.colored_label(egui::Color32::RED, RichText::new("🔒 Login richiesto per accedere alle conversazioni").size(14.0));
        });
        return;
    }
    let token = s.token.clone().unwrap();

    // Auto-carica conversazioni se non sono ancora state caricate
    if s.conversations.is_none() {
        let base = s.base.clone();
        let token2 = token.clone();
        let tx = s.ui_tx.clone();
        s.rt.spawn(async move {
            match net::conversation::get_conversations(&base, &token2).await {
                Ok(conversations) => {
                    let _ = tx.send(UiEvent::ConversationsLoaded(conversations));
                }
                Err(e) => {
                    let _ = tx.send(UiEvent::Error(format!("Caricamento conversazioni fallito: {e}")));
                }
            }
        });
    }

    // === Sezione Conversazioni ===
    ui.group(|ui| {
        ui.horizontal(|ui| {
            ui.heading(RichText::new("📋 Le tue conversazioni").size(16.0));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(RichText::new("🔄 Ricarica").small()).clicked() {
                    let base = s.base.clone();
                    let token2 = token.clone();
                    let tx = s.ui_tx.clone();
                    s.rt.spawn(async move {
                        match net::conversation::get_conversations(&base, &token2).await {
                            Ok(conversations) => {
                                let _ = tx.send(UiEvent::ConversationsLoaded(conversations));
                            }
                            Err(e) => {
                                let _ = tx.send(UiEvent::Error(format!("Caricamento conversazioni fallito: {e}")));
                            }
                        }
                    });
                }
            });
        });

        ui.separator();
        ui.add_space(4.0);

        // Lista conversazioni con scroll
        ScrollArea::vertical()
            .max_height(200.0)
            .show(ui, |ui| {
                if let Some(ref conversations) = s.conversations {
                    if conversations.is_empty() {
                        ui.centered_and_justified(|ui| {
                            ui.label(RichText::new("🔭 Nessuna conversazione trovata").color(egui::Color32::GRAY));
                        });
                    } else {
                        for conv in conversations {
                            ui.horizontal(|ui| {
                                let (icon, color) = match conv.kind.as_str() {
                                    "group" => ("👥", egui::Color32::BLUE),
                                    "dm" => ("💬", egui::Color32::GREEN),
                                    _ => ("📄", egui::Color32::GRAY),
                                };

                                let label = format!("{} {}", icon, conv.title);
                                if ui.button(RichText::new(&label).color(color)).clicked() {
                                    let _ = s.ui_tx.send(UiEvent::Opened(conv.id));
                                }
                            });
                        }
                    }
                } else {
                    ui.centered_and_justified(|ui| {
                        ui.label(RichText::new("🔭 Clicca 'Ricarica' per vedere le conversazioni").color(egui::Color32::GRAY));
                    });
                }
            });
    });

    ui.add_space(12.0);

    // === Sezione Azioni Veloci ===
    ui.columns(2, |columns| {
        // Colonna sinistra: Crea gruppo e DM
        columns[0].group(|ui| {
            ui.vertical_centered(|ui| {
                ui.heading(RichText::new("➕ Nuove Conversazioni").size(15.0));
            });
            ui.separator();
            ui.add_space(4.0);

            // Crea gruppo
            ui.label(RichText::new("👥 Crea un nuovo gruppo").strong());
            ui.horizontal(|ui| {
                ui.label("Nome:");
                ui.add(TextEdit::singleline(&mut s.group_name).desired_width(120.0));
            });

            ui.horizontal(|ui| {
                if ui.button(RichText::new("✨ Crea gruppo").color(egui::Color32::WHITE))
                    .on_hover_text("Crea un nuovo gruppo di chat")
                    .clicked() && !s.group_name.trim().is_empty() {
                    let base = s.base.clone();
                    let name = s.group_name.clone();
                    let tx = s.ui_tx.clone();
                    let token2 = token.clone();
                    s.group_name.clear(); // Pulisce il campo dopo la creazione
                    s.rt.spawn(async move {
                        match net::conversation::create_group(&base, &token2, &name).await {
                            Ok(cid) => {
                                let _ = tx.send(UiEvent::Opened(cid));
                            }
                            Err(e) => {
                                let _ = tx.send(UiEvent::Error(format!("Creazione gruppo fallita: {e}")));
                            }
                        }
                    });
                }

                if s.group_name.trim().is_empty() {
                    ui.label(RichText::new("(inserisci un nome)").color(egui::Color32::GRAY));
                }
            });

            ui.add_space(8.0);

            // Inizia DM
            ui.label(RichText::new("💬 Chat privata").strong());
            ui.horizontal(|ui| {
                ui.label("Username:");
                ui.add(TextEdit::singleline(&mut s.dm_user_username_input)
                    .desired_width(120.0)
                    .hint_text("Nome utente"));
            });

            if ui.button(RichText::new("🚀 Inizia DM").color(egui::Color32::WHITE))
                .on_hover_text("Avvia una chat privata")
                .clicked() {

                let target_username = s.dm_user_username_input.trim();

                if target_username.is_empty() {
                    let _ = s.ui_tx.send(UiEvent::Error("Username non può essere vuoto".into()));
                } else {
                    let base = s.base.clone();
                    let tx = s.ui_tx.clone();
                    let token2 = token.clone();
                    let username_clone = target_username.to_string();
                    s.dm_user_username_input.clear(); // Pulisce il campo

                    s.rt.spawn(async move {
                        match net::conversation::create_dm(&base, &token2, username_clone).await {
                            Ok(cid) => {
                                let _ = tx.send(UiEvent::Opened(cid));
                            }
                            Err(e) => {
                                let _ = tx.send(UiEvent::Error(format!("Creazione DM fallita: {e}")));
                            }
                        }
                    });
                }
            }
        });

        // Colonna destra: Join e Inviti
        columns[1].group(|ui| {
            ui.vertical_centered(|ui| {
                ui.heading(RichText::new("🎫 Inviti & Join").size(15.0));
            });
            ui.separator();
            ui.add_space(4.0);

            // Join da token
            ui.label(RichText::new("🔗 Unisciti con token").strong());
            ui.add(TextEdit::singleline(s.last_invite_token.get_or_insert_with(String::new))
                .desired_width(150.0)
                .hint_text("Token di invito"));

            if ui.button(RichText::new("🎯 Join").color(egui::Color32::YELLOW))
                .on_hover_text("Unisciti alla conversazione")
                .clicked() {
                if let Some(ref token_input) = s.last_invite_token {
                    if !token_input.trim().is_empty() {
                        let base = s.base.clone();
                        let token2 = token.clone();
                        let tx = s.ui_tx.clone();
                        let token_input_clone = token_input.clone();
                        s.rt.spawn(async move {
                            match net::conversation::join_by_token(&base, &token2, &token_input_clone).await {
                                Ok(cid) => {
                                    let _ = tx.send(UiEvent::Opened(cid));
                                }
                                Err(e) => {
                                    let _ = tx.send(UiEvent::Error(format!("Join fallito: {e}")));
                                }
                            }
                        });
                    }
                }
            }

            ui.add_space(8.0);

            // Crea invito - usa l'ID della conversazione attualmente aperta o inserita manualmente
            ui.label(RichText::new("🎁 Crea invito").strong());

            ui.horizontal(|ui| {
                if s.cid.is_some() {
                    ui.label("Conv. corrente:");
                    ui.label(RichText::new(&s.current_conversation_title()).color(egui::Color32::GRAY));
                    let conv_id_short = s.cid.unwrap().to_string();
                    ui.label(RichText::new(&format!("({})", &conv_id_short[..8])).color(egui::Color32::GRAY));
                } else {
                    ui.label("Conv. ID:");
                    ui.add(TextEdit::singleline(&mut s.invite_conversation_id)
                        .desired_width(120.0)
                        .hint_text("UUID conversazione"));
                }
            });

            if ui.button(RichText::new("🔮 Genera Invito").color(egui::Color32::LIGHT_BLUE))
                .on_hover_text("Crea un token di invito per questa conversazione")
                .clicked() {

                let conv_uuid = if let Some(current_conv) = s.cid {
                    current_conv
                } else {
                    match Uuid::parse_str(&s.invite_conversation_id.trim()) {
                        Ok(u) => u,
                        Err(_) => {
                            let _ = s.ui_tx.send(UiEvent::Error("UUID conversazione non valido".into()));
                            return;
                        }
                    }
                };

                let base = s.base.clone();
                let token2 = token.clone();
                let tx = s.ui_tx.clone();
                s.rt.spawn(async move {
                    match net::conversation::create_invite(&base, &token2, conv_uuid).await {
                        Ok(invite_token) => {
                            let _ = tx.send(UiEvent::InviteCreated(invite_token));
                        }
                        Err(e) => {
                            let _ = tx.send(UiEvent::Error(format!("Creazione invito fallita: {e}")));
                        }
                    }
                });
            }

            // Mostra l'ultimo invito creato
            if let Some(ref invite) = s.last_created_invite {
                ui.add_space(6.0);
                ui.group(|ui| {
                    ui.label(RichText::new("🎉 Invito creato:").strong().color(egui::Color32::LIGHT_GREEN));
                    ui.horizontal(|ui| {
                        let mut invite_text = invite.clone();
                        ui.add(TextEdit::singleline(&mut invite_text).desired_width(100.0));
                        if ui.small_button("📋").on_hover_text("Copia negli appunti").clicked() {
                            ui.output_mut(|o| o.copied_text = invite_text);
                            let _ = s.ui_tx.send(UiEvent::Info("Invito copiato negli appunti!".into()));
                        }
                    });
                });
            }
        });
    });
}