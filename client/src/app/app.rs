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
                .fixed_size([500.0, 580.0])
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

                            ui.add_space(20.0);

                            // === STATO SINCRONIZZAZIONE ===
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(egui_remixicon::icons::REFRESH_FILL)
                                        .size(28.0)
                                        .color(egui::Color32::from_rgb(200, 100, 40))
                                );
                                ui.add_space(16.0);
                                ui.vertical(|ui| {
                                    ui.label(
                                        egui::RichText::new("Sincronizzazione")
                                            .size(12.0)
                                            .color(ui.visuals().weak_text_color())
                                    );

                                    let health = SequenceHandler::get_sequence_health(&self.state);
                                    let health_color = if health > 0.9 {
                                        egui::Color32::GREEN
                                    } else if health > 0.7 {
                                        egui::Color32::YELLOW
                                    } else {
                                        egui::Color32::RED
                                    };

                                    ui.horizontal(|ui| {
                                        ui.label(
                                            egui::RichText::new(format!("{:.0}%", health * 100.0))
                                                .size(16.0)
                                                .strong()
                                                .color(health_color)
                                        );
                                        ui.label(
                                            egui::RichText::new("salute")
                                                .size(12.0)
                                                .color(ui.visuals().weak_text_color())
                                        );
                                    });

                                    if self.state.user_sequence_confirmed > 0 {
                                        ui.label(
                                            egui::RichText::new(format!("Ultimo evento: #{}", self.state.user_sequence_confirmed))
                                                .size(11.0)
                                                .color(ui.visuals().weak_text_color())
                                        );
                                    }

                                    // Mostra gap se presente
                                    let user_gap = if self.state.user_sequence_received > self.state.user_sequence_confirmed {
                                        self.state.user_sequence_received - self.state.user_sequence_confirmed
                                    } else {
                                        0
                                    };
                                    if user_gap > 0 {
                                        ui.label(
                                            egui::RichText::new(format!("⚠ Gap: {} eventi", user_gap))
                                                .size(11.0)
                                                .color(egui::Color32::YELLOW)
                                        );
                                    }
                                });
                            });
                        });

                    ui.add_space(25.0);


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
                            egui::RichText::new("Questa azione è irreversibile. Tutti i tuoi dati verranno eliminati permanentemente.")
                                .size(13.0)
                                .color(ui.visuals().weak_text_color())
                        );
                    });

                    ui.add_space(30.0);

                    // Bottoni centrati
                    ui.vertical_centered(|ui| {
                        ui.horizontal(|ui| {
                            // Annulla
                            let cancel_btn = egui::Button::new(
                                egui::RichText::new(format!("{} Annulla", egui_remixicon::icons::CLOSE_LINE))
                                    .size(14.0)
                            )
                                .fill(ui.visuals().widgets.inactive.bg_fill)
                                .min_size(egui::vec2(150.0, 40.0));

                            if ui.add(cancel_btn).clicked() {
                                let _ = self.state.ui_tx.send(crate::models::UiEvent::DeleteAccountCancel);
                            }

                            ui.add_space(20.0);

                            // Conferma eliminazione
                            let confirm_btn = egui::Button::new(
                                egui::RichText::new(format!("{} Elimina Account", egui_remixicon::icons::DELETE_BIN_FILL))
                                    .size(14.0)
                                    .color(egui::Color32::WHITE)
                            )
                                .fill(egui::Color32::from_rgb(200, 50, 50))
                                .min_size(egui::vec2(180.0, 40.0));

                            if ui.add(confirm_btn).clicked() {
                                let _ = self.state.ui_tx.send(crate::models::UiEvent::DeleteAccountConfirm);
                            }
                        });
                    });

                    ui.add_space(20.0);
                });
        }

        // Periodic cleanup
        self.periodic_cleanup();

        // Request continuous repaints for smooth animations
        ctx.request_repaint();
    }
}

impl App {
    fn handle_global_shortcuts(&mut self, ctx: &egui::Context) {
        // Toggle debug panel with Ctrl+D
        if ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::D)) {
            // Debounce to prevent rapid toggling
            if self.last_debug_toggle.elapsed() > std::time::Duration::from_millis(200) {
                self.show_debug_info = !self.show_debug_info;
                self.last_debug_toggle = std::time::Instant::now();
                tracing::info!("Debug panel toggled: {}", self.show_debug_info);
            }
        }
    }

    fn show_auth_layout(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
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
            // Centra verticalmente
            let available_height = ui.available_height();
            ui.add_space(available_height / 3.0);

            // Icona chat grande
            ui.label(
                egui::RichText::new(egui_remixicon::icons::CHAT_SMILE_FILL)
                    .size(80.0)
            );

            ui.add_space(24.0);

            // Titolo
            ui.label(
                egui::RichText::new("Benvenuto in Ruggine Chat")
                    .size(28.0)
                    .strong(),
            );

            ui.add_space(12.0);

            // Sottotitolo
            ui.label(
                egui::RichText::new(
                    "Seleziona una conversazione dalla barra laterale per iniziare a chattare",
                )
                    .size(14.0)
                    .color(ui.visuals().weak_text_color()),
            );
        });
    }

    fn show_debug_panel(&mut self, ctx: &egui::Context) {
        egui::Window::new("🔧 Debug Info")
            .default_pos([10.0, 400.0])
            .default_width(320.0)
            .resizable(true)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.heading("Connection");
                    ui.horizontal(|ui| {
                        ui.label("WebSocket:");
                        match self.state.ws_status {
                            WsStatus::Connected => ui.colored_label(egui::Color32::GREEN, "Connected"),
                            WsStatus::Connecting => ui.colored_label(egui::Color32::YELLOW, "Connecting..."),
                            WsStatus::Disconnected => ui.colored_label(egui::Color32::RED, "Disconnected"),
                        };
                    });

                    ui.separator();
                    ui.heading("Sequences");
                    ui.label(format!("User confirmed: {}", self.state.user_sequence_confirmed));
                    ui.label(format!("User received: {}", self.state.user_sequence_received));
                    ui.label(format!("Conv sequences: {}", self.state.conversation_sequences.len()));
                    ui.label(format!("Missed pings: {}/{}", self.state.missed_pings, self.state.max_missed_pings));

                    ui.separator();
                    ui.heading("Stats");
                    ui.label(format!("Ping count: {}", self.state.sequence_stats.ping_count));
                    ui.label(format!("Pong count: {}", self.state.sequence_stats.pong_count));
                    ui.label(format!("Gaps detected: {}", self.state.sequence_stats.gaps_detected));
                    ui.label(format!("Events recovered: {}", self.state.sequence_stats.events_recovered));

                    ui.separator();
                    ui.heading("Data");
                    ui.label(format!("Conversations: {}", self.state.conversations.as_ref().map_or(0, |c| c.len())));
                    ui.label(format!("Cached messages: {}", self.state.get_total_cached_messages()));
                    ui.label(format!("DM stubs: {}", self.state.dm_stubs.len()));
                    ui.label(format!("Group stubs: {}", self.state.group_stubs.len()));
                    ui.label(format!("Pending confirmations: {}", self.state.pending_confirmations.len()));

                    ui.separator();
                    ui.heading("Recovery");
                    ui.label(format!("Recovering user events: {}", self.state.is_recovering_user_events));
                    ui.label(format!("Pending resume: {}", self.state.pending_resume_requests));

                    ui.separator();
                    if ui.button("Force Reconnect").clicked() {
                        self.state.request_ws_reconnect = true;
                    }
                    if ui.button("Reset WS Stats").clicked() {
                        self.ws_manager.reset_stats();
                    }

                    ui.separator();
                    ui.heading("Health");
                    let health = SequenceHandler::get_sequence_health(&self.state);
                    let health_color = if health > 0.8 {
                        egui::Color32::GREEN
                    } else if health > 0.5 {
                        egui::Color32::YELLOW
                    } else {
                        egui::Color32::RED
                    };
                    ui.colored_label(health_color, format!("Sequence Health: {:.1}%", health * 100.0));
                });
            });
    }

    fn periodic_cleanup(&mut self) {
        static mut LAST_GENERAL_CLEANUP: Option<std::time::Instant> = None;

        // 1. Cleanup stub: SOLO se ci sono stub in attesa (check leggero)
        if !self.state.dm_stubs.is_empty() || !self.state.group_stubs.is_empty() {
            self.state.cleanup_expired_stubs();
        }

        // 2. Cleanup generale: ogni 5 minuti (300 secondi)
        let should_general_cleanup = unsafe {
            LAST_GENERAL_CLEANUP.map_or(true, |last| {
                last.elapsed() > std::time::Duration::from_secs(300)
            })
        };

        if should_general_cleanup {
            self.state.cleanup_old_data();

            unsafe {
                LAST_GENERAL_CLEANUP = Some(std::time::Instant::now());
            }
            tracing::debug!("Periodic cleanup completed");

            let stats = self.state.get_debug_info();
            tracing::debug!(
                "Cleanup stats: {} conversations, {} messages, {} dm_stubs, {} group_stubs",
                stats.get("conversations").unwrap_or(&"0".to_string()),
                stats.get("cached_messages").unwrap_or(&"0".to_string()),
                stats.get("dm_stubs").unwrap_or(&"0".to_string()),
                stats.get("group_stubs").unwrap_or(&"0".to_string())
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
        const BASE_ALPHA: f32 = 0.92; // Aumentato per migliore visibilità

        const FADE_START: f32 = 4.0;
        const FADE_END: f32 = 5.0;

        let screen_rect = ctx.screen_rect();
        let max_width = (screen_rect.width() * SCREEN_RATIO).clamp(MIN_MAX_WIDTH, MAX_ABS_WIDTH);

        let is_dark = ctx.style().visuals.dark_mode;

        let mut y_offset: f32 = 0.0;
        let mut to_remove = std::collections::HashSet::new();

        for toast in &self.state.toasts {
            let (bg, text_color, icon_color, icon) = match toast.kind {
                crate::state::ToastKind::Info => {
                    let bg = if is_dark {
                        egui::Color32::from_rgb(28, 100, 28)      // Verde scuro per dark
                    } else {
                        egui::Color32::from_rgb(225, 245, 225)    // Verde chiaro per light
                    };
                    let text_color = if is_dark {
                        egui::Color32::from_rgb(240, 255, 240)    // Bianco-verde per dark
                    } else {
                        egui::Color32::from_rgb(20, 80, 20)       // Verde scuro per light
                    };
                    let icon_color = if is_dark {
                        egui::Color32::from_rgb(120, 220, 120)    // Verde brillante per dark
                    } else {
                        egui::Color32::from_rgb(30, 130, 30)      // Verde medio per light
                    };
                    (bg, text_color, icon_color, egui_remixicon::icons::INFORMATION_LINE)
                }
                crate::state::ToastKind::Error => {
                    let bg = if is_dark {
                        egui::Color32::from_rgb(120, 35, 35)      // Rosso scuro bilanciato con il verde
                    } else {
                        egui::Color32::from_rgb(200, 80, 80)      // Rosso medio, meno aggressivo
                    };
                    let text_color = if is_dark {
                        egui::Color32::from_rgb(255, 240, 240)    // Bianco-rosa per dark
                    } else {
                        egui::Color32::from_rgb(255, 255, 255)    // Bianco per light
                    };
                    let icon_color = if is_dark {
                        egui::Color32::from_rgb(255, 120, 120)    // Rosso brillante per dark
                    } else {
                        egui::Color32::from_rgb(255, 230, 230)    // Rosa chiaro per light
                    };
                    (bg, text_color, icon_color, egui_remixicon::icons::ERROR_WARNING_LINE)
                }
            };

            // Colore per il close button
            let close_color = if is_dark {
                egui::Color32::from_rgba_premultiplied(220, 220, 220, 200)
            } else {
                egui::Color32::from_rgba_premultiplied(80, 80, 80, 180)
            };

            // Colore del bordo adattivo
            let stroke_color = if is_dark {
                egui::Color32::from_rgba_premultiplied(255, 255, 255, 50)
            } else {
                egui::Color32::from_rgba_premultiplied(0, 0, 0, 40)
            };

            let font_id = egui::TextStyle::Body.resolve(&ctx.style());

            // Tentativo single-line: larghezza infinita (nessun wrap)
            let single_line_galley = ctx.fonts(|f| {
                f.layout(
                    toast.message.clone(),
                    font_id.clone(),
                    text_color,
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
                    f.layout(toast.message.clone(), font_id.clone(), text_color, first_text_width)
                });

                let text_width_est = galley_initial.size().x;
                let tw = (text_width_est + ICON_WIDTH + CLOSE_WIDTH + H_PADDING).clamp(MIN_WIDTH, max_width);
                let final_text_width = (tw - ICON_WIDTH - CLOSE_WIDTH - H_PADDING).max(MULTILINE_SECOND_MIN);
                let galley_final = if (final_text_width - first_text_width).abs() > 1.0 {
                    ctx.fonts(|f| {
                        f.layout(toast.message.clone(), font_id.clone(), text_color, final_text_width)
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
                        .stroke(egui::Stroke::new(1.0, stroke_color))
                        .rounding(egui::Rounding::same(8.0))
                        .inner_margin(egui::Margin::symmetric(10.0, 6.0))
                        .show(ui, |ui| {
                            ui.set_width(toast_width);
                            ui.set_min_height(toast_height);

                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 8.0;

                                // Icona - centrata verticalmente
                                ui.allocate_ui_with_layout(
                                    egui::vec2(ICON_WIDTH, toast_height),
                                    egui::Layout::centered_and_justified(egui::Direction::TopDown),
                                    |ui| {
                                        ui.label(egui::RichText::new(icon).size(18.0).color(icon_color));
                                    },
                                );

                                // Testo - centrato verticalmente
                                ui.allocate_ui_with_layout(
                                    egui::vec2(final_text_width, toast_height),
                                    egui::Layout::centered_and_justified(egui::Direction::TopDown),
                                    |ui| {
                                        ui.add(
                                            egui::Label::new(
                                                egui::RichText::new(&toast.message)
                                                    .size(13.0)
                                                    .color(text_color),
                                            ).wrap(true)
                                        );
                                    },
                                );

                                // Pulsante X - centrato verticalmente
                                ui.allocate_ui_with_layout(
                                    egui::vec2(CLOSE_WIDTH, toast_height),
                                    egui::Layout::centered_and_justified(egui::Direction::TopDown),
                                    |ui| {
                                        let close_btn = egui::Button::new(
                                            egui::RichText::new(egui_remixicon::icons::CLOSE_LINE)
                                                .size(14.0)
                                                .color(close_color),
                                        ).frame(false);

                                        if ui.add(close_btn).clicked() {
                                            to_remove.insert(toast.id);
                                        }
                                    },
                                );
                            });
                        });
                });

            y_offset += response.response.rect.height() + 8.0;
        }

        if !to_remove.is_empty() {
            self.state.toasts.retain(|t| !to_remove.contains(&t.id));
        }

        // ✅ IMPORTANTE: Richiedi repaint continuo se ci sono toast attivi o in animazione
        if !self.state.toasts.is_empty() {
            ctx.request_repaint();
        }
    }
}
