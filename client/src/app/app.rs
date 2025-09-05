use eframe::egui;
use crate::app::components;


use crate::app::header::HeaderManager;
use crate::app::ws::WebSocketManager;
use crate::app::sidebar::SidebarManager;
use crate::models::Page;
use crate::state::AppState;

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
        // 1) drena eventi (REST/WS) che aggiornano lo stato
        self.state.drain_events();

        // 2) garantisce il ciclo di vita del WS (connetti se serve)
        self.ws_manager.ensure_ws_lifecycle(&mut self.state);

        // 3) UI - Header sempre presente
        self.header_manager.show_header(ctx, &mut self.state);

        // 4) Layout principale
        if self.state.token.is_none() {
            self.show_auth_layout(ctx);
        } else {
            self.show_main_layout(ctx);
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
    fn show_auth_layout(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            // Centra il pannello di login
            ui.vertical_centered(|ui| {
                ui.add_space(100.0);
                ui.group(|ui| {
                    ui.set_max_width(400.0);
                    components::auth::panel(ui, &mut self.state);
                });
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
                _ => {
                    // Per altre pagine o stati di fallback
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