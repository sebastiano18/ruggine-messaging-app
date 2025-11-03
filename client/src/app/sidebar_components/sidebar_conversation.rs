use crate::api;
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
            ui.label(RichText::new("💬 Conversazioni").heading().strong());

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .small_button("🔄")
                    .on_hover_text("Carica conversazioni")
                    .clicked()
                {
                    self.refresh_conversations(state, token);
                }

                ui.add_space(4.0);

                if ui
                    .small_button("➕")
                    .on_hover_text("Gestione gruppi")
                    .clicked()
                {
                    state.page = Page::GroupManagement;
                }
            });
        });
    }

    fn show_search_section(&mut self, ui: &mut egui::Ui, state: &mut AppState, token: &str) {
        Frame::group(ui.style())
            .fill(egui::Color32::from_rgb(255, 140, 60).linear_multiply(0.06))
            .stroke(Stroke::new(
                0.5,
                egui::Color32::from_rgb(240, 140, 80).linear_multiply(0.3),
            ))
            .inner_margin(egui::Margin::symmetric(10.0, 8.0))
            .rounding(egui::Rounding::same(8.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("🔍")
                            .size(16.0)
                            .color(egui::Color32::from_rgb(200, 100, 40)),
                    );
                    ui.add_space(6.0);

                    TextEdit::singleline(&mut self.search_query)
                        .hint_text("Cerca nelle conversazioni...")
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

        // Determina se il puntatore è dentro l'intera riga, indipendentemente da widget sovrapposti
        let pointer_over_row = ui.rect_contains_pointer(response.rect);
        // Usato per evitare l'apertura della chat quando si clicca sulla 'X'
        let mut delete_clicked = false;

        let (bg_color, text_color, preview_color) = if is_selected {
            (
                egui::Color32::from_rgb(200, 100, 40),
                egui::Color32::WHITE,
                egui::Color32::from_rgb(255, 220, 180),
            )
        } else if response.hovered() {
            (
                egui::Color32::from_rgb(240, 140, 80),
                egui::Color32::WHITE,
                egui::Color32::from_rgb(255, 200, 150),
            )
        } else {
            (
                egui::Color32::TRANSPARENT,
                egui::Color32::from_rgb(220, 160, 100),
                egui::Color32::from_rgb(180, 120, 70),
            )
        };

        if bg_color != egui::Color32::TRANSPARENT {
            ui.painter()
                .rect_filled(response.rect, egui::Rounding::same(6.0), bg_color);
        }

        // Mostra il bottone elimina SOLO se l'utente corrente è l'owner (for groups-only)
        let show_delete_button = state.user_id.map_or(false, |uid| uid == conv.owner_id);

        ui.allocate_ui_at_rect(response.rect.shrink(10.0), |ui| {
            ui.horizontal(|ui| {
                let (icon, icon_color) = match conv.kind.as_str() {
                    "group" => ("👥", egui::Color32::from_rgb(255, 140, 60)),
                    "dm" => ("💬", egui::Color32::from_rgb(255, 180, 100)),
                    _ => ("📄", egui::Color32::from_rgb(200, 120, 80)),
                };

                ui.label(RichText::new(icon).size(18.0).color(icon_color));
                ui.add_space(8.0);

                ui.vertical(|ui| {
                    ui.label(
                        RichText::new(&conv.title)
                            .strong()
                            .size(14.0)
                            .color(text_color),
                    );

                    ui.add_space(2.0);
                    self.show_message_preview(ui, state, conv, preview_color);
                });

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // ⭐ NUOVO: Badge con contatore unread (a destra, prima della X)
                    if let Some(&unread_count) = state.conversation_unread_counts.get(&conv.id) {
                        if unread_count > 0 {
                            let badge_text = if unread_count > 99 {
                                "99+".to_string()
                            } else {
                                unread_count.to_string()
                            };

                            let badge_size = egui::vec2(24.0, 24.0);
                            let (rect, _) = ui.allocate_exact_size(badge_size, egui::Sense::hover());

                            // Cerchio arancione
                            ui.painter().circle_filled(
                                rect.center(),
                                12.0,
                                egui::Color32::from_rgb(255, 100, 30)
                            );

                            // Testo bianco centrato
                            ui.painter().text(
                                rect.center(),
                                egui::Align2::CENTER_CENTER,
                                &badge_text,
                                egui::FontId::proportional(11.0),
                                egui::Color32::WHITE,
                            );

                            ui.add_space(8.0);
                        }
                    }

                    if conv.kind == "group" {
                        // Mostra la 'X' solo quando il mouse è sopra la riga e solo se l'utente è owner
                        if show_delete_button && pointer_over_row {
                            let delete_button = egui::Button::new(RichText::new("X").size(16.0))
                                .small()
                                .frame(false)
                                .fill(egui::Color32::TRANSPARENT);

                            let del_resp = ui.add(delete_button).on_hover_text("Elimina gruppo");

                            if del_resp.clicked() {
                                delete_clicked = true;
                                state.request_delete_confirmation(conv);
                            }

                            ui.add_space(8.0);
                        }
                    } else {
                        // Per le chat private mostra la 'X' solo quando il mouse è sopra la riga
                        if pointer_over_row {
                            let delete_button = egui::Button::new(RichText::new("X").size(16.0))
                                .small()
                                .frame(false)
                                .fill(egui::Color32::TRANSPARENT);

                            let del_resp = ui
                                .add(delete_button)
                                .on_hover_text("Elimina conversazione");

                            if del_resp.clicked() {
                                delete_clicked = true;
                                state.request_delete_confirmation(conv);
                            }

                            ui.add_space(8.0);
                        }
                    }
                });
            });
        });

        // Click sull'elemento per aprire la conversazione (solo se non si è cliccato elimina)
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
        let kind_label = match conversation.kind.as_str() {
            "group" => "gruppo",
            "dm" => "chat privata",
            _ => "conversazione",
        };

        let detail_message = if is_stub {
            "Si tratta di uno stub locale: verrà semplicemente rimosso dalla tua lista."
        } else if conversation.kind == "group" {
            "L'eliminazione rimuoverà il gruppo per tutti i partecipanti. L'azione è irreversibile."
        } else {
            "L'eliminazione rimuoverà definitivamente la conversazione. L'azione è irreversibile."
        };

        egui::Window::new("Conferma eliminazione")
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
            .open(&mut open)
            .show(ui.ctx(), |ui| {
                ui.set_width(320.0);
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new(format!(
                            "Sei sicuro di voler eliminare la {} \"{}\"?",
                            kind_label, conversation.title
                        ))
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
                            RichText::new("Elimina").color(egui::Color32::WHITE),
                        )
                        .fill(egui::Color32::from_rgb(200, 100, 40));

                        ui.add_space(100.0);

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
            ui.add_space(50.0);
            ui.label(RichText::new("📭").size(32.0));
            ui.add_space(8.0);
            ui.label(RichText::new("Nessuna conversazione").color(egui::Color32::GRAY));
            ui.add_space(12.0);

            ui.label(
                RichText::new("Clicca 🔄 per caricare le conversazioni")
                    .size(12.0)
                    .color(egui::Color32::GRAY),
            );

            ui.add_space(8.0);

            ui.label(
                RichText::new("oppure")
                    .size(11.0)
                    .color(egui::Color32::GRAY),
            );

            ui.add_space(8.0);

            if ui.button("Crea una nuova conversazione").clicked() {
                state.page = Page::GroupManagement;
            }
        });
    }

    fn show_no_search_results(&self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(40.0);
            ui.label(
                RichText::new("🔍")
                    .size(28.0)
                    .color(egui::Color32::from_rgb(160, 100, 60)),
            );
            ui.add_space(8.0);
            ui.label(
                RichText::new("Nessun risultato").color(egui::Color32::from_rgb(160, 100, 60)),
            );
            ui.add_space(4.0);
            ui.label(
                RichText::new(format!(
                    "Nessuna conversazione trovata per '{}'",
                    self.search_query
                ))
                .size(12.0)
                .color(egui::Color32::GRAY),
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
