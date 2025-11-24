use crate::api;
use crate::models::{ConversationDto, MessageDto, Outgoing, Page, UiEvent};
use crate::state::AppState;
use eframe::egui::{self, Align, Frame, Layout, RichText, ScrollArea, Stroke, TextEdit};
use std::collections::HashSet;
use tokio::runtime::Handle;
use tracing::{debug, error, info, warn};
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
            // Nuovo gruppo - ora con popup
            create_group_button(&mut columns[0], s);

            // Chat privata
            create_dm_subsection(&mut columns[1], s);
        });
    });
}

fn create_group_button(ui: &mut egui::Ui, s: &mut AppState) {
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

        ui.vertical_centered(|ui| {
            if ui
                .button(
                    RichText::new("➕ Crea Gruppo")
                        .size(14.0)
                        .color(egui::Color32::WHITE),
                )
                .clicked()
            {
                s.show_create_group_modal = true;
            }
        });

        ui.add_space(4.0);
        ui.label(
            RichText::new("💡 Crea un gruppo e aggiungi partecipanti")
                .size(11.0)
                .color(egui::Color32::from_rgb(160, 100, 60)),
        );
    });
}


pub fn show_create_group_modal(ctx: &egui::Context, s: &mut AppState) {
    let mut open = s.show_create_group_modal;
    let mut should_close = false;

    egui::Window::new("")
        .collapsible(false)
        .resizable(false)
        .title_bar(false)
        .fixed_size([500.0, 600.0])
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .open(&mut open)
        .show(ctx, |ui| {
            // Titolo centrato
            ui.vertical_centered(|ui| {
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
                            let name_field = TextEdit::singleline(&mut s.create_group_popup.group_name)
                                .hint_text("Es: Team Alpha")
                                .desired_width(available_width);

                            let response = ui.add(name_field);

                            if s.create_group_popup.group_name.trim().is_empty() && response.changed() {
                                ui.painter().rect_stroke(
                                    response.rect,
                                    2.0,
                                    egui::Stroke::new(1.5, ui.visuals().error_fg_color)
                                );
                            }
                        });
                    });

                    ui.add_space(20.0);

                    // Calcola se mostrare il bottone aggiungi
                    let search_text = s.create_group_popup.search_query.trim().to_string();
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
                        && !s.create_group_popup.selected_participants.contains(&search_text);

                    // Barra di ricerca con bottone aggiungi
                    ui.horizontal(|ui| {
                        ui.add_space(8.0);
                        ui.label(RichText::new(egui_remixicon::icons::SEARCH_LINE).size(20.0));
                        ui.add_space(12.0);
                        ui.vertical(|ui| {
                            ui.label(RichText::new("Cerca o aggiungi utenti").size(13.0).weak());

                            ui.horizontal(|ui| {
                                // Campo di ricerca - stessa larghezza del campo nome gruppo
                                let available_width = ui.available_width() - 50.0;
                                let search_response = ui.add(
                                    TextEdit::singleline(&mut s.create_group_popup.search_query)
                                        .hint_text("Cerca nei contatti o scrivi username...")
                                        .desired_width(available_width - 80.0)
                                );

                                // Enter per aggiungere
                                if search_response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                                    if show_add_button {
                                        s.create_group_popup.selected_participants.insert(search_text.clone());
                                        s.create_group_popup.search_query.clear();
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
                                    s.create_group_popup.selected_participants.insert(search_text.clone());
                                    s.create_group_popup.search_query.clear();
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

                    // Scroll area contatti disponibili con match esatto
                    if let Some(ref conversations) = s.conversations {
                        let dm_conversations: Vec<_> = conversations
                            .iter()
                            .filter(|c| c.kind == "dm")
                            .collect();

                        let search_lower = s.create_group_popup.search_query.trim().to_lowercase();
                        let filtered_dms: Vec<_> = dm_conversations
                            .iter()
                            .filter(|c| {
                                if search_lower.is_empty() {
                                    true
                                } else {
                                    // Match esatto: il nome deve iniziare con il testo cercato
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

                                ScrollArea::vertical()
                                    .max_height(fixed_height)
                                    .min_scrolled_height(fixed_height)
                                    .id_source("available_contacts_scroll")
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
                                                let is_selected = s.create_group_popup.selected_participants.contains(&username);

                                                ui.horizontal(|ui| {
                                                    ui.spacing_mut().item_spacing.x = 0.0;

                                                    let mut selected = is_selected;
                                                    if ui.checkbox(&mut selected, "").changed() {
                                                        if selected {
                                                            s.create_group_popup.selected_participants.insert(username.clone());
                                                        } else {
                                                            s.create_group_popup.selected_participants.remove(&username);
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
                        .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(200, 100, 40)))
                        .inner_margin(10.0)
                        .rounding(6.0)
                        .show(ui, |ui| {
                            let fixed_height = 140.0;

                            ScrollArea::vertical()
                                .max_height(fixed_height)
                                .min_scrolled_height(fixed_height)
                                .id_source("selected_participants_scroll")
                                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
                                .show(ui, |ui| {
                                    ui.set_min_width(ui.available_width());
                                    ui.set_min_height(fixed_height);

                                    if s.create_group_popup.selected_participants.is_empty() {
                                        ui.vertical_centered(|ui| {
                                            ui.add_space(fixed_height / 2.0 - 10.0);
                                            ui.label(
                                                RichText::new("Nessun partecipante aggiunto")
                                                    .italics()
                                                    .color(egui::Color32::GRAY)
                                            );
                                        });
                                    } else {
                                        let mut selected_list: Vec<String> = s.create_group_popup.selected_participants
                                            .iter()
                                            .cloned()
                                            .collect();
                                        selected_list.sort();

                                        ui.spacing_mut().item_spacing.y = 6.0;

                                        for username in selected_list {
                                            ui.horizontal(|ui| {
                                                ui.spacing_mut().item_spacing.x = 4.0;

                                                if ui.small_button(egui_remixicon::icons::CLOSE_LINE).clicked() {
                                                    s.create_group_popup.selected_participants.remove(&username);
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
                let can_create = !s.create_group_popup.group_name.trim().is_empty();

                ui.horizontal(|ui| {
                    let cancel_button = egui::Button::new(
                        RichText::new(format!("{} Annulla", egui_remixicon::icons::CLOSE_LINE))
                            .size(14.0)
                    )
                        .fill(ui.visuals().widgets.inactive.bg_fill)
                        .min_size(egui::vec2(130.0, 40.0));

                    if ui.add(cancel_button).clicked() {
                        s.create_group_popup.reset();
                        should_close = true;
                    }

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let create_button = egui::Button::new(
                            RichText::new(format!("{} Crea Gruppo", egui_remixicon::icons::CHECK_LINE))
                                .size(14.0)
                        )
                            .fill(if can_create {
                                egui::Color32::from_rgb(200, 100, 40)
                            } else {
                                ui.visuals().widgets.inactive.bg_fill
                            })
                            .min_size(egui::vec2(180.0, 40.0));

                        if ui.add_enabled(can_create, create_button).clicked() {
                            create_group_with_participants(s);
                            should_close = true;
                        }
                    });
                });
            });
        });

    if should_close {
        open = false;
    }

    s.show_create_group_modal = open;
}


fn create_group_with_participants(s: &mut AppState) {
    // CHECK CONNESSIONE: Blocca subito se non connesso
    if s.ws_status != crate::models::WsStatus::Connected {
        warn!("Cannot create group: WebSocket not connected");
        let _ = s.ui_tx.send(UiEvent::Error(crate::models::ErrorType::Connection));
        return;
    }

    let group_name = s.create_group_popup.group_name.trim().to_string();
    let participants: Vec<String> = s
        .create_group_popup
        .selected_participants
        .iter()
        .cloned()
        .collect();

    info!(
        "Creating group '{}' with {} participants via WebSocket: {:?}",
        group_name,
        participants.len(),
        participants
    );

    // Crea stub per il gruppo
    let stub_id = Uuid::new_v4();

    let stub_conversation = ConversationDto {
        id: stub_id,
        kind: "group".to_string(),
        title: group_name.clone(),
        owner_id: s.user_id.unwrap_or(Uuid::nil()),
        created_at: chrono::Utc::now().timestamp(),
        last_read_sequence: 0,
        last_activity: chrono::Utc::now().timestamp(),
        last_msg_seq: 0,
    };

    // Aggiungi stub alla lista conversazioni
    if let Some(ref mut convs) = s.conversations {
        convs.insert(0, stub_conversation);
    }

    // Traccia lo stub
    s.group_stubs.insert(stub_id, (group_name.clone(), std::time::Instant::now()));

    // Apri il gruppo stub
    s.cid = Some(stub_id);
    s.page = Page::Chat;
    s.conv_title = group_name.clone();

    // Messaggio di sistema nello stub
    let system_msg =
        MessageDto::system_message(format!("Creazione gruppo '{}' in corso...", group_name));
    s.conversation_messages
        .entry(stub_id)
        .or_insert_with(Vec::new)
        .push(system_msg.clone());
    s.messages = vec![system_msg];

    // Invia al server
    let outgoing = Outgoing::CreateGroupWithParticipants {
        group_name,
        participant_usernames: participants,
        client_temp_id: Some(stub_id.to_string()),
    };

    if let Err(e) = s.ui_to_net_tx.try_send(outgoing) {
        // Cleanup in caso di errore
        if let Some(ref mut convs) = s.conversations {
            convs.retain(|c| c.id != stub_id);
        }
        s.group_stubs.remove(&stub_id);
        s.conversation_messages.remove(&stub_id);
        s.messages.clear();
        s.cid = None;
        s.page = Page::Conversations;

        error!("Failed to send group creation message: {}", e);
        let _ = s.ui_tx.send(UiEvent::Error(
            crate::models::ErrorType::GroupCreate
        ));
        return;
    }

    info!("Created group stub {} and opened it", stub_id);
    s.create_group_popup.reset();
}

fn create_dm_subsection(ui: &mut egui::Ui, s: &mut AppState) {
    styled_frame(ui).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("💬")
                    .size(16.0)
                    .color(egui::Color32::from_rgb(120, 180, 255)),
            );
            ui.add_space(6.0);
            ui.label(
                RichText::new("Chat Privata")
                    .strong()
                    .color(egui::Color32::from_rgb(220, 160, 100)),
            );
        });

        ui.add_space(8.0);

        labeled_text(ui, "Username:", &mut s.dm_username, "nome_utente");

        ui.add_space(8.0);

        let can_dm = !s.dm_username.trim().is_empty();
        if action_button(ui, "➕ Crea Chat", can_dm) {
            let target_username = s.dm_username.trim().to_string();

            // Verifica se esiste già un DM con questo utente
            let existing_dm = s.conversations.as_ref().and_then(|convs| {
                convs
                    .iter()
                    .find(|c| c.kind == "dm" && c.title == target_username)
            });

            if let Some(existing_conv) = existing_dm {
                // DM già esistente, apri quella conversazione
                info!(
                    "DM with {} already exists (id: {}), opening it",
                    target_username, existing_conv.id
                );

                let _ = s.ui_tx.send(UiEvent::Info(format!(
                    "Chat con {} già esistente, apertura in corso...",
                    target_username
                )));

                let _ = s.ui_tx.send(UiEvent::Opened(existing_conv.id));
                s.dm_username.clear();
            } else {
                // CHECK CONNESSIONE: Blocca subito se non connesso
                if s.ws_status != crate::models::WsStatus::Connected {
                    warn!("Cannot create DM: WebSocket not connected");
                    let _ = s.ui_tx.send(UiEvent::Error(crate::models::ErrorType::Connection));
                    return;
                }

                // Crea uno stub locale per la DM
                let stub_conversation_id = Uuid::new_v4();

                info!(
                    "Creating DM stub for {} with temp ID: {}",
                    target_username, stub_conversation_id
                );

                let _ = s.ui_tx.send(UiEvent::DmStubCreated(
                    stub_conversation_id,
                    target_username.clone(),
                ));

                s.dm_username.clear();

                let _ = s.ui_tx.send(UiEvent::Info(format!(
                    "Chat con {} pronta! Scrivi il primo messaggio per iniziare",
                    target_username
                )));

                debug!("DM stub created successfully:");
                debug!("  Stub ID: {}", stub_conversation_id);
                debug!("  Target: {}", target_username);
                debug!("  Will use as client_temp_id when sending first message");
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
                match api::conversation::join_by_token(&base, &token2, &token_input_clone).await {
                    Ok(cid) => {
                        let _ = tx.send(UiEvent::Info(
                            "Unito al gruppo! Aggiornamento lista...".into(),
                        ));

                        tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;

                        match crate::api::conversation::get_conversations(&base, &token2).await {
                            Ok(conversations) => {
                                let _ = tx.send(UiEvent::ConversationsLoaded(conversations));
                                let _ = tx.send(UiEvent::Opened(cid));
                                let _ = tx.send(UiEvent::Info("Ti sei unito al gruppo!".into()));
                            }
                            Err(e) => {
                                error!("Failed to refresh conversations after join: {}", e);
                                let _ = tx.send(UiEvent::Error(
                                    crate::models::ErrorType::DataRecovery
                                ));
                            }
                        }
                    }
                    Err(e) => {
                        error!("Failed to join group by token: {}", e);
                        let _ = tx.send(UiEvent::Error(
                            crate::models::ErrorType::Invite
                        ));
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
                        let _ = s.ui_tx.send(UiEvent::Error(
                            crate::models::ErrorType::Generic("UUID conversazione non valido".to_string())
                        ));
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
                        error!("Failed to create invite: {}", e);
                        let _ = tx.send(UiEvent::Error(
                            crate::models::ErrorType::Invite
                        ));
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