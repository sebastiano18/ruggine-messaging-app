use crate::api;
use crate::models::{LoginState, UiEvent};
use crate::state::AppState;
use eframe::egui::{self, TextEdit, RichText};
use egui::Id;

pub fn panel(ui: &mut egui::Ui, s: &mut AppState) {
    // Prendi tutto lo spazio disponibile
    let available_rect = ui.available_rect_before_wrap();

    // Usa il colore di sfondo del tema corrente
    let bg_color = ui.visuals().window_fill();
    ui.painter().rect_filled(available_rect, 0.0, bg_color);

    // Centra il contenuto
    egui::Area::new(Id::from("login_area"))
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ui.ctx(), |ui| {
            ui.vertical_centered(|ui| {
                if s.token.is_some() {
                    show_logged_in_view(ui, s);
                } else {
                    show_login_view(ui, s);
                }
            });
        });
}

fn show_logged_in_view(ui: &mut egui::Ui, s: &mut AppState) {
    ui.label(
        RichText::new("✓ Autenticato")
            .size(42.0)
            .strong()
            .color(egui::Color32::from_rgb(200, 100, 40))
    );
    ui.add_space(30.0);

    egui::Frame::none()
        .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
        .inner_margin(20.0)
        .rounding(8.0)
        .show(ui, |ui| {
            ui.set_min_width(450.0);

            ui.horizontal(|ui| {
                ui.label(RichText::new("👤").size(24.0));
                ui.add_space(12.0);
                ui.label(RichText::new(&s.username).size(24.0).strong());
            });

            ui.add_space(12.0);

            if let Some(user_id) = s.user_id {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("🆔").size(20.0));
                    ui.add_space(12.0);
                    ui.label(RichText::new(&user_id.to_string()[..8]).code().size(16.0));
                });
                ui.add_space(8.0);
            }

            if s.user_sequence_confirmed > 0 {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("📊").size(20.0));
                    ui.add_space(12.0);
                    ui.label(RichText::new(format!("Sequenza: #{}", s.user_sequence_confirmed)).size(16.0));
                });
                ui.add_space(8.0);
            }

            if !s.conversation_sequences.is_empty() {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("💬").size(20.0));
                    ui.add_space(12.0);
                    ui.label(RichText::new(format!("{} conversazioni attive", s.conversation_sequences.len())).size(16.0));
                });
            }
        });

    ui.add_space(30.0);

    let logout_button = egui::Button::new(RichText::new("🚪 Logout").size(18.0))
        .fill(egui::Color32::from_rgb(180, 40, 40))
        .min_size(egui::vec2(250.0, 50.0));

    if ui.add(logout_button).clicked() {
        let base = s.base.clone();
        let token = s.token.clone().unwrap();
        let tx = s.ui_tx.clone();
        s.rt.spawn(async move {
            if let Err(e) = api::auth::logout(&base, &token).await {
                let _ = tx.send(UiEvent::Info(format!("logout note: {e}")));
            }
            let _ = tx.send(UiEvent::LoggedOut);
        });
    }
}

fn show_login_view(ui: &mut egui::Ui, s: &mut AppState) {
    let is_busy = matches!(s.login_state, LoginState::LoggingIn | LoginState::Registering);

    ui.label(
        RichText::new("🔥 Rust Chat")
            .size(56.0)
            .strong()
            .color(egui::Color32::from_rgb(200, 100, 40))
    );

    ui.add_space(50.0);

    egui::Frame::none()
        .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
        .inner_margin(30.0)
        .rounding(8.0)
        .show(ui, |ui| {
            ui.set_min_width(500.0);
            ui.set_max_width(500.0);

            // Server
            ui.horizontal(|ui| {
                ui.add_space(20.0);
                ui.label(RichText::new("🌐").size(24.0));
                ui.add_space(15.0);
                ui.vertical(|ui| {
                    ui.label(RichText::new("Server").size(14.0).weak());
                    ui.add(
                        TextEdit::singleline(&mut s.base)
                            .desired_width(380.0)
                            .font(egui::TextStyle::Monospace)
                            .hint_text("http://localhost:8080")
                    );
                });
            });

            ui.add_space(24.0);

            // Username
            ui.horizontal(|ui| {
                ui.add_space(20.0);
                ui.label(RichText::new("👤").size(24.0));
                ui.add_space(15.0);
                ui.vertical(|ui| {
                    ui.label(RichText::new("Username").size(14.0).weak());
                    ui.add(
                        TextEdit::singleline(&mut s.username)
                            .desired_width(380.0)
                            .hint_text("Il tuo username")
                    );
                });
            });

            ui.add_space(24.0);

            // Password
            ui.horizontal(|ui| {
                ui.add_space(20.0);
                ui.label(RichText::new("🔒").size(24.0));
                ui.add_space(15.0);
                ui.vertical(|ui| {
                    ui.label(RichText::new("Password").size(14.0).weak());
                    ui.add(
                        TextEdit::singleline(&mut s.password)
                            .password(true)
                            .desired_width(380.0)
                            .hint_text("La tua password")
                    );
                });
            });

            ui.add_space(35.0);

            // Bottoni
            ui.horizontal(|ui| {
                ui.add_space(60.0);

                let login_button = egui::Button::new(RichText::new("🔓 Login").size(18.0))
                    .fill(egui::Color32::from_rgb(200, 100, 40))
                    .min_size(egui::vec2(170.0, 50.0));

                ui.add_enabled_ui(!is_busy, |ui| {
                    if ui.add(login_button).clicked() {
                        start_login(s);
                    }
                });

                ui.add_space(20.0);

                let register_button = egui::Button::new(RichText::new("✨ Registrati").size(18.0))
                    .min_size(egui::vec2(170.0, 50.0));

                ui.add_enabled_ui(!is_busy, |ui| {
                    if ui.add(register_button).clicked() {
                        start_registration(s);
                    }
                });
            });
        });

    ui.add_space(30.0);

    match s.login_state {
        LoginState::LoggingIn => {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.add_space(8.0);
                ui.label(RichText::new("Autenticazione in corso...").size(16.0).color(egui::Color32::from_rgb(200, 100, 40)));
            });
        }
        LoginState::Registering => {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.add_space(8.0);
                ui.label(RichText::new("Creazione account in corso...").size(16.0).color(egui::Color32::from_rgb(200, 100, 40)));
            });
        }
        _ => {}
    }
}

fn start_login(s: &mut AppState) {
    let base = s.base.clone();
    let u = s.username.clone();
    let p = s.password.clone();
    let tx = s.ui_tx.clone();

    let _ = tx.send(UiEvent::LoginStarted);

    s.rt.spawn(async move {
        tracing::debug!("Starting login for user: {}", u);
        match api::auth::login(&base, &u, &p).await {
            Ok(login_resp) => {
                tracing::info!(
                    "Login successful - token: {}, user_id: {}, username: {}, last_sequence: {}",
                    login_resp.token, login_resp.user_id, login_resp.username, login_resp.last_sequence
                );

                let _ = tx.send(UiEvent::Logged(
                    login_resp.token,
                    login_resp.user_id,
                    login_resp.last_sequence
                ));
            }
            Err(e) => {
                tracing::error!("Login failed: {}", e);
                let _ = tx.send(UiEvent::Info(format!("❌ Login fallito: {}", e)));
                let _ = tx.send(UiEvent::LoggedOut);
            }
        }
    });
}

fn start_registration(s: &mut AppState) {
    let base = s.base.clone();
    let u = s.username.clone();
    let p = s.password.clone();
    let tx = s.ui_tx.clone();

    let _ = tx.send(UiEvent::RegisterStarted);

    s.rt.spawn(async move {
        tracing::debug!("Starting registration for user: {}", u);
        match api::auth::register(&base, &u, &p).await {
            Ok(_) => {
                let _ = tx.send(UiEvent::Info("✓ Registrazione completata, login automatico...".into()));
                match api::auth::login(&base, &u, &p).await {
                    Ok(login_resp) => {
                        tracing::info!(
                            "Post-registration login successful - token: {}, user_id: {}, last_sequence: {}",
                            login_resp.token, login_resp.user_id, login_resp.last_sequence
                        );

                        let _ = tx.send(UiEvent::Logged(
                            login_resp.token,
                            login_resp.user_id,
                            login_resp.last_sequence
                        ));
                    }
                    Err(e) => {
                        tracing::error!("Login after registration failed: {}", e);
                        let _ = tx.send(UiEvent::Info(format!("❌ Login dopo registrazione fallito: {}", e)));
                        let _ = tx.send(UiEvent::LoggedOut);
                    }
                }
            }
            Err(e) => {
                tracing::error!("Registration failed: {}", e);
                let _ = tx.send(UiEvent::Info(format!("❌ Registrazione fallita: {}", e)));
                let _ = tx.send(UiEvent::LoggedOut);
            }
        }
    });
}