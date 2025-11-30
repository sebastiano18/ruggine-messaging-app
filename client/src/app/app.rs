use std::time::Duration;
use std::sync::Arc;
use crate::app::events::sequence_handler::SequenceHandler;
use crate::app::ws_manager::ws_manager::WebSocketManager;
use crate::models::{Page, WsStatus};
use crate::state::AppState;
use eframe::egui;
use crate::ui::{components, pages};
use crate::ui::layout::header::HeaderManager;
use crate::ui::layout::sidebar::SidebarManager;
use crate::ui::modals::account::AccountModal;
use crate::ui::modals::delete_account::DeleteAccountModal;
use crate::ui::components::toast_renderer::ToastRenderer;

pub struct App {
    state: AppState,
    ws_manager: WebSocketManager,
    sidebar_manager: SidebarManager,
    header_manager: HeaderManager,
    account_modal: AccountModal,
    delete_account_modal: DeleteAccountModal,
    toast_renderer: ToastRenderer,
}

impl App {
    pub fn new(waker: Arc<dyn Fn() + Send + Sync>) -> Self {
        Self {
            state: AppState::new(waker),
            ws_manager: WebSocketManager::new(),
            sidebar_manager: SidebarManager::new(),
            header_manager: HeaderManager::new(),
            account_modal: AccountModal::new(),
            delete_account_modal: DeleteAccountModal::new(),
            toast_renderer: ToastRenderer::new(),
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Lifecycle management
        self.ws_manager.ensure_ws_lifecycle(&mut self.state);

        //  drain_events()  verrà chiamato perché waker sveglia egui quando arrivano messaggi
        self.state.drain_events();
        self.state.prune_expired_toasts(std::time::Duration::from_secs(5));

        // Header (sempre visibile)
        self.header_manager.show_header(ctx, &mut self.state);

        // Layout principale basato su autenticazione
        if self.state.token.is_none() {
            self.show_auth_layout(ctx);
        } else {
            self.show_main_layout(ctx);
        }

        // Toast notifications (solo se autenticato e non in auth page)
        if self.state.token.is_some() && !matches!(self.state.page, Page::Auth) {
            self.toast_renderer.render(ctx, &mut self.state);
        }

        // Modal account
        self.account_modal.show_modal(ctx, &mut self.state);

        // Modal conferma eliminazione account
        self.delete_account_modal.show_modal(ctx, &mut self.state);

        // Cleanup periodico
        self.periodic_cleanup();
    }
}

impl App {
    fn show_auth_layout(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            pages::auth::show(ui, &mut self.state);
        });
    }

    fn show_main_layout(&mut self, ctx: &egui::Context) {
        // Sidebar a sinistra
        egui::SidePanel::left("sidebar")
            .resizable(true)
            .default_width(300.0)
            .min_width(200.0)   // Larghezza minima
            .max_width(500.0)   // Larghezza massima
            .show(ctx, |ui| {
                self.sidebar_manager.show_sidebar(ui, &mut self.state);
            });

        // Pannello centrale
        egui::CentralPanel::default().show(ctx, |ui| {
            match self.state.page {
                Page::Conversations => {
                    self.show_welcome_screen(ui);
                }
                Page::Chat => {
                    pages::chat::show(ui, &mut self.state);
                }
                Page::Auth => {
                    // Non dovrebbe mai arrivare qui, ma per sicurezza
                    pages::auth::show(ui, &mut self.state);
                }
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
}