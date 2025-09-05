use eframe::egui;
use crate::models::Page;
use crate::state::AppState;

pub struct GroupManagementSidebar;

impl GroupManagementSidebar {
    pub fn new() -> Self {
        Self
    }

    pub fn show(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        ui.heading("🔧 Gestione");
        ui.separator();
        ui.add_space(8.0);

        ui.label("Usa il pannello principale per gestire gruppi e inviti");

        ui.add_space(12.0);

        if ui.button("← Torna alle Chat").clicked() {
            state.page = Page::Conversations;
        }
    }
}