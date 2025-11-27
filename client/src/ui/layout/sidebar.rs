use eframe::egui;
use crate::models::Page;
use crate::state::AppState;
use crate::ui::sidebar::conversations::ConversationsSidebar;

pub struct SidebarManager {
    conversations_sidebar: ConversationsSidebar,
}

impl SidebarManager {
    pub fn new() -> Self {
        Self {
            conversations_sidebar: ConversationsSidebar::new(),
        }
    }

    pub fn show_sidebar(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        ui.vertical(|ui| {
            match state.page {
                Page::Auth => {
                    // Sulla pagina Auth non mostriamo sidebar
                },
                Page::Conversations | Page::Chat => {
                    self.conversations_sidebar.show(ui, state);
                },
            }
        });
    }
}
