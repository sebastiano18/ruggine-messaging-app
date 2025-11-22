use crate::app::components;
use crate::app::events::sequence_handler::SequenceHandler;
use crate::app::header::HeaderManager;
use crate::app::sidebar::SidebarManager;
use crate::app::ws_manager::ws_manager::WebSocketManager;
use crate::models::{Page, WsStatus};
use crate::state::AppState;
use eframe::egui;

pub struct App {
    state: AppState,
    ws_manager: WebSocketManager,
    sidebar_manager: SidebarManager,
    header_manager: HeaderManager,

    // Debug info visibility
    show_debug_info: bool,
    last_debug_toggle: std::time::Instant,
}

impl App {
    pub fn new() -> Self {
        Self {
            state: AppState::new(),
            ws_manager: WebSocketManager::new(),
            sidebar_manager: SidebarManager::new(),
            header_manager: HeaderManager::new(),
            show_debug_info: cfg!(debug_assertions),
            last_debug_toggle: std::time::Instant::now(),
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.ws_manager.ensure_ws_lifecycle(&mut self.state);
        self.state.drain_events();
        // Prune expired toast notifications
        self.state.prune_expired_toasts(std::time::Duration::from_secs(5));
        self.handle_global_shortcuts(ctx);
        self.header_manager.show_header(ctx, &mut self.state);

        if self.show_debug_info {
            self.show_debug_panel(ctx);
        }

        if self.state.token.is_none() {
            self.show_auth_layout(ctx);
        } else {
            self.show_main_layout(ctx);
        }

        if self.state.token.is_some() && !matches!(self.state.page, Page::Auth) {
            self.show_toasts(ctx);
        }


        // Pop-up dettagli account centrale
        if self.state.token.is_some() && !matches!(self.state.page, Page::Auth) {
            self.show_toasts(ctx);
        }


        // Pop-up dettagli account centrale
        if self.state.show_account_modal {
            let mut open = self.state.show_account_modal;
            let mut should_close = false;

            egui::Window::new("")
                .id(egui::Id::new("account_modal_popup"))
                .title_bar(false)
                .collapsible(false)
                .resizable(false)
                .fixed_size([500.0, 500.0])
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .open(&mut open)
                .show(ctx, |ui| {
                    // Bottone X in alto a destra
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                        let close_button = egui::Button::new(
                            egui::RichText::new(egui_remixicon::icons::CLOSE_LINE)
                                .size(20.0)
                        )
                            .frame(false);

                        if ui.add(close_button).on_hover_text("Chiudi").clicked() {
                            should_close = true;
                        }
                    });

                    // Titolo centrato
                    ui.vertical_centered(|ui| {
                        ui.add_space(5.0);
                        ui.label(
                            egui::RichText::new(format!("{} Impostazioni Account", egui_remixicon::icons::SETTINGS_4_FILL))
                                .size(26.0)
                                .strong()
                                .color(egui::Color32::from_rgb(200, 100, 40))
                        );
                    });

                    ui.add_space(20.0);

                    // Frame con padding uniforme per il contenuto
                    egui::Frame::none()
                        .inner_margin(egui::Margin::symmetric(40.0, 0.0))
                        .show(ui, |ui| {
                            // === INFORMAZIONI UTENTE ===
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(egui_remixicon::icons::USER_FILL)
                                        .size(32.0)
                                        .color(egui::Color32::from_rgb(200, 100, 40))
                                );
                                ui.add_space(16.0);
                                ui.vertical(|ui| {
                                    ui.label(
                                        egui::RichText::new("Username")
                                            .size(12.0)
                                            .color(ui.visuals().weak_text_color())
                                    );
                                    ui.label(
                                        egui::RichText::new(&self.state.username)
                                            .size(18.0)
                                            .strong()
                                    );
                                });
                            });

                            ui.add_space(20.0);

                            // === STATISTICHE ===
                            if let Some(ref conversations) = self.state.conversations {
                                let groups = conversations.iter().filter(|c| c.kind == "group").count();
                                let dms = conversations.iter().filter(|c| c.kind == "dm").count();

                                ui.horizontal(|ui| {
                                    // Gruppi
                                    ui.label(
                                        egui::RichText::new(egui_remixicon::icons::TEAM_FILL)
                                            .size(28.0)
                                            .color(egui::Color32::from_rgb(200, 100, 40))
                                    );
                                    ui.add_space(12.0);
                                    ui.vertical(|ui| {
                                        ui.label(
                                            egui::RichText::new("Gruppi")
                                                .size(12.0)
                                                .color(ui.visuals().weak_text_color())
                                        );
                                        ui.label(
                                            egui::RichText::new(groups.to_string())
                                                .size(20.0)
                                                .strong()
                                        );
                                    });

                                    ui.add_space(40.0);

                                    // Chat private
                                    ui.label(
                                        egui::RichText::new(egui_remixicon::icons::CHAT_1_FILL)
                                            .size(28.0)
                                            .color(egui::Color32::from_rgb(200, 100, 40))
                                    );
                                    ui.add_space(12.0);
                                    ui.vertical(|ui| {
                                        ui.label(
                                            egui::RichText::new("Chat Private")
                                                .size(12.0)
                                                .color(ui.visuals().weak_text_color())
                                        );
                                        ui.label(
                                            egui::RichText::new(dms.to_string())
                                                .size(20.0)
                                                .strong()
                                        );
                                    });
                                });
                            }

                            ui.add_space(20.0);

                            // === CONFIGURAZIONE SERVER ===
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(egui_remixicon::icons::GLOBAL_FILL)
                                        .size(28.0)
                                        .color(egui::Color32::from_rgb(200, 100, 40))
                                );
                                ui.add_space(16.0);
                                ui.vertical(|ui| {
                                    ui.label(
                                        egui::RichText::new("Server")
                                            .size(12.0)
                                            .color(ui.visuals().weak_text_color())
                                    );
                                    ui.label(
                                        egui::RichText::new(&self.state.base)
                                            .size(14.0)
                                            .font(egui::FontId::monospace(14.0))
                                    );
                                });
                            });

                            ui.add_space(20.0);

                            // === STATO WEBSOCKET ===
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(egui_remixicon::icons::WIFI_FILL)
                                        .size(28.0)
                                        .color(egui::Color32::from_rgb(200, 100, 40))
                                );
                                ui.add_space(16.0);
                                ui.vertical(|ui| {
                                    ui.label(
                                        egui::RichText::new("WebSocket")
                                            .size(12.0)
                                            .color(ui.visuals().weak_text_color())
                                    );
                                    ui.horizontal(|ui| {
                                        match self.state.ws_status {
                                            crate::models::WsStatus::Connected => {
                                                ui.label(
                                                    egui::RichText::new(egui_remixicon::icons::CHECKBOX_CIRCLE_FILL)
                                                        .size(18.0)
                                                        .color(egui::Color32::GREEN)
                                                );
                                                ui.label(
                                                    egui::RichText::new("Connesso")
                                                        .size(14.0)
                                                );
                                            },
                                            crate::models::WsStatus::Connecting => {
                                                ui.label(
                                                    egui::RichText::new(egui_remixicon::icons::REFRESH_FILL)
                                                        .size(18.0)
                                                        .color(egui::Color32::YELLOW)
                                                );
                                                ui.label(
                                                    egui::RichText::new("Connessione...")
                                                        .size(14.0)
                                                );
                                            },
                                            crate::models::WsStatus::Disconnected => {
                                                ui.label(
                                                    egui::RichText::new(egui_remixicon::icons::CLOSE_CIRCLE_FILL)
                                                        .size(18.0)
                                                        .color(egui::Color32::RED)
                                                );
                                                ui.label(
                                                    egui::RichText::new("Disconnesso")
                                                        .size(14.0)
                                                );
                                                ui.add_space(8.0);
                                                if ui.small_button(egui_remixicon::icons::REFRESH_LINE)
                                                    .on_hover_text("Riconnetti")
                                                    .clicked()
                                                {
                                                    self.state.request_ws_reconnect = true;
                                                }
                                            },
                                        }
                                    });
                                });
                            });
                        });

                    ui.add_space(45.0);


                    // === BOTTONI AZIONE ===
                    egui::Frame::none()
                        .inner_margin(egui::Margin::symmetric(40.0, 0.0))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                // Logout a sinistra
                                let logout_btn = egui::Button::new(
                                    egui::RichText::new(format!("{} Logout", egui_remixicon::icons::LOGOUT_BOX_R_FILL))
                                        .size(14.0)
                                        .color(egui::Color32::WHITE)
                                )
                                    .fill(egui::Color32::from_rgb(200, 100, 40))
                                    .min_size(egui::vec2(180.0, 40.0));

                                if ui.add(logout_btn).clicked() {
                                    if let Some(token) = &self.state.token {
                                        let base = self.state.base.clone();
                                        let token = token.clone();
                                        let tx = self.state.ui_tx.clone();
                                        self.state.rt.spawn(async move {
                                            if let Err(e) = crate::api::auth::logout(&base, &token).await {
                                                let _ = tx.send(crate::models::UiEvent::Info(format!("logout note: {e}")));
                                            }
                                            let _ = tx.send(crate::models::UiEvent::LoggedOut);
                                        });
                                    }
                                }

                                // Elimina account a destra
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    let delete_btn = egui::Button::new(
                                        egui::RichText::new(format!("{} Elimina Account", egui_remixicon::icons::DELETE_BIN_FILL))
                                            .size(14.0)
                                            .color(egui::Color32::WHITE)
                                    )
                                        .fill(egui::Color32::from_rgb(180, 50, 50))
                                        .min_size(egui::vec2(180.0, 40.0));

                                    if ui.add(delete_btn).clicked() {
                                        let _ = self.state.ui_tx.send(crate::models::UiEvent::DeleteAccountStart);
                                    }
                                });
                            });
                        });
                });

            if should_close {
                open = false;
            }

            self.state.show_account_modal = open;
        }

        // Pop-up conferma eliminazione account
        if self.state.confirm_delete_account {
            egui::Window::new("")
                .id(egui::Id::new("delete_account_confirmation_popup"))
                .title_bar(false)
                .collapsible(false)
                .resizable(false)
                .fixed_size([480.0, 260.0])
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    // Titolo centrato
                    ui.vertical_centered(|ui| {
                        ui.add_space(20.0);
                        ui.label(
                            egui::RichText::new(format!("{} Elimina Account", egui_remixicon::icons::ALERT_FILL))
                                .size(24.0)
                                .strong()
                                .color(egui::Color32::from_rgb(220, 60, 60))
                        );
                        ui.add_space(12.0);
                        ui.label(
                            egui::RichText::new("Sei sicuro di voler eliminare il tuo account?")
                                .size(15.0)
                                .strong()
                        );
                        ui.add_space(8.0);
                        ui.label(
                            egui::RichText::new("Questa azione è irreversibile e comporterà:")
                                .size(12.0)
                                .color(ui.visuals().weak_text_color())
                        );
                    });

                    ui.add_space(16.0);

                    // Lista conseguenze
                    egui::Frame::none()
                        .inner_margin(egui::Margin::symmetric(60.0, 0.0))
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new(format!("{} Perdita di tutti i tuoi messaggi e conversazioni", egui_remixicon::icons::CHAT_DELETE_LINE))
                                    .size(12.0)
                                    .color(ui.visuals().weak_text_color())
                            );
                            ui.add_space(4.0);
                            ui.label(
                                egui::RichText::new(format!("{} Rimozione da tutti i gruppi", egui_remixicon::icons::TEAM_LINE))
                                    .size(12.0)
                                    .color(ui.visuals().weak_text_color())
                            );
                            ui.add_space(4.0);
                            ui.label(
                                egui::RichText::new(format!("{} Eliminazione permanente dei tuoi dati", egui_remixicon::icons::DELETE_BIN_LINE))
                                    .size(12.0)
                                    .color(ui.visuals().weak_text_color())
                            );
                        });

                    ui.add_space(24.0);

                    // Bottoni ai lati
                    egui::Frame::none()
                        .inner_margin(egui::Margin::symmetric(30.0, 0.0))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let cancel_button = egui::Button::new(
                                    egui::RichText::new(format!("{} Annulla", egui_remixicon::icons::CLOSE_LINE))
                                        .size(14.0)
                                )
                                    .fill(ui.visuals().widgets.inactive.bg_fill)
                                    .min_size(egui::vec2(160.0, 40.0));

                                if ui.add(cancel_button).clicked() {
                                    let _ = self.state.ui_tx.send(crate::models::UiEvent::DeleteAccountCancel);
                                }

                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    let confirm_button = egui::Button::new(
                                        egui::RichText::new(format!("{} Elimina Definitivamente", egui_remixicon::icons::DELETE_BIN_FILL))
                                            .size(14.0)
                                            .color(egui::Color32::WHITE)
                                    )
                                        .fill(egui::Color32::from_rgb(220, 20, 20))
                                        .min_size(egui::vec2(200.0, 40.0));

                                    if ui.add(confirm_button).clicked() {
                                        let _ = self.state.ui_tx.send(crate::models::UiEvent::DeleteAccountConfirm);
                                    }
                                });
                            });
                        });
                });
        }

        self.periodic_cleanup();
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        tracing::info!("App shutting down");

        if let Some(ctrl) = self.state.ws_ctrl.take() {
            let _ = ctrl.shutdown.send(());
        }

        let debug_info = self.state.get_debug_info();
        tracing::info!("Final app state: {:?}", debug_info);

        let stats = self.ws_manager.get_connection_stats();
        tracing::info!("Final connection stats: {:?}", stats);

        tracing::info!(
            "Final sequence stats - User seq: {}, Conv seqs: {}, Pings: {}, Pongs: {}, Gaps: {}",
            self.state.user_sequence_confirmed,
            self.state.conversation_sequences.len(),
            self.state.sequence_stats.ping_count,
            self.state.sequence_stats.pong_count,
            self.state.sequence_stats.gaps_detected
        );
    }
}

impl App {
    fn handle_global_shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::D)) {
            if self.last_debug_toggle.elapsed() > std::time::Duration::from_millis(500) {
                self.show_debug_info = !self.show_debug_info;
                self.last_debug_toggle = std::time::Instant::now();
                tracing::info!("Debug panel toggled: {}", self.show_debug_info);
            }
        }

        if ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::R)) {
            self.state.request_ws_reconnect = true;
            tracing::info!("Manual WebSocket reconnection requested");
        }

        if ctx.input(|i| i.key_pressed(egui::Key::F5)) {
            self.state.request_conversations_refresh = true;
            tracing::info!("Manual conversations refresh requested");
        }
    }

    fn show_debug_panel(&mut self, ctx: &egui::Context) {
        egui::Window::new("🔧 Debug Info")
            .default_size([450.0, 500.0])
            .resizable(true)
            .collapsible(true)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.strong("📌 Connection Status");
                    ui.separator();

                    let ws_status_text = match self.state.ws_status {
                        WsStatus::Connected => "🟢 Connected",
                        WsStatus::Connecting => "🟡 Connecting",
                        WsStatus::Disconnected => "🔴 Disconnected",
                    };
                    ui.label(format!("WebSocket: {}", ws_status_text));
                    ui.label(format!(
                        "Authenticated: {}",
                        if self.state.is_authenticated() {
                            "✅"
                        } else {
                            "❌"
                        }
                    ));

                    if let Some(uptime) = self.ws_manager.get_connection_stats().current_uptime() {
                        ui.label(format!("Uptime: {:?}", uptime));
                    }

                    ui.add_space(10.0);

                    // DUAL Sequence System
                    ui.strong("🔄 Dual Sequence System");
                    ui.separator();

                    ui.label(egui::RichText::new("User Events:").strong());
                    ui.label(format!(
                        "  Confirmed: {}",
                        self.state.user_sequence_confirmed
                    ));
                    ui.label(format!("  Received: {}", self.state.user_sequence_received));
                    let user_gap =
                        if self.state.user_sequence_received > self.state.user_sequence_confirmed {
                            self.state.user_sequence_received - self.state.user_sequence_confirmed
                        } else {
                            0
                        };
                    if user_gap > 0 {
                        ui.colored_label(
                            egui::Color32::YELLOW,
                            format!("  Gap: {} events", user_gap),
                        );
                    }

                    ui.add_space(5.0);

                    ui.label(egui::RichText::new("Active Conversation:").strong());
                    if let Some(cid) = self.state.cid {
                        ui.label(format!("  ID: {}", cid));
                        let conv_seq = self
                            .state
                            .conversation_sequences
                            .get(&cid)
                            .copied()
                            .unwrap_or(0);
                        let conv_seq_conf = self
                            .state
                            .conversation_sequences_confirmed
                            .get(&cid)
                            .copied()
                            .unwrap_or(0);
                        ui.label(format!("  Confirmed: {}", conv_seq_conf));
                        ui.label(format!("  Received: {}", conv_seq));

                        let conv_gap = if conv_seq > conv_seq_conf {
                            conv_seq - conv_seq_conf
                        } else {
                            0
                        };
                        if conv_gap > 0 {
                            ui.colored_label(
                                egui::Color32::YELLOW,
                                format!("  Gap: {} messages", conv_gap),
                            );
                        }
                    } else {
                        ui.label("  None selected");
                    }

                    ui.label(format!(
                        "Total tracked conversations: {}",
                        self.state.conversation_sequences.len()
                    ));

                    ui.add_space(10.0);

                    ui.strong("🔄 Recovery State");
                    ui.separator();
                    ui.label(format!(
                        "Recovering user events: {}",
                        self.state.is_recovering_user_events
                    ));
                    ui.label(format!(
                        "Recovering conversations: {}",
                        self.state.is_recovering_messages.len()
                    ));
                    ui.label(format!(
                        "Pending resume requests: {}",
                        self.state.pending_resume_requests
                    ));

                    ui.add_space(10.0);

                    ui.strong("📡 Ping/Pong");
                    ui.separator();

                    ui.label(format!(
                        "Sequence Health: {:.2}",
                        SequenceHandler::get_sequence_health(&self.state)
                    ));
                    ui.label(format!(
                        "Missed Pings: {}/{}",
                        self.state.missed_pings, self.state.max_missed_pings
                    ));

                    let next_ping_secs = (self.state.ping_interval.as_secs() as f64
                        - self.state.last_ping_time.elapsed().as_secs_f64())
                    .max(0.0);
                    ui.label(format!("Next Ping: {:.1}s", next_ping_secs));
                    ui.label(format!(
                        "Ping Timeout: {:.0}s",
                        self.state.ping_timeout.as_secs_f64()
                    ));

                    ui.add_space(10.0);

                    ui.strong("📊 Sequence Statistics");
                    ui.separator();

                    let stats = &self.state.sequence_stats;
                    ui.label(format!(
                        "Total events received: {}",
                        stats.total_events_received
                    ));
                    ui.label(format!("Gaps detected: {}", stats.gaps_detected));
                    ui.label(format!("Pings sent: {}", stats.ping_count));
                    ui.label(format!("Pongs received: {}", stats.pong_count));

                    ui.add_space(10.0);

                    ui.strong("💬 Message Cache");
                    ui.separator();

                    ui.label(format!(
                        "Conversations cached: {}",
                        self.state.conversation_messages.len()
                    ));
                    let total_messages: usize = self
                        .state
                        .conversation_messages
                        .values()
                        .map(|v| v.len())
                        .sum();
                    ui.label(format!("Total messages cached: {}", total_messages));

                    if let Some(cid) = self.state.cid {
                        if let Some(messages) = self.state.conversation_messages.get(&cid) {
                            ui.label(format!("Current conversation messages: {}", messages.len()));

                            let sequenced =
                                messages.iter().filter(|m| m.sequence_num.is_some()).count();
                            ui.label(format!("  With sequence: {}", sequenced));
                            ui.label(format!(
                                "  Without sequence: {}",
                                messages.len() - sequenced
                            ));
                        }
                    }

                    ui.add_space(10.0);

                    ui.strong("🔄 Connection Stats");
                    ui.separator();

                    let conn_stats = self.ws_manager.get_connection_stats();

                    if let Some(uptime) = conn_stats.current_uptime() {
                        ui.label(format!("Current uptime: {:?}", uptime));
                    }

                    ui.add_space(10.0);

                    ui.strong("🛠️ Actions");
                    ui.separator();

                    ui.horizontal(|ui| {
                        if ui.button("🗑️ Clear Cache").clicked() {
                            self.state.conversation_messages.clear();
                            tracing::info!("Message cache cleared");
                        }

                        if ui.button("📊 Reset Seq Stats").clicked() {
                            self.state.sequence_stats = Default::default();
                            tracing::info!("Sequence statistics reset");
                        }

                        if ui.button("🔄 Reset WS Stats").clicked() {
                            self.ws_manager.reset_stats();
                        }
                    });

                    ui.add_space(5.0);
                    ui.separator();
                    if ui.button("⚠️ Reset ALL Sequences (Dangerous)").clicked() {
                        self.state.user_sequence_confirmed = 0;
                        self.state.user_sequence_received = 0;
                        self.state.conversation_sequences.clear();
                        self.state.conversation_sequences_confirmed.clear();
                        self.state.sequence_stats = Default::default();
                        tracing::warn!("Manual sequence reset performed - ALL sequences cleared");
                    }
                });
            });
    }

    fn show_auth_layout(&mut self, ctx: &egui::Context) {
        // Lascia che auth::panel gestisca tutto il fullscreen
        egui::CentralPanel::default()
            .frame(egui::Frame::none())
            .show(ctx, |ui| {
                components::auth::panel(ui, &mut self.state);
            });
    }

    fn show_main_layout(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("sidebar")
            .resizable(true)
            .default_width(350.0)
            .min_width(300.0)
            .max_width(500.0)
            .show(ctx, |ui| {
                self.sidebar_manager.show_sidebar(ui, &mut self.state);
            });

        egui::CentralPanel::default().show(ctx, |ui| match self.state.page {
            Page::Chat => {
                components::chat::panel(ui, &mut self.state);
            }
            Page::Conversations => {
                self.show_welcome_screen(ui);
            }
            Page::Auth => {
                self.show_welcome_screen(ui);
            }
        });
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

            match self.state.ws_status {
                WsStatus::Connected => {
                    ui.colored_label(egui::Color32::GREEN, "🟢 Sincronizzato");
                    if self.state.user_sequence_confirmed > 0 {
                        ui.label(format!(
                            "Ultimo evento utente: #{}",
                            self.state.user_sequence_confirmed
                        ));
                    }

                    let health = SequenceHandler::get_sequence_health(&self.state);
                    if health < 1.0 {
                        let health_color = if health > 0.8 {
                            egui::Color32::YELLOW
                        } else {
                            egui::Color32::RED
                        };
                        ui.colored_label(
                            health_color,
                            format!("Salute sincronizzazione: {:.1}%", health * 100.0),
                        );
                    }
                }
                WsStatus::Connecting => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.colored_label(egui::Color32::YELLOW, "🟡 Connessione in corso...");
                    });
                }
                WsStatus::Disconnected => {
                    ui.colored_label(
                        egui::Color32::RED,
                        "🔴 Disconnesso - riconnessione automatica...",
                    );
                    if self.state.user_sequence_confirmed > 0 {
                        ui.colored_label(
                            egui::Color32::GRAY,
                            format!(
                                "Ultima sequenza utente nota: #{}",
                                self.state.user_sequence_confirmed
                            ),
                        );
                    }
                }
            }

            ui.add_space(20.0);

            let has_conversations = self
                .state
                .conversations
                .as_ref()
                .map_or(false, |convs| !convs.is_empty());

            if !has_conversations {
                ui.label("Sembra che tu non abbia ancora conversazioni!");
                ui.add_space(8.0);
            } else {
                if self.state.cid.is_none() {
                    ui.label("Hai delle conversazioni disponibili!");
                    ui.add_space(8.0);
                    ui.label("Selezionane una dalla barra laterale per iniziare a chattare.");
                }
            }

            if self.state.is_loading && !self.state.is_initial_load_complete {
                ui.add_space(20.0);
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Caricamento dati in corso...");
                });
            }

            ui.add_space(30.0);
            ui.separator();
            ui.add_space(10.0);
            ui.label(
                egui::RichText::new("Shortcuts utili:")
                    .size(12.0)
                    .color(egui::Color32::GRAY),
            );
            ui.label(
                egui::RichText::new("Ctrl+D: Debug Panel • Ctrl+R: Riconnetti • F5: Aggiorna")
                    .size(10.0)
                    .color(egui::Color32::GRAY),
            );
        });
    }

    fn periodic_cleanup(&mut self) {
        static mut LAST_CLEANUP: Option<std::time::Instant> = None;

        let should_cleanup = unsafe {
            LAST_CLEANUP.map_or(true, |last| {
                last.elapsed() > std::time::Duration::from_secs(300)
            })
        };

        if should_cleanup {
            self.state.cleanup_old_data();
            self.state.cleanup_dm_stubs();

            unsafe {
                LAST_CLEANUP = Some(std::time::Instant::now());
            }
            tracing::debug!("Periodic cleanup completed");

            let stats = self.state.get_debug_info();
            tracing::debug!(
                "Cleanup stats: {} conversations, {} messages, {} stubs",
                stats.get("conversations").unwrap_or(&"0".to_string()),
                stats.get("cached_messages").unwrap_or(&"0".to_string()),
                stats.get("dm_stubs").unwrap_or(&"0".to_string())
            );
        }
    }

    fn show_toasts(&mut self, ctx: &egui::Context) {
    const TOP_MARGIN: f32 = 100.0;
    const SLIDE_IN_DURATION: f32 = 0.7; // secondi per la slide-in (più fluido)
    const SLIDE_IN_OFFSET: f32 = 40.0; // pixel di partenza sopra la posizione finale
        const RIGHT_PADDING: f32 = 48.0;

        const ICON_WIDTH: f32 = 24.0;
        const CLOSE_WIDTH: f32 = 22.0;
        const H_PADDING: f32 = 28.0;
        const MIN_WIDTH: f32 = 220.0;
        const MAX_ABS_WIDTH: f32 = 640.0;      
        const MIN_MAX_WIDTH: f32 = 300.0;      
        const SCREEN_RATIO: f32 = 0.55;        
        const MULTILINE_MIN_TEXT: f32 = 80.0;
        const MULTILINE_SECOND_MIN: f32 = 60.0;
        const BASE_ALPHA: f32 = 0.4;
        const FADE_START: f32 = 9.5;
        const FADE_END: f32 = 10.0;

        let screen_rect = ctx.input(|i| i.screen_rect());
        let max_width = (screen_rect.width() * SCREEN_RATIO)
            .min(MAX_ABS_WIDTH)
            .max(MIN_MAX_WIDTH);

        use std::collections::HashSet;
        let mut to_remove: HashSet<uuid::Uuid> = HashSet::new();
        let mut y_offset = 0.0;

    for toast in &self.state.toasts {
            let (bg, icon) = match toast.kind {
                crate::state::ToastKind::Info => (
                    egui::Color32::from_rgb(40, 120, 40),
                    egui_remixicon::icons::INFORMATION_LINE,
                ),
                crate::state::ToastKind::Error => (
                    egui::Color32::from_rgb(160, 40, 40),
                    egui_remixicon::icons::ERROR_WARNING_LINE,
                ),
            };

            let font_id = egui::TextStyle::Body.resolve(&ctx.style());

            // Tentativo single-line: larghezza infinita (nessun wrap)
            let single_line_galley = ctx.fonts(|f| {
                f.layout(
                    toast.message.clone(),
                    font_id.clone(),
                    egui::Color32::WHITE,
                    f32::INFINITY, // no wrapping
                )
            });

            let raw_text_width = single_line_galley.size().x;
            let desired_single_line_width = raw_text_width + ICON_WIDTH + CLOSE_WIDTH + H_PADDING;

            let (galley, toast_width, final_text_width) = if desired_single_line_width <= max_width {
                let tw = desired_single_line_width.clamp(MIN_WIDTH, max_width);
                let inner = tw - ICON_WIDTH - CLOSE_WIDTH - H_PADDING;
                (single_line_galley, tw, inner)
            } else {
                let first_text_width = (max_width - ICON_WIDTH - CLOSE_WIDTH - H_PADDING).max(MULTILINE_MIN_TEXT);
                let galley_initial = ctx.fonts(|f| {
                    f.layout(toast.message.clone(), font_id.clone(), egui::Color32::WHITE, first_text_width)
                });

                let text_width_est = galley_initial.size().x;
                let tw = (text_width_est + ICON_WIDTH + CLOSE_WIDTH + H_PADDING).clamp(MIN_WIDTH, max_width);
                let final_text_width = (tw - ICON_WIDTH - CLOSE_WIDTH - H_PADDING).max(MULTILINE_SECOND_MIN);
                let galley_final = if (final_text_width - first_text_width).abs() > 1.0 {
                    ctx.fonts(|f| {
                        f.layout(toast.message.clone(), font_id.clone(), egui::Color32::WHITE, final_text_width)
                    })
                } else {
                    galley_initial
                };
                (galley_final, tw, final_text_width)
            };

            let toast_height = galley.size().y + 12.0;

            let pos_x = (screen_rect.max.x - toast_width - 12.0 - RIGHT_PADDING)
                .clamp(8.0, screen_rect.max.x - toast_width - 8.0);

            let ttl = toast.created.elapsed().as_secs_f32();
            let alpha = if ttl >= FADE_START {
                let t = ((FADE_END - ttl) / (FADE_END - FADE_START)).clamp(0.0, 1.0);
                t * BASE_ALPHA
            } else {
                BASE_ALPHA
            };
            let frame_bg = egui::Color32::from_rgba_premultiplied(bg.r(), bg.g(), bg.b(), (alpha * 255.0) as u8);

            // Slide-in: calcola offset verticale animato con easing (ease-out cubic)
            let mut slide_t = (ttl / SLIDE_IN_DURATION).clamp(0.0, 1.0);
            // Ease-out cubic: y = 1 - (1-t)^3
            slide_t = 1.0 - (1.0 - slide_t).powi(3);
            let slide_offset = SLIDE_IN_OFFSET * (1.0 - slide_t);
            let pos_y = TOP_MARGIN + y_offset - slide_offset;

            let area_id = egui::Id::new("toast").with(toast.id);
            let response = egui::Area::new(area_id)
                .order(egui::Order::Foreground)
                .fixed_pos(egui::pos2(pos_x, pos_y))
                .show(ctx, |ui| {
                    egui::Frame::none()
                        .fill(frame_bg)
                        .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgba_premultiplied(255, 255, 255, 60)))
                        .rounding(egui::Rounding::same(8.0))
                        .inner_margin(egui::Margin::symmetric(10.0, 6.0))
                        .show(ui, |ui| {
                            ui.set_width(toast_width);
                            ui.set_min_height(toast_height);

                            ui.horizontal_top(|ui| {
                                ui.label(egui::RichText::new(icon).size(18.0).color(egui::Color32::WHITE));
                                ui.add_space(6.0);

                                ui.vertical(|ui| {
                                    ui.set_width(final_text_width);
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(&toast.message)
                                                .size(13.0)
                                                .color(egui::Color32::WHITE),
                                        ).wrap(true)
                                    );
                                });

                                ui.add_space(4.0);

                                let close_btn = egui::Button::new(
                                    egui::RichText::new(egui_remixicon::icons::CLOSE_LINE)
                                        .size(14.0)
                                        .color(egui::Color32::from_rgba_premultiplied(255, 255, 255, 200)),
                                ).frame(false);

                                if ui.add(close_btn).on_hover_text("Chiudi").clicked() {
                                    to_remove.insert(toast.id);
                                }
                            });
                        });
                });

            y_offset += response.response.rect.height() + 8.0;
        }

        if !to_remove.is_empty() {
            self.state.toasts.retain(|t| !to_remove.contains(&t.id));
        }
    }
}
