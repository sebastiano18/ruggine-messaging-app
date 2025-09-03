use eframe::egui;
use egui::{Align, Layout};
use crate::state::{AppState, Page, UiEvent, WsStatus};
use crate::ui;

pub struct App { state: AppState }

impl App {
    pub fn new() -> Self { Self { state: AppState::new() } }

    fn ensure_ws_lifecycle(&mut self) {
        // Se l'utente non è autenticato, nessun WS
        if self.state.token.is_none() {
            // opzionale: potresti impostare Disconnected qui
            self.state.ws_status = WsStatus::Disconnected;
            return;
        }

        // Se l’utente ha richiesto riconnessione manuale, forziamo
        if self.state.request_ws_reconnect && self.state.ws_status != WsStatus::Connecting {
            self.state.request_ws_reconnect = false;
            self.state.ws_status = WsStatus::Disconnected; // forza ramo sotto
        }
        
        

        match self.state.ws_status {
            WsStatus::Disconnected => {
                // avvia connessione WS globale
                let base = self.state.base.clone();
                let token = self.state.token.clone().unwrap();
                let tx = self.state.ui_tx.clone();

                self.state.ws_status = WsStatus::Connecting;

                // NB: net::ws::connect(base, token) e subscribe(&mut ws) SENZA conversation_id
                self.state.rt.spawn(async move {
                    match crate::net::ws::connect(&base, &token).await {
                        Ok(mut ws) => {
                            if let Err(e) = crate::net::ws::subscribe(&mut ws).await {
                                let _ = tx.send(UiEvent::WsError(format!("WS subscribe fallito: {e}")));
                                return;
                            }
                            let _ = tx.send(UiEvent::WsConnected);
                            let tx_reader = tx.clone(); // <-- clone per la closure

                            let ctrl = crate::net::ws::spawn_reader_and_pinger(ws, move |msg| {
                                let _ = tx_reader.send(UiEvent::WsIncoming(msg));
                            });
                            
                            let _ = tx.send(UiEvent::WsControlReady(ctrl));
                            
                        }
                        Err(e) => {
                            let _ = tx.send(UiEvent::WsError(format!("WS connect fallito: {e}")));
                        }
                    }
                });
            }
            WsStatus::Connecting | WsStatus::Connected => {
                // nulla da fare
            }
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 1) drena eventi (REST/WS) che aggiornano lo stato
        self.state.drain_events();

        // 2) garantisce il ciclo di vita del WS (connetti se serve)
        self.ensure_ws_lifecycle();

        // 3) UI
        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.heading("Ruggine");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.selectable_value(&mut self.state.page, Page::Chat, "Chat");
                    ui.selectable_value(&mut self.state.page, Page::Conversations, "Gruppi");
                    ui.selectable_value(&mut self.state.page, Page::Auth, "Auth");
                });
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            match self.state.page {
                Page::Auth           => ui::auth::panel(ui, &mut self.state),
                Page::Conversations  => ui::conversation::panel(ui, &mut self.state),
                Page::Chat           => ui::chat::panel(ui, &mut self.state),
            }
        });

        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if let Some(ctrl) = self.state.ws_ctrl.take() {
            let _ = ctrl.shutdown.send(());
        }
    }
}
