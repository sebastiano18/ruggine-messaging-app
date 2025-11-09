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

        // Pop-up dettagli account account centrale
        if self.state.show_account_modal {
            let mut open = self.state.show_account_modal;
            egui::Window::new(
                egui::RichText::new(format!(
                    "{} Impostazioni account",
                    egui_remixicon::icons::SETTINGS_4_FILL
                ))
                .color(egui::Color32::WHITE),
            )
            .collapsible(false)
            .resizable(true)
            .auto_sized()
            .min_width(300.0)
            .max_width(400.0)
            .open(&mut open)
            .anchor(egui::Align2::CENTER_CENTER, [-150.0, 0.0])
            .show(ctx, |ui| {
                self.header_manager
                    .show_account_popup_content(ui, &mut self.state);
            });
            self.state.show_account_modal = open;
        }

        // NOTA: Il popup di creazione gruppo è ora gestito in sidebar_conversation.rs
        // Non serve chiamarlo qui perché viene già renderizzato dalla sidebar

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
}
