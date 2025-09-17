use eframe::egui;
use crate::app::components;

use crate::app::header::HeaderManager;
use crate::app::ws_manager::ws_manager::WebSocketManager;
use crate::app::sidebar::SidebarManager;
use crate::models::{Page, WsStatus};
use crate::state::AppState;

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
            show_debug_info: cfg!(debug_assertions), // Debug mode per default in debug builds
            last_debug_toggle: std::time::Instant::now(),
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 1) CRITICO: Gestione ciclo di vita WebSocket PRIMA di tutto
        // Questo include ping automatico, rate limiting, health check
        self.ws_manager.ensure_ws_lifecycle(&mut self.state);

        // 2) Drena eventi UI (REST/WS) che aggiornano lo stato
        self.state.drain_events();

        // 3) Controlli tastiera globali
        self.handle_global_shortcuts(ctx);

        // 4) UI - Header sempre presente
        self.header_manager.show_header(ctx, &mut self.state);

        // 5) Debug panel (se abilitato)
        if self.show_debug_info {
            self.show_debug_panel(ctx);
        }

        // 6) Layout principale
        if self.state.token.is_none() {
            self.show_auth_layout(ctx);
        } else {
            self.show_main_layout(ctx);
        }

        // 7) Pulizia periodica memoria
        self.periodic_cleanup();

        // 8) Repaint continuo per UI reattiva
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        tracing::info!("App shutting down");

        // Shutdown graceful WebSocket
        if let Some(ctrl) = self.state.ws_ctrl.take() {
            let _ = ctrl.shutdown.send(());
        }

        // Log statistiche finali
        let debug_info = self.state.get_debug_info();
        tracing::info!("Final app state: {:?}", debug_info);

        let stats = self.ws_manager.get_connection_stats();
        tracing::info!("Final connection stats: {:?}", stats);

        // Log sequence stats finali
        tracing::info!("Final sequence stats - Last: {}, Pings: {}, Pongs: {}, Gaps: {}",
                      self.state.last_sequence_received,
                      self.state.sequence_stats.ping_count,
                      self.state.sequence_stats.pong_count,
                      self.state.sequence_stats.gaps_detected);
    }
}

impl App {
    /// Gestisce shortcut globali
    fn handle_global_shortcuts(&mut self, ctx: &egui::Context) {
        // Ctrl+D per toggle debug panel
        if ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::D)) {
            if self.last_debug_toggle.elapsed() > std::time::Duration::from_millis(500) {
                self.show_debug_info = !self.show_debug_info;
                self.last_debug_toggle = std::time::Instant::now();
                tracing::info!("Debug panel toggled: {}", self.show_debug_info);
            }
        }

        // Ctrl+R per forzare riconnessione WebSocket
        if ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::R)) {
            self.state.request_ws_reconnect = true;
            tracing::info!("Manual WebSocket reconnection requested");
        }

        // F5 per refresh conversazioni
        if ctx.input(|i| i.key_pressed(egui::Key::F5)) {
            self.state.request_conversations_refresh = true;
            tracing::info!("Manual conversations refresh requested");
        }
    }

    /// Panel debug con informazioni sistema sequenze COMPLETO
    fn show_debug_panel(&mut self, ctx: &egui::Context) {
        egui::Window::new("🔧 Debug Info")
            .default_size([450.0, 400.0])
            .resizable(true)
            .collapsible(true)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    // Informazioni generali
                    ui.strong("📌 Connection Status");
                    ui.separator();

                    let ws_status_text = match self.state.ws_status {
                        WsStatus::Connected => "🟢 Connected",
                        WsStatus::Connecting => "🟡 Connecting",
                        WsStatus::Disconnected => "🔴 Disconnected",
                    };
                    ui.label(format!("WebSocket: {}", ws_status_text));
                    ui.label(format!("Authenticated: {}", if self.state.is_authenticated() { "✅" } else { "❌" }));

                    // Connection uptime
                    if let Some(uptime) = self.ws_manager.get_connection_stats().current_uptime() {
                        ui.label(format!("Uptime: {:?}", uptime));
                    }

                    ui.add_space(10.0);

                    // Sistema sequenze - DETTAGLIATO
                    ui.strong("🔄 Sequence System");
                    ui.separator();

                    ui.label(format!("Last Sequence: {}", self.state.last_sequence_received));
                    ui.label(format!("Sequence Health: {:.2}", self.state.get_sequence_health()));
                    ui.label(format!("Missed Pings: {}/{}", self.state.missed_pings, self.state.max_missed_pings));

                    let next_ping_secs = (self.state.ping_interval.as_secs() as f64 -
                        self.state.last_ping_time.elapsed().as_secs_f64()).max(0.0);
                    ui.label(format!("Next Ping: {:.1}s", next_ping_secs));

                    ui.label(format!("Ping Timeout: {:.0}s", self.state.ping_timeout.as_secs_f64()));

                    if self.state.is_recovering_sequence {
                        ui.colored_label(egui::Color32::YELLOW, "🔄 Recovering sequence...");
                    }

                    ui.add_space(10.0);

                    // Statistiche sequence - COMPLETE
                    ui.strong("📊 Sequence Statistics");
                    ui.separator();

                    let stats = &self.state.sequence_stats;
                    ui.label(format!("Total Events: {}", stats.total_events_received));
                    ui.label(format!("Pings Sent: {}", stats.ping_count));
                    ui.label(format!("Pongs Received: {}", stats.pong_count));
                    ui.label(format!("Gaps Detected: {}", stats.gaps_detected));
                    ui.label(format!("Events Recovered: {}", stats.events_recovered));

                    if stats.ping_count > 0 {
                        let pong_rate = (stats.pong_count as f64 / stats.ping_count as f64) * 100.0;
                        ui.label(format!("Pong Success Rate: {:.1}%", pong_rate));
                    }

                    if stats.gaps_detected > 0 {
                        ui.label(format!("Avg Gap Size: {:.1}", stats.average_gap_size));

                        if let Some(last_gap) = stats.last_gap_time {
                            let gap_ago = last_gap.elapsed().as_secs();
                            ui.label(format!("Last Gap: {}s ago", gap_ago));
                        }
                    }

                    ui.add_space(10.0);

                    // Cache info
                    ui.strong("💾 Cache Info");
                    ui.separator();

                    ui.label(format!("Conversations: {}",
                                     self.state.conversations.as_ref().map(|c| c.len()).unwrap_or(0)));
                    ui.label(format!("Cached Messages: {}", self.state.get_total_cached_messages()));
                    ui.label(format!("DM Stubs: {}", self.state.dm_stubs.len()));
                    ui.label(format!("Cached Conversations: {}", self.state.conversation_messages.len()));

                    ui.add_space(10.0);

                    // Connection stats - DETTAGLIATE
                    ui.strong("📈 Connection Stats");
                    ui.separator();

                    let conn_stats = self.ws_manager.get_connection_stats();
                    ui.label(format!("Messages Sent: {}", conn_stats.total_messages_sent));
                    ui.label(format!("Messages Received: {}", conn_stats.total_messages_received));
                    ui.label(format!("Connection Attempts: {}", conn_stats.connection_attempts));
                    ui.label(format!("Successful Connections: {}", conn_stats.successful_connections));
                    ui.label(format!("Disconnections: {}", conn_stats.disconnections));

                    if conn_stats.connection_attempts > 0 {
                        ui.label(format!("Success Rate: {:.1}%",
                                         conn_stats.connection_success_rate() * 100.0));
                    }

                    if let Some(ref error) = conn_stats.last_error {
                        ui.colored_label(egui::Color32::RED, format!("Last Error: {}", error));
                    }

                    ui.add_space(15.0);

                    // Controlli manuali - ESPANSI
                    ui.strong("🎮 Manual Controls");
                    ui.separator();

                    ui.horizontal(|ui| {
                        if ui.button("📡 Force Ping").clicked() {
                            self.state.send_ping();
                        }

                        if ui.button("🔌 Reconnect").clicked() {
                            self.state.request_ws_reconnect = true;
                        }
                    });

                    ui.horizontal(|ui| {
                        if ui.button("📋 Refresh Convs").clicked() {
                            self.state.request_conversations_refresh = true;
                        }

                        if ui.button("🧹 Cleanup").clicked() {
                            self.state.cleanup_old_data();
                            self.state.cleanup_dm_stubs();
                        }
                    });

                    ui.horizontal(|ui| {
                        if ui.button("📊 Reset Seq Stats").clicked() {
                            self.state.sequence_stats = Default::default();
                            tracing::info!("Sequence statistics reset");
                        }

                        if ui.button("🔄 Reset WS Stats").clicked() {
                            self.ws_manager.reset_stats();
                        }
                    });

                    // Sequence reset button (dangerous)
                    ui.add_space(5.0);
                    ui.separator();
                    if ui.button("⚠️ Reset Sequence (Dangerous)").clicked() {
                        self.state.last_sequence_received = 0;
                        self.state.sequence_stats = Default::default();
                        tracing::warn!("Manual sequence reset performed");
                    }
                });
            });
    }

    fn show_auth_layout(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            // Centra il pannello di login
            ui.vertical_centered(|ui| {
                ui.add_space(100.0);

                // Logo e titolo
                ui.label(egui::RichText::new("🦀").size(64.0));
                ui.add_space(16.0);
                ui.heading(egui::RichText::new("Ruggine Chat").size(32.0));
                ui.add_space(20.0);

                ui.group(|ui| {
                    ui.set_max_width(400.0);
                    components::auth::panel(ui, &mut self.state);
                });

                // Status WebSocket nella schermata di login
                ui.add_space(20.0);
                match self.state.ws_status {
                    WsStatus::Connecting => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label("Connessione in corso...");
                        });
                    }
                    WsStatus::Disconnected => {
                        ui.colored_label(egui::Color32::GRAY, "🔴 Disconnesso");
                    }
                    WsStatus::Connected => {
                        ui.colored_label(egui::Color32::GREEN, "🟢 Connesso");
                        if self.state.last_sequence_received > 0 {
                            ui.label(format!("Sequenza: #{}", self.state.last_sequence_received));
                        }
                    }
                }
            });
        });
    }

    fn show_main_layout(&mut self, ctx: &egui::Context) {
        // Layout WhatsApp: sidebar sinistra + contenuto principale
        egui::SidePanel::left("sidebar")
            .resizable(true)
            .default_width(350.0)
            .min_width(300.0)
            .max_width(500.0)
            .show(ctx, |ui| {
                self.sidebar_manager.show_sidebar(ui, &mut self.state);
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            match self.state.page {
                Page::Chat => {
                    components::chat::panel(ui, &mut self.state);
                },
                Page::GroupManagement => {
                    components::conversation_management::panel(ui, &mut self.state);
                },
                Page::Conversations => {
                    // Se siamo nella pagina Conversations ma non abbiamo una chat selezionata
                    self.show_welcome_screen(ui);
                },
                Page::Auth => {
                    // Questo non dovrebbe mai accadere se siamo nel main_layout
                    // ma lo gestiamo per sicurezza
                    self.show_welcome_screen(ui);
                }
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

            // Status connessione nella welcome screen - MIGLIORATO
            match self.state.ws_status {
                WsStatus::Connected => {
                    ui.colored_label(egui::Color32::GREEN, "🟢 Sincronizzato");
                    if self.state.last_sequence_received > 0 {
                        ui.label(format!("Ultimo evento: #{}", self.state.last_sequence_received));
                    }

                    // Mostra salute sequenze
                    let health = self.state.get_sequence_health();
                    if health < 1.0 {
                        let health_color = if health > 0.8 { egui::Color32::YELLOW } else { egui::Color32::RED };
                        ui.colored_label(health_color, format!("Salute sincronizzazione: {:.1}%", health * 100.0));
                    }
                }
                WsStatus::Connecting => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.colored_label(egui::Color32::YELLOW, "🟡 Connessione in corso...");
                    });
                }
                WsStatus::Disconnected => {
                    ui.colored_label(egui::Color32::RED, "🔴 Disconnesso - riconnessione automatica...");
                    if self.state.last_sequence_received > 0 {
                        ui.colored_label(egui::Color32::GRAY,
                                         format!("Ultima sequenza nota: #{}", self.state.last_sequence_received));
                    }
                }
            }

            ui.add_space(20.0);

            // Logica conversazioni
            let has_conversations = self.state.conversations
                .as_ref()
                .map_or(false, |convs| !convs.is_empty());

            if !has_conversations {
                ui.label("Sembra che tu non abbia ancora conversazioni!");
                ui.add_space(8.0);

                if ui.button("Crea la tua prima conversazione").clicked() {
                    self.state.page = Page::GroupManagement;
                }
            } else {
                if self.state.cid.is_none() {
                    ui.label("Hai delle conversazioni disponibili!");
                    ui.add_space(8.0);
                    ui.label("Selezionane una dalla barra laterale per iniziare a chattare.");
                }
            }

            // Stato di caricamento
            if self.state.is_loading && !self.state.is_initial_load_complete {
                ui.add_space(20.0);
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Caricamento dati in corso...");
                });
            }

            // Shortcuts info
            ui.add_space(30.0);
            ui.separator();
            ui.add_space(10.0);
            ui.label(egui::RichText::new("Shortcuts utili:").size(12.0).color(egui::Color32::GRAY));
            ui.label(egui::RichText::new("Ctrl+D: Debug Panel • Ctrl+R: Riconnetti • F5: Aggiorna").size(10.0).color(egui::Color32::GRAY));
        });
    }

    /// Pulizia periodica per evitare memory leaks
    fn periodic_cleanup(&mut self) {
        // Cleanup ogni 5 minuti
        static mut LAST_CLEANUP: Option<std::time::Instant> = None;

        let should_cleanup = unsafe {
            LAST_CLEANUP.map_or(true, |last| last.elapsed() > std::time::Duration::from_secs(300))
        };

        if should_cleanup {
            self.state.cleanup_old_data();
            self.state.cleanup_dm_stubs();

            unsafe { LAST_CLEANUP = Some(std::time::Instant::now()); }
            tracing::debug!("Periodic cleanup completed");

            // Log statistiche durante cleanup
            let stats = self.state.get_debug_info();
            tracing::debug!("Cleanup stats: {} conversations, {} messages, {} stubs",
                           stats.get("conversations").unwrap_or(&"0".to_string()),
                           stats.get("cached_messages").unwrap_or(&"0".to_string()),
                           stats.get("dm_stubs").unwrap_or(&"0".to_string()));
        }
    }
}