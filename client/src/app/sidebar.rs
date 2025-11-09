use eframe::egui;
use crate::app::sidebar_components::sidebar_account::AccountSidebar;
use crate::app::sidebar_components::sidebar_conversation::ConversationsSidebar;
use crate::models::Page;
use crate::state::AppState;

pub struct SidebarManager {
    account_sidebar: AccountSidebar,
    conversations_sidebar: ConversationsSidebar,
}

impl SidebarManager {
    pub fn new() -> Self {
        Self {
            account_sidebar: AccountSidebar::new(),
            conversations_sidebar: ConversationsSidebar::new(),
        }
    }

    pub fn show_sidebar(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        ui.vertical(|ui| {
            // Contenuto della sidebar basato sulla pagina
            match state.page {
                Page::Auth => {
                    self.account_sidebar.show(ui, state);
                },
                Page::Conversations | Page::Chat => {
                    self.conversations_sidebar.show(ui, state);
                },
            }
        });
    }
}