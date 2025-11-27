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
}

impl App {
    pub fn new() -> Self {
        Self {
            state: AppState::new(),
            ws_manager: WebSocketManager::new(),
            sidebar_manager: SidebarManager::new(),
            header_manager: HeaderManager::new(),
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.ws_manager.ensure_ws_lifecycle(&mut self.state);
        self.state.drain_events();
        // Prune expired toast notifications
        self.state.prune_expired_toasts(std::time::Duration::from_secs(5));
        self.header_manager.show_header(ctx, &mut self.state);
        

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
        const SLIDE_IN_DURATION: f32 = 0.7;
        const SLIDE_IN_OFFSET: f32 = 40.0;
        const RIGHT_PADDING: f32 = 48.0;
        const TOAST_SPACING: f32 = 8.0;
        const MAX_WIDTH: f32 = 300.0;

        let screen_rect = ctx.screen_rect();
        let is_dark = ctx.style().visuals.dark_mode;

        let mut y_offset: f32 = 0.0;
        let mut to_remove = std::collections::HashSet::new();

        // Itera in ordine inverso: i toast più recenti appaiono in cima
        for toast in self.state.toasts.iter().rev() {
            let (bg, text_color, icon, icon_color) = match toast.kind {
                crate::state::ToastKind::Info => {
                    if is_dark {
                        (
                            egui::Color32::from_rgb(28, 100, 28),
                            egui::Color32::from_rgb(240, 255, 240),
                            egui_remixicon::icons::INFORMATION_LINE,
                            egui::Color32::from_rgb(120, 220, 120),
                        )
                    } else {
                        (
                            egui::Color32::from_rgb(225, 245, 225),
                            egui::Color32::from_rgb(20, 80, 20),
                            egui_remixicon::icons::INFORMATION_LINE,
                            egui::Color32::from_rgb(30, 130, 30),
                        )
                    }
                }
                crate::state::ToastKind::Error => {
                    if is_dark {
                        (
                            egui::Color32::from_rgb(120, 35, 35),
                            egui::Color32::from_rgb(255, 240, 240),
                            egui_remixicon::icons::ERROR_WARNING_LINE,
                            egui::Color32::from_rgb(255, 120, 120),
                        )
                    } else {
                        (
                            egui::Color32::from_rgb(200, 80, 80),
                            egui::Color32::from_rgb(255, 255, 255),
                            egui_remixicon::icons::ERROR_WARNING_LINE,
                            egui::Color32::from_rgb(255, 230, 230),
                        )
                    }
                }
            };

            let close_color = if is_dark {
                egui::Color32::from_rgba_premultiplied(220, 220, 220, 200)
            } else {
                egui::Color32::from_rgba_premultiplied(80, 80, 80, 180)
            };

            let ttl = toast.created.elapsed().as_secs_f32();

            // Animazione slide-in con easing
            let slide_t = ((ttl / SLIDE_IN_DURATION).clamp(0.0, 1.0));
            let slide_t = 1.0 - (1.0 - slide_t).powi(3); // ease-out cubic
            let slide_offset = SLIDE_IN_OFFSET * (1.0 - slide_t);
            let pos_y = TOP_MARGIN + y_offset - slide_offset;

            let area_id = egui::Id::new("toast").with(toast.id);
            let response = egui::Area::new(area_id)
                .order(egui::Order::Foreground)
                .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-RIGHT_PADDING, pos_y))
                .movable(false)
                .interactable(false)
                .show(ctx, |ui| {
                    // Frame semplice e solido
                    egui::Frame::none()
                        .fill(bg)
                        .stroke(egui::Stroke::new(1.0, egui::Color32::from_black_alpha(30)))
                        .rounding(8.0)
                        .inner_margin(egui::Margin::symmetric(12.0, 8.0))
                        .show(ui, |ui| {
                            ui.set_max_width(MAX_WIDTH);

                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 10.0;

                                // Icona
                                ui.label(
                                    egui::RichText::new(icon)
                                        .size(18.0)
                                        .color(icon_color)
                                );

                                // Testo - usa tutto lo spazio disponibile
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(&toast.message)
                                            .color(text_color)
                                    )
                                        .wrap(true)
                                );

                                // Spazio flessibile per spingere la X a destra
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    let close_response = ui.add(
                                        egui::Button::new(
                                            egui::RichText::new(egui_remixicon::icons::CLOSE_LINE)
                                                .size(14.0)
                                                .color(close_color)
                                        )
                                            .frame(false)
                                            .fill(egui::Color32::TRANSPARENT)
                                    );

                                    if close_response.clicked() {
                                        to_remove.insert(toast.id);
                                    }
                                });
                            });
                        });
                });

            y_offset += response.response.rect.height() + TOAST_SPACING;
        }

        // Rimuovi toast chiusi
        if !to_remove.is_empty() {
            self.state.toasts.retain(|t| !to_remove.contains(&t.id));
        }

        // Repaint solo durante l'animazione
        if !self.state.toasts.is_empty() {
            let has_animating = self.state.toasts.iter().any(|t| {
                t.created.elapsed().as_secs_f32() < SLIDE_IN_DURATION
            });

            if has_animating {
                ctx.request_repaint_after(std::time::Duration::from_millis(16));
            }
        }
    }
}
