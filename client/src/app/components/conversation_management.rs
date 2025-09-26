use crate::models::UiEvent;
use crate::api;
use crate::state::AppState;
use eframe::egui::{self, Align, Frame, Layout, RichText, ScrollArea, Stroke, TextEdit};
use tokio::runtime::Handle;
use tracing::{info, warn, error, debug};
use uuid::Uuid;

// Costanti di stile per matching con sidebar
const SECTION_MARGIN: egui::Margin = egui::Margin::symmetric(10.0, 8.0);
const SECTION_ROUNDING: egui::Rounding = egui::Rounding::same(8.0);

// Helper per creare frame in stile sidebar
fn styled_frame(ui: &egui::Ui) -> Frame {
    Frame::group(ui.style())
        .fill(egui::Color32::from_rgb(255, 140, 60).linear_multiply(0.06))
        .stroke(Stroke::new(
            0.5,
            egui::Color32::from_rgb(240, 140, 80).linear_multiply(0.3),
        ))
        .inner_margin(SECTION_MARGIN)
        .rounding(SECTION_ROUNDING)
}

fn section_card(
    ui: &mut egui::Ui,
    title_icon: &str,
    title: &str,
    body: impl FnOnce(&mut egui::Ui),
) {
    styled_frame(ui).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(title_icon)
                    .size(16.0)
                    .color(egui::Color32::from_rgb(200, 100, 40)),
            );
            ui.add_space(6.0);
            ui.label(
                RichText::new(title)
                    .size(14.0)
                    .strong()
                    .color(egui::Color32::from_rgb(220, 160, 100)),
            );
        });
        ui.add_space(6.0);
        body(ui);
    });
}

fn labeled_text(ui: &mut egui::Ui, label: &str, text: &mut String, hint: &str) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).color(egui::Color32::from_rgb(180, 120, 70)));
        ui.add(
            TextEdit::singleline(text)
                .hint_text(hint)
                .desired_width(ui.available_width()),
        );
    });
}

fn labeled_mono_text(ui: &mut egui::Ui, label: &str, text: &mut String, hint: &str) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).color(egui::Color32::from_rgb(180, 120, 70)));
        ui.add(
            TextEdit::singleline(text)
                .hint_text(hint)
                .font(egui::TextStyle::Monospace)
                .desired_width(ui.available_width()),
        );
    });
}

fn action_button(ui: &mut egui::Ui, text: &str, enabled: bool) -> bool {
    ui.add_enabled(
        enabled,
        egui::Button::new(RichText::new(text).color(if enabled {
            egui::Color32::WHITE
        } else {
            egui::Color32::GRAY
        })),
    )
        .clicked()
}

fn combo_conversations(ui: &mut egui::Ui, s: &mut AppState) {
    if let Some(ref conversations) = s.conversations {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("Gruppo da condividere:")
                    .color(egui::Color32::from_rgb(180, 120, 70)),
            );

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
                            "Seleziona...".to_string()
                        }
                    } else {
                        "Seleziona...".to_string()
                    }
                })
                .show_ui(ui, |ui| {
                    for conv in conversations {
                        let icon = if conv.kind == "group" { "👥" } else { "💬" };
                        let label = format!("{} {}", icon, conv.title);
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
        );
    }
}

pub fn panel(ui: &mut egui::Ui, s: &mut AppState) {
    // Header principale con stile matching
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("🔧 Gestione Gruppi & Inviti")
                .heading()
                .strong(),
        );

        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui
                .small_button("🏠")
                .on_hover_text("Torna alle conversazioni")
                .clicked()
            {
                s.page = crate::models::Page::Chat;
            }
        });
    });

    ui.separator();
    ui.add_space(6.0);

    if s.token.is_none() {
        ui.vertical_centered(|ui| {
            ui.colored_label(
                egui::Color32::RED,
                "Login richiesto per gestire gruppi e inviti",
            );
        });
        return;
    }

    let token = s.token.clone().unwrap();
    let rt_handle = s.rt.handle().clone();

    ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            // Sezione creazione nuove chat
            create_chats_section(ui, s, &token, &rt_handle);
            ui.add_space(12.0);

            // Sezione sistema inviti
            invites_section(ui, s, &token, &rt_handle);
            ui.add_space(12.0);
        });
}

fn create_chats_section(ui: &mut egui::Ui, s: &mut AppState, token: &str, rt: &Handle) {
    section_card(ui, "💬", "Crea Nuove Chat", |ui| {
        ui.columns(2, |columns| {
            // Nuovo gruppo
            create_group_subsection(&mut columns[0], s, token, rt);

            // Chat privata
            create_dm_subsection(&mut columns[1], s);
        });
    });
}

fn create_group_subsection(ui: &mut egui::Ui, s: &mut AppState, token: &str, rt: &Handle) {
    styled_frame(ui).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("👥")
                    .size(16.0)
                    .color(egui::Color32::from_rgb(255, 140, 60)),
            );
            ui.add_space(6.0);
            ui.label(
                RichText::new("Nuovo Gruppo")
                    .strong()
                    .color(egui::Color32::from_rgb(220, 160, 100)),
            );
        });

        ui.add_space(8.0);

        labeled_text(ui, "Nome gruppo:", &mut s.group_name, "Es: Team Alpha");

        ui.add_space(8.0);

        let can_create = !s.group_name.trim().is_empty();
        if action_button(ui, "➕ Crea Gruppo", can_create) {
            let base = s.base.clone();
            let name = s.group_name.trim().to_string();
            let tx = s.ui_tx.clone();
            let token2 = token.to_string();
            s.group_name.clear();

            rt.spawn(async move {
                match api::conversation::create_group(&base, &token2, &name).await {
                    Ok(cid) => {
                        let _ = tx.send(UiEvent::Info("Gruppo creato! Aggiornamento lista...".into()));

                        // Attendi un momento per permettere al server di processare
                        tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;

                        // Ricarica le conversazioni
                        match crate::api::conversation::get_conversations(&base, &token2).await {
                            Ok(conversations) => {
                                // Invia la lista aggiornata
                                let _ = tx.send(UiEvent::ConversationsLoaded(conversations));

                                // Prova ad aprire la conversazione
                                let _ = tx.send(UiEvent::Opened(cid));
                                let _ = tx.send(UiEvent::Info("Gruppo creato con successo!".into()));
                            }
                            Err(e) => {
                                let _ = tx.send(UiEvent::Error(format!(
                                    "Gruppo creato ma errore nel refresh: {}",
                                    e
                                )));
                            }
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(UiEvent::Error(format!(
                            "Creazione gruppo fallita: {}",
                            e
                        )));
                    }
                }
            });
        }

        if !can_create {
            ui.add_space(4.0);
            ui.label(
                RichText::new("💡 Inserisci un nome per creare il gruppo")
                    .size(11.0)
                    .color(egui::Color32::from_rgb(160, 100, 60)),
            );
        }
    });
}

fn create_dm_subsection(ui: &mut egui::Ui, s: &mut AppState) {
    styled_frame(ui).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("💬")
                    .size(16.0)
                    .color(egui::Color32::from_rgb(255, 180, 100)),
            );
            ui.add_space(6.0);
            ui.label(
                RichText::new("Chat Privata")
                    .strong()
                    .color(egui::Color32::from_rgb(220, 160, 100)),
            );
        });

        ui.add_space(8.0);

        labeled_text(
            ui,
            "Destinatario:",
            &mut s.dm_user_username_input,
            "Username",
        );

        ui.add_space(8.0);

        let can_dm = !s.dm_user_username_input.trim().is_empty();

        if action_button(ui, "🚀 Inizia Chat", can_dm) {
            let target_username = s.dm_user_username_input.trim().to_owned();

            // CRITICO: Genera UUID univoco per lo stub locale
            // Questo UUID sarà usato come client_temp_id quando invieremo il primo messaggio
            let stub_conversation_id = Uuid::new_v4();
            info!("Creating DM stub with ID: {} for target user: {}", 
                  stub_conversation_id, target_username);

            // CONTROLLO DUPLICATI CONVERSAZIONI: Verifica se esiste già una conversazione con questo target
            let mut duplicate_found = false;

            // Prima controlla tra le conversazioni esistenti
            if let Some(ref conversations) = s.conversations {
                for conv in conversations {
                    if conv.kind == "dm" && conv.title == target_username {
                        warn!("DM conversation with {} already exists: {}", 
                              target_username, conv.id);
                        let _ = s.ui_tx.send(UiEvent::Info(format!(
                            "Chat con {} già esistente",
                            target_username
                        )));
                        // Apri la conversazione esistente
                        let _ = s.ui_tx.send(UiEvent::Opened(conv.id));
                        duplicate_found = true;
                        break;
                    }
                }
            }

            // CONTROLLO DUPLICATI STUB: Verifica se esiste già uno stub per questo target
            if !duplicate_found {
                for (existing_stub_id, existing_target) in &s.dm_stubs {
                    if existing_target == &target_username {
                        warn!("DM stub for {} already exists: {}", 
                              target_username, existing_stub_id);
                        let _ = s.ui_tx.send(UiEvent::Info(format!(
                            "Chat con {} già in preparazione",
                            target_username
                        )));
                        // Apri lo stub esistente
                        let _ = s.ui_tx.send(UiEvent::Opened(*existing_stub_id));
                        duplicate_found = true;
                        break;
                    }
                }
            }

            if !duplicate_found {
                // IMPORTANTE FLOW:
                // 1. Lo stub usa il suo UUID come identificatore locale
                // 2. Quando invieremo il primo messaggio, useremo questo UUID come client_temp_id
                // 3. Il server creerà la conversazione reale e restituirà il client_temp_id nella conferma
                // 4. Useremo il client_temp_id per trovare e rimuovere questo stub

                info!("Creating new DM stub: {} -> {}", stub_conversation_id, target_username);
                debug!("This stub UUID will be used as client_temp_id: {}", stub_conversation_id);

                // Invia evento per creare lo stub
                let _ = s.ui_tx.send(UiEvent::DmStubCreated(
                    stub_conversation_id,
                    target_username.clone()
                ));

                // Pulisci l'input solo se lo stub è stato creato con successo
                s.dm_user_username_input.clear();

                // Feedback positivo all'utente
                let _ = s.ui_tx.send(UiEvent::Info(format!(
                    "Chat con {} pronta - invia il primo messaggio per iniziare!",
                    target_username
                )));

                // Log dettagliato per debug
                debug!("DM stub created successfully:");
                debug!("  Stub ID: {}", stub_conversation_id);
                debug!("  Target: {}", target_username);
                debug!("  Will use as client_temp_id when sending first message");
            } else {
                // Se trovato duplicato, non pulire l'input per permettere all'utente di correggere
                debug!("Duplicate DM found for {}, not creating new stub", target_username);
            }
        }

        if !can_dm {
            ui.add_space(4.0);
            ui.label(
                RichText::new("💡 Inserisci un username per avviare la chat")
                    .size(11.0)
                    .color(egui::Color32::from_rgb(160, 100, 60)),
            );
        }
    });
}

fn invites_section(ui: &mut egui::Ui, s: &mut AppState, token: &str, rt: &Handle) {
    section_card(ui, "🎫", "Sistema Inviti", |ui| {
        ui.columns(2, |columns| {
            // Join con token
            join_by_token_subsection(&mut columns[0], s, token, rt);

            // Genera invito
            generate_invite_subsection(&mut columns[1], s, token, rt);
        });

        // Token generato (mostrato sotto le colonne)
        show_generated_token(ui, s);
    });
}

fn join_by_token_subsection(ui: &mut egui::Ui, s: &mut AppState, token: &str, rt: &Handle) {
    styled_frame(ui).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("🔗")
                    .size(16.0)
                    .color(egui::Color32::from_rgb(200, 140, 80)),
            );
            ui.add_space(6.0);
            ui.label(
                RichText::new("Unisciti a un Gruppo")
                    .strong()
                    .color(egui::Color32::from_rgb(220, 160, 100)),
            );
        });

        ui.add_space(8.0);

        let invite_token = s.last_invite_token.get_or_insert_with(String::new);
        labeled_mono_text(ui, "Token:", invite_token, "abc123def456");

        ui.add_space(8.0);

        let can_join = !invite_token.trim().is_empty();
        if action_button(ui, "🎯 Unisciti", can_join) {
            let base = s.base.clone();
            let token2 = token.to_string();
            let tx = s.ui_tx.clone();
            let token_input_clone = invite_token.trim().to_owned();

            rt.spawn(async move {
                match api::conversation::join_by_token(&base, &token2, &token_input_clone)
                    .await
                {
                    Ok(cid) => {
                        let _ = tx.send(UiEvent::Info("Unito al gruppo! Aggiornamento lista...".into()));

                        // Attendi un momento per permettere al server di processare
                        tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;

                        // Ricarica le conversazioni
                        match crate::api::conversation::get_conversations(&base, &token2).await {
                            Ok(conversations) => {
                                let _ = tx.send(UiEvent::ConversationsLoaded(conversations));
                                let _ = tx.send(UiEvent::Opened(cid));
                                let _ = tx.send(UiEvent::Info("Ti sei unito al gruppo!".into()));
                            }
                            Err(e) => {
                                let _ = tx.send(UiEvent::Error(format!(
                                    "Unito al gruppo ma errore nel refresh: {}",
                                    e
                                )));
                            }
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(UiEvent::Error(format!("Join fallito: {}", e)));
                    }
                }
            });
        }

        if !can_join {
            ui.add_space(4.0);
            ui.label(
                RichText::new("💡 Inserisci un token valido per procedere")
                    .size(11.0)
                    .color(egui::Color32::from_rgb(160, 100, 60)),
            );
        }
    });
}

fn generate_invite_subsection(ui: &mut egui::Ui, s: &mut AppState, token: &str, rt: &Handle) {
    styled_frame(ui).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("🎟")
                    .size(16.0)
                    .color(egui::Color32::from_rgb(140, 180, 220)),
            );
            ui.add_space(6.0);
            ui.label(
                RichText::new("Genera Invito")
                    .strong()
                    .color(egui::Color32::from_rgb(220, 160, 100)),
            );
        });

        ui.add_space(8.0);

        combo_conversations(ui, s);

        ui.add_space(8.0);

        let can_invite = s.cid.is_some() || !s.invite_conversation_id.trim().is_empty();
        if action_button(ui, "🔮 Genera Token", can_invite) {
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
                match api::conversation::create_invite(&base, &token2, conv_uuid).await {
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

        if !can_invite {
            ui.add_space(4.0);
            ui.label(
                RichText::new("💡 Seleziona un gruppo per generare un invito")
                    .size(11.0)
                    .color(egui::Color32::from_rgb(160, 100, 60)),
            );
        }
    });
}

fn show_generated_token(ui: &mut egui::Ui, s: &mut AppState) {
    if let Some(ref invite) = s.last_created_invite {
        ui.add_space(12.0);
        styled_frame(ui).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("🎉")
                        .size(16.0)
                        .color(egui::Color32::LIGHT_GREEN),
                );
                ui.add_space(6.0);
                ui.label(
                    RichText::new("Token generato:")
                        .strong()
                        .color(egui::Color32::LIGHT_GREEN),
                );
            });

            ui.add_space(6.0);

            ui.horizontal(|ui| {
                let mut invite_text = invite.clone();
                ui.add(
                    TextEdit::singleline(&mut invite_text)
                        .desired_width(ui.available_width() - 60.0)
                        .font(egui::TextStyle::Monospace),
                );
                if ui
                    .button(RichText::new("📋").size(16.0))
                    .on_hover_text("Copia")
                    .clicked()
                {
                    ui.output_mut(|o| o.copied_text = invite_text);
                    let _ = s
                        .ui_tx
                        .send(UiEvent::Info("Token copiato negli appunti!".into()));
                }
            });

            ui.add_space(4.0);
            ui.label(
                RichText::new("💡 Condividi questo token per invitare altri utenti")
                    .size(11.0)
                    .color(egui::Color32::from_rgb(160, 100, 60)),
            );
        });
    }
}