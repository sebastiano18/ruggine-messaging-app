use crate::models::UiEvent;
use crate::state::AppState;
use eframe::egui;
use egui::{Align, Align2, Frame, Layout, RichText, TextEdit};
use chrono::{DateTime, Local};
use tracing::error;
use crate::ui::modals::conversation_popups;

pub struct ConversationsSidebar {
    search_query: String,
    dm_username: String,
    show_action_popup: bool,
    show_create_dm_popup: bool,
}

impl ConversationsSidebar {
    pub fn new() -> Self {
        Self {
            search_query: String::new(),
            dm_username: String::new(),
            show_action_popup: false,
            show_create_dm_popup: false,
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

        ui.add_space(8.0);
        // Header principale
        self.show_header(ui, state, &token);
        ui.add_space(6.0);

        // Sezione ricerca
        self.show_search_section(ui, state, &token);
        ui.add_space(8.0);

        // Tutti i popup
        conversation_popups::show_delete_confirmation_popup(ui.ctx(), state);
        conversation_popups::show_action_selection_popup(
            ui.ctx(),
            state,
            &mut self.show_action_popup,
            &mut self.show_create_dm_popup
        );
        conversation_popups::show_create_dm_popup(
            ui.ctx(),
            state,
            &mut self.show_create_dm_popup,
            &mut self.dm_username
        );
        conversation_popups::show_create_group_popup(ui.ctx(), state);

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

    fn show_header(&mut self, ui: &mut egui::Ui, state: &mut AppState, token: &str) {
        ui.horizontal(|ui| {
            ui.add_space(8.0);

            let text_color = ui.visuals().text_color();

            ui.label(
                RichText::new("Conversazioni".to_string())
                    .size(18.0)
                    .color(text_color)
                    .strong()
            );

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {

                let create_btn = egui::Button::new(
                    RichText::new(format!("{}", egui_remixicon::icons::CHAT_NEW_LINE)).size(16.0)
                )
                    .frame(false)
                    .fill(egui::Color32::TRANSPARENT);

                if ui.add(create_btn).on_hover_text("Nuova conversazione").clicked() {
                    self.show_action_popup = true;
                }

                ui.add_space(4.0);
            });
        });
    }

    fn show_search_section(&mut self, ui: &mut egui::Ui, _state: &mut AppState, _token: &str) {
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
                    ui.label(
                        RichText::new(egui_remixicon::icons::SEARCH_LINE)
                            .size(14.0)
                            .color(ui.visuals().weak_text_color())
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
                        self.show_empty_state(ui, state);
                    }
                    Some(conversations) if conversations.is_empty() => {
                        self.show_empty_state(ui, state);
                    }
                    Some(conversations) => {
                        let filtered: Vec<_> = self
                            .filter_conversations(conversations)
                            .into_iter()
                            .cloned()
                            .collect();

                        if filtered.is_empty() && !self.search_query.is_empty() {
                            self.show_no_search_results(ui);
                        } else {
                            for conv in &filtered {
                                self.render_conversation_item(ui, state, conv);
                                ui.add_space(3.0);
                            }
                        }
                    }
                }
            });
    }

    fn render_conversation_item(
        &self,
        ui: &mut egui::Ui,
        state: &mut AppState,
        conv: &crate::models::ConversationDto,
    ) {
        let is_selected = state.cid.map_or(false, |cid| cid == conv.id);
        let response = ui.allocate_response(egui::vec2(ui.available_width(), 56.0), egui::Sense::click());
        let pointer_over_row = ui.rect_contains_pointer(response.rect);

        let orange = egui::Color32::from_rgb(200, 100, 40);
        let orange_hover = egui::Color32::from_rgb(240, 140, 80);

        let (bg_color, text_color, preview_color) = if is_selected {
            (orange, egui::Color32::WHITE, egui::Color32::WHITE)
        } else if pointer_over_row {
            let hover_bg = if ui.visuals().dark_mode {
                egui::Color32::from_gray(40)
            } else {
                egui::Color32::from_gray(240)
            };
            (hover_bg, orange_hover, ui.visuals().text_color())
        } else {
            (egui::Color32::TRANSPARENT, ui.visuals().text_color(), ui.visuals().weak_text_color())
        };

        if bg_color != egui::Color32::TRANSPARENT {
            ui.painter().rect_filled(response.rect, egui::Rounding::same(8.0), bg_color);
        }

        let is_owner = state.user_id.map_or(false, |uid| uid == conv.owner_id);
        let is_participant = !is_owner && conv.kind == "group";

        ui.allocate_ui_at_rect(response.rect.shrink(12.0), |ui| {
            ui.horizontal(|ui| {
                let (icon, icon_color) = match conv.kind.as_str() {
                    "group" => (egui_remixicon::icons::TEAM_FILL, if is_selected { egui::Color32::WHITE } else if pointer_over_row { orange_hover } else { ui.visuals().weak_text_color() }),
                    _ => (egui_remixicon::icons::CHAT_1_FILL, if is_selected { egui::Color32::WHITE } else if pointer_over_row { orange_hover } else { ui.visuals().weak_text_color() }),
                };

                ui.add(egui::Label::new(RichText::new(icon).size(16.0).color(icon_color)).selectable(false));
                ui.add_space(10.0);

                ui.vertical(|ui| {
                    ui.add(egui::Label::new(RichText::new(&conv.title).size(14.0).color(text_color)).selectable(false));
                    ui.add_space(2.0);
                    self.show_message_preview(ui, state, conv, preview_color);
                });

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if let Some(&unread_count) = state.conversation_unread_counts.get(&conv.id) {
                        if unread_count > 0 {
                            let badge_text = if unread_count > 99 { "99+".to_string() } else { unread_count.to_string() };
                            let (rect, _) = ui.allocate_exact_size(egui::vec2(22.0, 22.0), egui::Sense::hover());
                            ui.painter().circle_filled(rect.center(), 11.0, orange);
                            ui.painter().text(rect.center(), Align2::CENTER_CENTER, &badge_text, egui::FontId::proportional(10.0), egui::Color32::WHITE);
                            ui.add_space(6.0);
                        }
                    }

                    if let Some(date_text) = self.get_last_message_date(state, conv) {
                        ui.add(egui::Label::new(RichText::new(date_text).size(10.0).color(preview_color)).selectable(false));
                        ui.add_space(4.0);
                    }
                });
            });
        });

        // Menu contestuale al click destro sulla conversazione
        if ((conv.kind == "group" && (is_owner || is_participant)) || conv.kind != "group") {
            response.context_menu(|ui| {
                let label = if conv.kind == "group" {
                    if is_owner {
                        format!("{} Elimina gruppo", egui_remixicon::icons::DELETE_BIN_LINE)
                    } else {
                        format!("{} Esci dal gruppo", egui_remixicon::icons::LOGOUT_BOX_LINE)
                    }
                } else {
                    format!("{} Elimina conversazione", egui_remixicon::icons::DELETE_BIN_LINE)
                };

                if ui.button(RichText::new(label).size(14.0)).clicked() {
                    state.request_delete_confirmation(conv);
                    ui.close_menu();
                }
            });
        }

        if response.clicked() {
            if let Some(current_cid) = state.cid {
                if current_cid != conv.id {
                    let _ = state.ui_tx.send(UiEvent::Closed(current_cid));
                }
            }
            let _ = state.ui_tx.send(UiEvent::Opened(conv.id));
        }
    }

    fn show_message_preview(&self, ui: &mut egui::Ui, state: &AppState, conv: &crate::models::ConversationDto, color: egui::Color32) {
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

        ui.add(egui::Label::new(RichText::new(preview_text).size(12.0).color(color)).selectable(false));
    }

    fn get_last_message_date(&self, state: &AppState, conv: &crate::models::ConversationDto) -> Option<String> {
        let timestamp = state
            .conversation_messages
            .get(&conv.id)
            .and_then(|messages| messages.last())
            .map(|msg| msg.created_at)
            .or_else(|| Some(conv.last_activity))?;

        Some(format_date_label(timestamp))
    }

    fn filter_conversations<'a>(&self, conversations: &'a [crate::models::ConversationDto]) -> Vec<&'a crate::models::ConversationDto> {
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

    fn show_empty_state(&mut self, ui: &mut egui::Ui, _state: &mut AppState) {
        ui.vertical_centered(|ui| {
            ui.add_space(60.0);
            ui.label(
                RichText::new(egui_remixicon::icons::INBOX_LINE)
                    .size(48.0)
                    .color(ui.visuals().weak_text_color())
            );
            ui.add_space(12.0);
            ui.label(RichText::new("Nessuna conversazione").size(16.0).color(ui.visuals().text_color()));
            ui.add_space(8.0);
            ui.label(
                RichText::new("Le tue conversazioni appariranno qui")
                    .size(12.0)
                    .color(ui.visuals().weak_text_color())
            );
            ui.add_space(12.0);
            ui.label(RichText::new("oppure").size(11.0).color(ui.visuals().weak_text_color()));
            ui.add_space(12.0);

            let btn = egui::Button::new(RichText::new("Crea nuova conversazione").color(egui::Color32::WHITE))
                .fill(egui::Color32::from_rgb(200, 100, 40))
                .rounding(egui::Rounding::same(6.0));

            if ui.add(btn).clicked() {
                self.show_action_popup = true;
            }
        });
    }

    fn show_no_search_results(&self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(50.0);
            ui.label(
                RichText::new(egui_remixicon::icons::SEARCH_LINE)
                    .size(40.0)
                    .color(ui.visuals().weak_text_color())
            );
            ui.add_space(12.0);
            ui.label(RichText::new("Nessun risultato").size(15.0).color(ui.visuals().text_color()));
            ui.add_space(6.0);
            ui.label(
                RichText::new(format!("Nessuna conversazione trovata per '{}'", self.search_query))
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
                    error!("Failed to load conversations: {}", e);
                    let _ = tx.send(UiEvent::Error(crate::models::ErrorType::DataRecovery));
                    let _ = tx.send(UiEvent::ConversationsLoaded(vec![]));
                }
            }
        });
    }
}

fn format_date_label(timestamp: i64) -> String {
    let dt = match DateTime::from_timestamp(timestamp, 0) {
        Some(dt) => dt.with_timezone(&Local),
        None => return "".to_string(),
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