use eframe::egui;
use egui::{Align, Layout};
use crate::state::{AppState, Page};
use crate::ui;

pub struct App { state: AppState }

impl App {
    pub fn new() -> Self { Self { state: AppState::new() } }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // drena eventi (REST/WS)
        self.state.drain_events();

        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.heading("Ruggine – Chat");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.selectable_value(&mut self.state.page, Page::Chat, "Chat");
                    ui.selectable_value(&mut self.state.page, Page::Groups, "Gruppi");
                    ui.selectable_value(&mut self.state.page, Page::Auth, "Auth");
                });
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            match self.state.page {
                Page::Auth   => ui::auth::panel(ui, &mut self.state),
                Page::Groups => ui::groups::panel(ui, &mut self.state),
                Page::Chat   => ui::chat::panel(ui, &mut self.state),
            }
        });

        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }
}
