use eframe::egui;
use egui::{Align, Layout};
use crate::state::{AppState, Page, UiEvent, WsStatus};
use crate::models::MessageDto;
use crate::ui;

pub struct App {
    state: AppState
}

impl App {
    pub fn new() -> Self {
        Self {
            state: AppState::new()
        }
    }

    fn ensure_ws_lifecycle(&mut self) {
        // Se l'utente non è autenticato, nessun WS
        if self.state.token.is_none() {
            self.state.ws_status = WsStatus::Disconnected;
            return;
        }

        // Se l'utente ha richiesto riconnessione manuale, forziamo
        if self.state.request_ws_reconnect && self.state.ws_status != WsStatus::Connecting {
            self.state.request_ws_reconnect = false;
            self.state.ws_status = WsStatus::Disconnected; // forza ramo sotto
        }

        match self.state.ws_status {
            WsStatus::Disconnected => {
                // avvia connessione WS globale
                let base = self.state.base.clone();
                let token = self.state.token.clone().unwrap();
                let tx = self.state.ui_tx.clone();

                self.state.ws_status = WsStatus::Connecting;

                self.state.rt.spawn(async move {
                    match crate::net::ws::connect(&base, &token).await {
                        Ok(mut ws) => {
                            if let Err(e) = crate::net::ws::subscribe(&mut ws).await {
                                let _ = tx.send(UiEvent::WsError(format!("WS subscribe fallito: {e}")));
                                return;
                            }
                            let _ = tx.send(UiEvent::WsConnected);
                            let tx_reader = tx.clone();

                            let ctrl = crate::net::ws::spawn_reader_and_pinger(ws, move |msg| {
                                // Parse the incoming WebSocket message as MessageDto
                                match serde_json::from_str::<MessageDto>(&msg) {
                                    Ok(message_dto) => {
                                        let _ = tx_reader.send(UiEvent::WsIncoming(message_dto));
                                    }
                                    Err(e) => {
                                        // If parsing fails, create a system message with the raw content
                                        let system_message = MessageDto {
                                            id: uuid::Uuid::new_v4(),
                                            author_id: uuid::Uuid::nil(),
                                            author_username: "system".to_string(),
                                            content: format!("Raw WS message: {}", msg),
                                            created_at: chrono::Utc::now().timestamp(),
                                        };
                                        let _ = tx_reader.send(UiEvent::WsIncoming(system_message));

                                        // Also send error notification
                                        let _ = tx_reader.send(UiEvent::Error(
                                            format!("Failed to parse WS message: {}", e)
                                        ));
                                    }
                                }
                            });

                            let _ = tx.send(UiEvent::WsControlReady(ctrl));
                        }
                        Err(e) => {
                            let _ = tx.send(UiEvent::WsError(format!("WS connect fallito: {e}")));
                        }
                    }
                });
            }
            WsStatus::Connecting | WsStatus::Connected => {
                // nulla da fare
            }
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 1) drena eventi (REST/WS) che aggiornano lo stato
        self.state.drain_events();

        // 2) garantisce il ciclo di vita del WS (connetti se serve)
        self.ensure_ws_lifecycle();

        // 3) UI - Header sempre presente
        egui::TopBottomPanel::top("header")
            .min_height(50.0)
            .show(ctx, |ui| {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    // Titolo app con icona
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("🦀").size(24.0));
                        ui.label(egui::RichText::new("Ruggine Chat").size(20.0).strong());
                    });

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        // Status indicators
                        match self.state.ws_status {
                            WsStatus::Connected => {
                                ui.colored_label(egui::Color32::GREEN, "🟢")
                                    .on_hover_text("WebSocket connesso");
                            },
                            WsStatus::Connecting => {
                                ui.colored_label(egui::Color32::YELLOW, "🟡")
                                    .on_hover_text("Connessione in corso...");
                            },
                            WsStatus::Disconnected => {
                                ui.colored_label(egui::Color32::RED, "🔴")
                                    .on_hover_text("WebSocket disconnesso");
                            },
                        };

                        // User info se autenticato
                        if let Some(ref username) = self.state.token.as_ref().map(|_| &self.state.username) {
                            ui.separator();
                            ui.label(format!("👤 {}", username));
                            if self.state.is_loading && !self.state.is_initial_load_complete {
                                ui.spinner();
                                ui.label("Caricamento...");
                            }
                        }
                    });
                });
                ui.add_space(4.0);
            });

        // Se non è autenticato, mostra solo il pannello di auth
        if self.state.token.is_none() {
            egui::CentralPanel::default().show(ctx, |ui| {
                // Centra il pannello di login
                ui.vertical_centered(|ui| {
                    ui.add_space(100.0);
                    ui.group(|ui| {
                        ui.set_max_width(400.0);
                        ui::auth::panel(ui, &mut self.state);
                    });
                });
            });
        } else {
            // Layout WhatsApp: sidebar sinistra + contenuto principale
            egui::SidePanel::left("sidebar")
                .resizable(true)
                .default_width(350.0)
                .min_width(300.0)
                .max_width(500.0)
                .show(ctx, |ui| {
                    self.show_sidebar(ui);
                });

            egui::CentralPanel::default().show(ctx, |ui| {
                match self.state.page {
                    Page::Chat => {
                        ui::chat::panel(ui, &mut self.state);
                    },
                    Page::GroupManagement => {
                        ui::conversation_management::panel(ui, &mut self.state);
                    },
                    _ => {
                        // Per altre pagine o stati di fallback
                        self.show_welcome_screen(ui);
                    }
                }
            });
        }

        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if let Some(ctrl) = self.state.ws_ctrl.take() {
            let _ = ctrl.shutdown.send(());
        }
    }
}

impl App {
    fn show_sidebar(&mut self, ui: &mut egui::Ui) {
        ui.vertical(|ui| {
            // Header della sidebar con tabs
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.state.page, Page::Conversations, "💬 Chat");
                ui.selectable_value(&mut self.state.page, Page::GroupManagement, "🔧 Gestione");
                ui.selectable_value(&mut self.state.page, Page::Auth, "⚙️ Account");
            });

            ui.separator();
            ui.add_space(4.0);

            // Contenuto della sidebar basato sulla tab selezionata
            match self.state.page {
                Page::Auth => {
                    self.show_account_sidebar(ui);
                },
                Page::Conversations | Page::Chat => {
                    self.show_conversations_sidebar(ui);
                },
                Page::GroupManagement => {
                    self.show_group_management_sidebar(ui);
                }
            }
        });
    }

    fn show_account_sidebar(&mut self, ui: &mut egui::Ui) {
        ui.heading("⚙️ Account");
        ui.separator();
        ui.add_space(8.0);

        if let Some(ref username) = self.state.token.as_ref().map(|_| &self.state.username) {
            ui.group(|ui| {
                ui.label(egui::RichText::new("Informazioni Utente").strong());
                ui.separator();

                ui.horizontal(|ui| {
                    ui.label("👤 Username:");
                    ui.label(egui::RichText::new(*username).strong());
                });

                if let Some(user_id) = self.state.user_id {
                    ui.horizontal(|ui| {
                        ui.label("🆔 ID:");
                        ui.label(egui::RichText::new(&user_id.to_string()[..8]).code());
                    });
                }
            });

            ui.add_space(12.0);

            ui.group(|ui| {
                ui.label(egui::RichText::new("Configurazione").strong());
                ui.separator();

                ui.horizontal(|ui| {
                    ui.label("🌐 Server:");
                    ui.add(egui::TextEdit::singleline(&mut self.state.base).desired_width(200.0));
                });

                ui.add_space(8.0);

                ui.horizontal(|ui| {
                    ui.label("🔌 WebSocket:");
                    match self.state.ws_status {
                        WsStatus::Connected => ui.colored_label(egui::Color32::GREEN, "Connesso"),
                        WsStatus::Connecting => ui.colored_label(egui::Color32::YELLOW, "Connessione..."),
                        WsStatus::Disconnected => ui.colored_label(egui::Color32::RED, "Disconnesso"),
                    };

                    if self.state.ws_status == WsStatus::Disconnected {
                        if ui.small_button("🔄").on_hover_text("Riconnetti").clicked() {
                            self.state.request_ws_reconnect = true;
                        }
                    }
                });
            });

            ui.add_space(12.0);

            // Statistiche
            if let Some(ref conversations) = self.state.conversations {
                ui.group(|ui| {
                    ui.label(egui::RichText::new("Statistiche").strong());
                    ui.separator();

                    let groups = conversations.iter().filter(|c| c.kind == "group").count();
                    let dms = conversations.iter().filter(|c| c.kind == "dm").count();
                    let total_messages = self.state.conversation_messages.values().map(|msgs| msgs.len()).sum::<usize>();

                    ui.horizontal(|ui| {
                        ui.label("👥 Gruppi:");
                        ui.label(egui::RichText::new(groups.to_string()).strong());
                    });
                    ui.horizontal(|ui| {
                        ui.label("💬 Chat private:");
                        ui.label(egui::RichText::new(dms.to_string()).strong());
                    });
                    ui.horizontal(|ui| {
                        ui.label("📝 Messaggi totali:");
                        ui.label(egui::RichText::new(total_messages.to_string()).strong());
                    });
                });

                ui.add_space(12.0);
            }

            if ui.button(egui::RichText::new("🚪 Logout").color(egui::Color32::WHITE))
                .on_hover_text("Esci dall'applicazione")
                .clicked() {
                if let Some(token) = &self.state.token {
                    let base = self.state.base.clone();
                    let token = token.clone();
                    let tx = self.state.ui_tx.clone();
                    self.state.rt.spawn(async move {
                        if let Err(e) = crate::net::auth::logout(&base, &token).await {
                            let _ = tx.send(UiEvent::Info(format!("logout note: {e}")));
                        }
                        let _ = tx.send(UiEvent::LoggedOut);
                    });
                }
            }
        }
    }

    fn show_conversations_sidebar(&mut self, ui: &mut egui::Ui) {
        let token = self.state.token.clone().unwrap();

        // Auto-carica conversazioni se non sono ancora state caricate
        if self.state.conversations.is_none() {
            let base = self.state.base.clone();
            let token2 = token.clone();
            let tx = self.state.ui_tx.clone();
            self.state.rt.spawn(async move {
                match crate::net::conversation::get_conversations(&base, &token2).await {
                    Ok(conversations) => {
                        let _ = tx.send(UiEvent::ConversationsLoaded(conversations));
                    }
                    Err(e) => {
                        let _ = tx.send(UiEvent::Error(format!("Caricamento conversazioni fallito: {e}")));
                    }
                }
            });
        }

        // Header con pulsante refresh e search
        ui.horizontal(|ui| {
            ui.heading("💬 Conversazioni");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.small_button("🔄").on_hover_text("Ricarica conversazioni").clicked() {
                    let base = self.state.base.clone();
                    let token2 = token.clone();
                    let tx = self.state.ui_tx.clone();
                    self.state.rt.spawn(async move {
                        match crate::net::conversation::get_conversations(&base, &token2).await {
                            Ok(conversations) => {
                                let _ = tx.send(UiEvent::ConversationsLoaded(conversations));
                            }
                            Err(e) => {
                                let _ = tx.send(UiEvent::Error(format!("Caricamento conversazioni fallito: {e}")));
                            }
                        }
                    });
                }

                if ui.small_button("➕").on_hover_text("Vai alla gestione gruppi").clicked() {
                    self.state.page = Page::GroupManagement;
                }
            });
        });

        ui.separator();
        ui.add_space(4.0);

        // Lista conversazioni con scroll
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if let Some(ref conversations) = self.state.conversations {
                    if conversations.is_empty() {
                        ui.vertical_centered(|ui| {
                            ui.add_space(40.0);
                            ui.label(egui::RichText::new("📭").size(32.0));
                            ui.add_space(8.0);
                            ui.colored_label(egui::Color32::GRAY, "Nessuna conversazione");
                            ui.add_space(8.0);
                            ui.small_button("Vai a Gestione per crearne una").clicked().then(|| {
                                self.state.page = Page::GroupManagement;
                            });
                        });
                    } else {
                        for conv in conversations {
                            let is_selected = self.state.cid.map_or(false, |cid| cid == conv.id);

                            // Frame per ogni conversazione
                            let response = ui.allocate_response(
                                egui::vec2(ui.available_width(), 60.0),
                                egui::Sense::click()
                            );

                            // Colore di sfondo basato su selezione e hover
                            let bg_color = if is_selected {
                                egui::Color32::from_rgb(70, 100, 200)
                            } else if response.hovered() {
                                egui::Color32::from_rgb(50, 50, 55)
                            } else {
                                egui::Color32::TRANSPARENT
                            };

                            // Disegna il frame della conversazione
                            ui.painter().rect_filled(
                                response.rect,
                                egui::Rounding::same(8.0),
                                bg_color
                            );

                            // Contenuto della conversazione
                            ui.allocate_ui_at_rect(response.rect.shrink(8.0), |ui| {
                                ui.horizontal(|ui| {
                                    let (icon, _) = match conv.kind.as_str() {
                                        "group" => ("👥", egui::Color32::BLUE),
                                        "dm" => ("💬", egui::Color32::GREEN),
                                        _ => ("📄", egui::Color32::GRAY),
                                    };

                                    ui.label(egui::RichText::new(icon).size(20.0));
                                    ui.vertical(|ui| {
                                        ui.label(egui::RichText::new(&conv.title)
                                            .strong()
                                            .color(egui::Color32::WHITE));

                                        // Mostra anteprima ultimo messaggio se disponibile
                                        if let Some(messages) = self.state.conversation_messages.get(&conv.id) {
                                            if let Some(last_msg) = messages.last() {
                                                let preview = if last_msg.content.len() > 30 {
                                                    format!("{}...", &last_msg.content[..30])
                                                } else {
                                                    last_msg.content.clone()
                                                };
                                                ui.label(egui::RichText::new(preview)
                                                    .small()
                                                    .color(if is_selected {
                                                        egui::Color32::LIGHT_GRAY
                                                    } else {
                                                        egui::Color32::GRAY
                                                    }));
                                            }
                                        }
                                    });
                                });
                            });

                            if response.clicked() {
                                let _ = self.state.ui_tx.send(UiEvent::Opened(conv.id));
                                self.state.page = Page::Chat;
                            }
                        }
                    }
                } else {
                    ui.vertical_centered(|ui| {
                        ui.add_space(40.0);
                        ui.spinner();
                        ui.add_space(8.0);
                        ui.label("Caricamento conversazioni...");
                    });
                }
            });
    }

    fn show_group_management_sidebar(&mut self, ui: &mut egui::Ui) {
        ui.heading("🔧 Gestione");
        ui.separator();
        ui.add_space(8.0);

        ui.label("Usa il pannello principale per gestire gruppi e inviti");

        ui.add_space(12.0);

        if ui.button("← Torna alle Chat").clicked() {
            self.state.page = Page::Conversations;
        }
    }

    fn show_welcome_screen(&mut self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(100.0);

            ui.label(egui::RichText::new("🦀").size(64.0));
            ui.add_space(16.0);
            ui.heading(egui::RichText::new("Benvenuto in Ruggine Chat").size(24.0));
            ui.add_space(8.0);
            ui.label("Seleziona una conversazione dalla barra laterale per iniziare a chattare");

            ui.add_space(20.0);

            if self.state.conversations.as_ref().map_or(true, |c| c.is_empty()) {
                ui.label("Sembra che tu non abbia ancora conversazioni!");
                ui.add_space(8.0);
                if ui.button("Crea la tua prima conversazione").clicked() {
                    self.state.page = Page::GroupManagement;
                }
            }
        });
    }
}