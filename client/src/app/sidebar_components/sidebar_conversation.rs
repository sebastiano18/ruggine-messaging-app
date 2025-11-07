use crate::models::{Page, UiEvent};
use crate::state::AppState;
use eframe::egui;
use egui::{Align, Align2, Frame, Layout, RichText, Stroke, TextEdit};

pub struct ConversationsSidebar {
    search_query: String,
    dm_username: String,
}

impl ConversationsSidebar {
    pub fn new() -> Self {
        Self {
            search_query: String::new(),
            dm_username: String::new(),
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        if state.token.is_none() {
            ui.colored_label(
                egui::Color32::RED,
                "Login richiesto per visualizzare le conversazioni",
            );
            return;
        }

        // Auto-refresh se richiesto esplicitamente
        self.auto_refresh_if_needed(state);

        let token = state.token.clone().unwrap();

        // Header principale
        self.show_header(ui, state, &token);
        ui.separator();
        ui.add_space(6.0);

        // Sezione ricerca
        self.show_search_section(ui, state, &token);
        ui.add_space(8.0);

        self.show_delete_confirmation_popup(ui, state);

        // Lista conversazioni
        self.show_filtered_conversations(ui, state);
    }

    fn auto_refresh_if_needed(&self, state: &mut AppState) {
        if state.request_conversations_refresh {
            let token = match &state.token {
                Some(t) => t.clone(),
                None => return,
            };

            self.refresh_conversations(state, &token);
            state.request_conversations_refresh = false;
        }
    }

    fn show_header(&self, ui: &mut egui::Ui, state: &mut AppState, token: &str) {
        ui.horizontal(|ui| {
            ui.add_space(8.0);

            // Usa il colore del testo adattivo al tema
            let text_color = ui.visuals().text_color();

            ui.label(
                RichText::new(format!("{} Conversazioni", egui_remixicon::icons::CHAT_4_FILL))
                    .size(18.0)
                    .color(text_color)
                    .strong()
            );

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                // Bottone refresh con stile minimale
                let refresh_btn = egui::Button::new(
                    RichText::new("🔄").size(14.0)
                )
                    .frame(false)
                    .fill(egui::Color32::TRANSPARENT);

                if ui.add(refresh_btn)
                    .on_hover_text("Aggiorna conversazioni")
                    .clicked()
                {
                    self.refresh_conversations(state, token);
                }

                ui.add_space(4.0);

                // Bottone crea gruppo con stile minimale
                let create_btn = egui::Button::new(
                    RichText::new("➕").size(14.0)
                )
                    .frame(false)
                    .fill(egui::Color32::TRANSPARENT);

                if ui.add(create_btn)
                    .on_hover_text("Gestione gruppi")
                    .clicked()
                {
                    state.page = Page::GroupManagement;
                }

                ui.add_space(4.0);
            });
        });
    }

    fn show_search_section(&mut self, ui: &mut egui::Ui, state: &mut AppState, token: &str) {
        // Frame minimale che si adatta al tema
        let bg_color = if ui.visuals().dark_mode {
            ui.visuals().extreme_bg_color
        } else {
            egui::Color32::from_gray(245)
        };

        Frame::none()
            .fill(bg_color)
            .inner_margin(egui::Margin::symmetric(12.0, 10.0))
            .rounding(egui::Rounding::same(8.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    // Icona ricerca con colore adattivo
                    let icon_color = ui.visuals().weak_text_color();
                    ui.label(
                        RichText::new("🔍")
                            .size(14.0)
                            .color(icon_color),
                    );
                    ui.add_space(8.0);

                    TextEdit::singleline(&mut self.search_query)
                        .hint_text("Cerca conversazioni...")
                        .desired_width(ui.available_width())
                        .show(ui);
                });
            });
    }

    fn show_filtered_conversations(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                match &state.conversations {
                    None => {
                        // Mostra lo stato vuoto quando non ci sono conversazioni caricate
                        self.show_empty_state(ui, state);
                    }
                    Some(conversations) if conversations.is_empty() => {
                        // Mostra lo stato vuoto quando l'array è vuoto
                        self.show_empty_state(ui, state);
                    }
                    Some(conversations) => {
                        let filtered: Vec<crate::models::ConversationDto> = self
                            .filter_conversations(conversations)
                            .into_iter()
                            .cloned()
                            .collect();

                        if filtered.is_empty() && !self.search_query.is_empty() {
                            self.show_no_search_results(ui);
                        } else {
                            let filtered_refs: Vec<&crate::models::ConversationDto> =
                                filtered.iter().collect();
                            self.show_conversation_list(ui, state, &filtered_refs);
                        }
                    }
                }
            });
    }

    fn filter_conversations<'a>(
        &self,
        conversations: &'a [crate::models::ConversationDto],
    ) -> Vec<&'a crate::models::ConversationDto> {
        if self.search_query.is_empty() {
            return conversations.iter().collect();
        }

        let query = self.search_query.to_lowercase();
        conversations
            .iter()
            .filter(|conv| {
                conv.title.to_lowercase().contains(&query)
                    || match conv.kind.as_str() {
                    "group" => "gruppo".contains(&query),
                    "dm" => "privata".contains(&query) || "dm".contains(&query),
                    _ => false,
                }
            })
            .collect()
    }

    fn show_conversation_list(
        &self,
        ui: &mut egui::Ui,
        state: &mut AppState,
        conversations: &[&crate::models::ConversationDto],
    ) {
        for &conv in conversations {
            self.render_conversation_item(ui, state, conv);
            ui.add_space(3.0);
        }
    }

    fn render_conversation_item(
        &self,
        ui: &mut egui::Ui,
        state: &mut AppState,
        conv: &crate::models::ConversationDto,
    ) {
        let is_selected = state.cid.map_or(false, |cid| cid == conv.id);

        let response =
            ui.allocate_response(egui::vec2(ui.available_width(), 56.0), egui::Sense::click());

        // Determina se il puntatore è dentro l'intera riga
        let pointer_over_row = ui.rect_contains_pointer(response.rect);
        let mut delete_clicked = false;

        // Colori adattivi per light/dark mode con arancione solo per interazioni
        let orange = egui::Color32::from_rgb(200, 100, 40);
        let orange_hover = egui::Color32::from_rgb(240, 140, 80);

        let (bg_color, text_color, preview_color) = if is_selected {
            // Selezionato: arancione
            (
                orange,
                egui::Color32::WHITE,
                egui::Color32::WHITE,
            )
        } else if pointer_over_row {
            // Hover: sfondo sottile adattivo con testo arancione
            let hover_bg = if ui.visuals().dark_mode {
                egui::Color32::from_gray(40)
            } else {
                egui::Color32::from_gray(240)
            };
            (
                hover_bg,
                orange_hover,
                ui.visuals().text_color(),
            )
        } else {
            // Normale: trasparente con colori del tema
            (
                egui::Color32::TRANSPARENT,
                ui.visuals().text_color(),
                ui.visuals().weak_text_color(),
            )
        };

        // Disegna sfondo con bordi arrotondati
        if bg_color != egui::Color32::TRANSPARENT {
            ui.painter()
                .rect_filled(response.rect, egui::Rounding::same(8.0), bg_color);
        }

        let is_owner = state.user_id.map_or(false, |uid| uid == conv.owner_id);
        let is_participant = !is_owner && conv.kind == "group";

        ui.allocate_ui_at_rect(response.rect.shrink(12.0), |ui| {
            ui.horizontal(|ui| {
                // Icona con colore adattivo
                let (icon, icon_color) = match conv.kind.as_str() {
                    "group" => {
                        let color = if is_selected {
                            egui::Color32::WHITE
                        } else if pointer_over_row {
                            orange_hover
                        } else {
                            ui.visuals().weak_text_color()
                        };
                        (egui_remixicon::icons::TEAM_FILL, color)
                    },
                    "dm" => {
                        let color = if is_selected {
                            egui::Color32::WHITE
                        } else if pointer_over_row {
                            orange_hover
                        } else {
                            ui.visuals().weak_text_color()
                        };
                        (egui_remixicon::icons::CHAT_1_FILL, color)
                    },
                    _ => {
                        let color = if is_selected {
                            egui::Color32::WHITE
                        } else if pointer_over_row {
                            orange_hover
                        } else {
                            ui.visuals().weak_text_color()
                        };
                        (egui_remixicon::icons::FILE_TEXT_FILL, color)
                    },
                };

                ui.label(RichText::new(icon).size(16.0).color(icon_color));
                ui.add_space(10.0);

                ui.vertical(|ui| {
                    ui.label(
                        RichText::new(&conv.title)
                            .size(14.0)
                            .color(text_color),
                    );

                    ui.add_space(2.0);
                    self.show_message_preview(ui, state, conv, preview_color);
                });

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // Mostra X solo su hover
                    if conv.kind == "group" {
                        if (is_owner || is_participant) && pointer_over_row {
                            let (button_text, hover_text) = if is_owner {
                                ("×", "Elimina gruppo")
                            } else {
                                ("×", "Esci dal gruppo")
                            };

                            let delete_color = if ui.visuals().dark_mode {
                                egui::Color32::from_gray(180)
                            } else {
                                egui::Color32::from_gray(100)
                            };

                            let delete_button = egui::Button::new(
                                RichText::new(button_text)
                                    .size(20.0)
                                    .color(delete_color)
                            )
                                .frame(false)
                                .fill(egui::Color32::TRANSPARENT);

                            let del_resp = ui.add(delete_button).on_hover_text(hover_text);

                            if del_resp.hovered() {
                                ui.painter().text(
                                    del_resp.rect.center(),
                                    egui::Align2::CENTER_CENTER,
                                    "×",
                                    egui::FontId::proportional(20.0),
                                    egui::Color32::from_rgb(220, 80, 60),
                                );
                            }

                            if del_resp.clicked() {
                                delete_clicked = true;
                                state.request_delete_confirmation(conv);
                            }

                            ui.add_space(6.0);
                        }
                    } else {
                        if pointer_over_row {
                            let delete_color = if ui.visuals().dark_mode {
                                egui::Color32::from_gray(180)
                            } else {
                                egui::Color32::from_gray(100)
                            };

                            let delete_button = egui::Button::new(
                                RichText::new("×")
                                    .size(20.0)
                                    .color(delete_color)
                            )
                                .frame(false)
                                .fill(egui::Color32::TRANSPARENT);

                            let del_resp = ui
                                .add(delete_button)
                                .on_hover_text("Elimina conversazione");

                            if del_resp.hovered() {
                                ui.painter().text(
                                    del_resp.rect.center(),
                                    egui::Align2::CENTER_CENTER,
                                    "×",
                                    egui::FontId::proportional(20.0),
                                    egui::Color32::from_rgb(220, 80, 60),
                                );
                            }

                            if del_resp.clicked() {
                                delete_clicked = true;
                                state.request_delete_confirmation(conv);
                            }

                            ui.add_space(6.0);
                        }
                    }

                    // Badge unread con arancione
                    if let Some(&unread_count) = state.conversation_unread_counts.get(&conv.id) {
                        if unread_count > 0 {
                            let badge_text = if unread_count > 99 {
                                "99+".to_string()
                            } else {
                                unread_count.to_string()
                            };

                            let badge_size = egui::vec2(22.0, 22.0);
                            let (rect, _) = ui.allocate_exact_size(badge_size, egui::Sense::hover());

                            // Cerchio arancione minimalista
                            ui.painter().circle_filled(
                                rect.center(),
                                11.0,
                                orange
                            );

                            // Testo bianco
                            ui.painter().text(
                                rect.center(),
                                egui::Align2::CENTER_CENTER,
                                &badge_text,
                                egui::FontId::proportional(10.0),
                                egui::Color32::WHITE,
                            );

                            ui.add_space(6.0);
                        }
                    }
                });
            });
        });

        // Click per aprire
        if response.clicked() && !delete_clicked {
            let _ = state.ui_tx.send(UiEvent::Opened(conv.id));
            state.page = Page::Chat;
        }
    }

    // Gestione pop-up di eliminazione dm/group
    fn show_delete_confirmation_popup(&self, ui: &mut egui::Ui, state: &mut AppState) {
        let Some(pending) = state.pending_deletion.clone() else {
            return;
        };

        let conversation = pending.conversation;
        let mut open = true;
        let mut confirm = false;
        let mut cancel = false;

        let is_stub = state.is_dm_stub(conversation.id);

        // Determina se l'utente è l'owner del gruppo
        let is_owner = state.user_id.map_or(false, |uid| uid == conversation.owner_id);

        let (title, main_message, detail_message, confirm_label) = if is_stub {
            (
                "Conferma eliminazione",
                format!("Sei sicuro di voler eliminare la chat privata \"{}\"?", conversation.title),
                "Si tratta di uno stub locale: verrà semplicemente rimosso dalla tua lista.",
                "Elimina"
            )
        } else if conversation.kind == "group" {
            if is_owner {
                (
                    "Conferma eliminazione gruppo",
                    format!("Sei sicuro di voler eliminare il gruppo \"{}\"?", conversation.title),
                    "Attenzione: eliminando il gruppo, questo verrà rimosso per TUTTI i partecipanti. L'azione è irreversibile.",
                    "Elimina per tutti"
                )
            } else {
                (
                    "Conferma uscita dal gruppo",
                    format!("Sei sicuro di voler uscire dal gruppo \"{}\"?", conversation.title),
                    "Uscirai dal gruppo e non potrai più vedere i messaggi. Potrai rientrare solo se verrai invitato nuovamente.",
                    "Esci dal gruppo"
                )
            }
        } else {
            (
                "Conferma eliminazione",
                format!("Sei sicuro di voler eliminare la chat privata \"{}\"?", conversation.title),
                "L'eliminazione rimuoverà definitivamente la conversazione. L'azione è irreversibile.",
                "Elimina"
            )
        };

        egui::Window::new(title)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
            .open(&mut open)
            .show(ui.ctx(), |ui| {
                ui.set_width(340.0);
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new(main_message)
                            .strong(),
                    );

                    ui.add_space(6.0);
                    ui.label(
                        RichText::new(detail_message)
                            .size(12.0)
                            .color(egui::Color32::from_rgb(210, 200, 200)),
                    );

                    ui.add_space(12.0);

                    ui.horizontal(|ui| {
                        ui.add_space(50.0);
                        if ui.button("Annulla").clicked() {
                            cancel = true;
                        }

                        let confirm_button = egui::Button::new(
                            RichText::new(confirm_label).color(egui::Color32::WHITE),
                        )
                            .fill(egui::Color32::from_rgb(200, 100, 40));

                        ui.add_space(80.0);

                        if ui.add(confirm_button).clicked() {
                            confirm = true;
                        }
                    });
                });
            });

        if confirm {
            state.execute_pending_deletion();
        } else if cancel || !open {
            state.cancel_delete_confirmation();
        }
    }

    fn show_message_preview(
        &self,
        ui: &mut egui::Ui,
        state: &AppState,
        conv: &crate::models::ConversationDto,
        color: egui::Color32,
    ) {
        let preview_text = match state.conversation_messages.get(&conv.id) {
            Some(messages) => {
                if let Some(last_msg) = messages.last() {
                    if last_msg.content.len() > 35 {
                        format!("{}...", &last_msg.content[..35])
                    } else {
                        last_msg.content.clone()
                    }
                } else {
                    "Nessun messaggio".to_string()
                }
            }
            None => "Clicca per aprire".to_string(),
        };

        ui.label(RichText::new(preview_text).size(12.0).color(color));
    }

    fn show_empty_state(&self, ui: &mut egui::Ui, state: &mut AppState) {
        ui.vertical_centered(|ui| {
            ui.add_space(60.0);

            let icon_color = ui.visuals().weak_text_color();
            ui.label(RichText::new("📭").size(48.0));

            ui.add_space(12.0);

            ui.label(
                RichText::new("Nessuna conversazione")
                    .size(16.0)
                    .color(ui.visuals().text_color())
            );

            ui.add_space(8.0);

            ui.label(
                RichText::new("Clicca 🔄 per caricare le conversazioni")
                    .size(12.0)
                    .color(ui.visuals().weak_text_color()),
            );

            ui.add_space(12.0);

            ui.label(
                RichText::new("oppure")
                    .size(11.0)
                    .color(ui.visuals().weak_text_color()),
            );

            ui.add_space(12.0);

            // Bottone con stile arancione minimalista
            let btn = egui::Button::new(
                RichText::new("Crea nuova conversazione")
                    .color(egui::Color32::WHITE)
            )
                .fill(egui::Color32::from_rgb(200, 100, 40))
                .rounding(egui::Rounding::same(6.0));

            if ui.add(btn).clicked() {
                state.page = Page::GroupManagement;
            }
        });
    }

    fn show_no_search_results(&self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(50.0);

            let icon_color = ui.visuals().weak_text_color();
            ui.label(
                RichText::new("🔍")
                    .size(40.0)
                    .color(icon_color),
            );

            ui.add_space(12.0);

            ui.label(
                RichText::new("Nessun risultato")
                    .size(15.0)
                    .color(ui.visuals().text_color())
            );

            ui.add_space(6.0);

            ui.label(
                RichText::new(format!(
                    "Nessuna conversazione trovata per '{}'",
                    self.search_query
                ))
                    .size(12.0)
                    .color(ui.visuals().weak_text_color()),
            );
        });
    }

    fn refresh_conversations(&self, state: &AppState, token: &str) {
        let base = state.base.clone();
        let token2 = token.to_string();
        let tx = state.ui_tx.clone();

        state.rt.spawn(async move {
            match crate::api::conversation::get_conversations(&base, &token2).await {
                Ok(conversations) => {
                    let _ = tx.send(UiEvent::ConversationsLoaded(conversations));
                }
                Err(e) => {
                    let _ = tx.send(UiEvent::Error(format!(
                        "Errore nel caricamento: {e}"
                    )));
                    // Invia array vuoto per mostrare lo stato vuoto
                    let _ = tx.send(UiEvent::ConversationsLoaded(vec![]));
                }
            }
        });
    }
}
