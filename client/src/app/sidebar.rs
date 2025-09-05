use eframe::egui;
use egui::{Align, Layout};
use crate::app::sidebar_components::sidebar_account::AccountSidebar;
use crate::app::sidebar_components::sidebar_conversation::ConversationsSidebar;
use crate::app::sidebar_components::sidebar_group_management::GroupManagementSidebar;
use crate::models::Page;
use crate::state::AppState;

pub struct SidebarManager {
    account_sidebar: AccountSidebar,
    conversations_sidebar: ConversationsSidebar,
    group_management_sidebar: GroupManagementSidebar,
}

impl SidebarManager {
    pub fn new() -> Self {
        Self {
            account_sidebar: AccountSidebar::new(),
            conversations_sidebar: ConversationsSidebar::new(),
            group_management_sidebar: GroupManagementSidebar::new(),
        }
    }

    pub fn show_sidebar(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        ui.vertical(|ui| {
            // Header della sidebar con tabs
            self.show_sidebar_tabs(ui, state);

            ui.separator();
            ui.add_space(4.0);

            // Contenuto della sidebar basato sulla tab selezionata
            match state.page {
                Page::Auth => {
                    self.account_sidebar.show(ui, state);
                },
                Page::Conversations | Page::Chat => {
                    self.conversations_sidebar.show(ui, state);
                },
                Page::GroupManagement => {
                    self.group_management_sidebar.show(ui, state);
                }
            }
        });
    }

    fn show_sidebar_tabs(&self, ui: &mut egui::Ui, state: &mut AppState) {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut state.page, Page::Conversations, "💬 Chat");
            ui.selectable_value(&mut state.page, Page::GroupManagement, "🔧 Gestione");
            ui.selectable_value(&mut state.page, Page::Auth, "⚙️ Account");
        });
    }
}