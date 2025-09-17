use crate::api;
use crate::models::{LoginState, UiEvent}; // Make sure LoginResp is accessible through api::auth
use crate::state::AppState;
use crate::style::apply_azure_theme;
use eframe::egui::{self, TextEdit};

pub fn panel(ui: &mut egui::Ui, s: &mut AppState) {
    apply_azure_theme(ui);

    ui.heading("Accesso");
    ui.separator();

    // === Caso: già loggato -> mostra solo Logout ===
    if let Some(tok) = s.token.as_ref() {
        ui.label(format!("Loggato come: {}", s.username));
        if let Some(user_id) = s.user_id {
            ui.label(format!("User ID: {}", user_id));
        }

        // Mostra sequence corrente
        if s.last_sequence_received > 0 {
            ui.label(format!("Sequenza: #{}", s.last_sequence_received));
        }

        ui.add_space(8.0);

        if ui.button("Logout").clicked() {
            let base = s.base.clone();
            let token = tok.clone();
            let tx = s.ui_tx.clone();
            s.rt.spawn(async move {
                if let Err(e) = api::auth::logout(&base, &token).await {
                    let _ = tx.send(UiEvent::Info(format!("logout note: {e}")));
                }
                let _ = tx.send(UiEvent::LoggedOut);
            });
        }
        return;
    }

    // === Caso: NON loggato -> mostra form di login/registrazione ===
    ui.label("Server base URL");
    ui.text_edit_singleline(&mut s.base);
    ui.add_space(8.0);

    ui.label("Username");
    ui.text_edit_singleline(&mut s.username);

    ui.label("Password");
    ui.add(TextEdit::singleline(&mut s.password).password(true));
    ui.add_space(8.0);

    // Disabilita i pulsanti se stiamo facendo login/registrazione
    let is_busy = matches!(
        s.login_state,
        LoginState::LoggingIn | LoginState::Registering
    );

    ui.horizontal(|ui| {
        // Pulsante di Login
        ui.add_enabled_ui(!is_busy, |ui| {
            if ui.button("Login").clicked() {
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

                            // Send all 3 parameters
                            let _ = tx.send(UiEvent::Logged(
                                login_resp.token,
                                login_resp.user_id,
                                login_resp.last_sequence
                            ));
                        }
                        Err(e) => {
                            tracing::error!("Login failed: {}", e);
                            let _ = tx.send(UiEvent::Error(format!("Login failed: {}", e)));
                        }
                    }
                });
            }
        });

        // Pulsante di Registrazione
        ui.add_enabled_ui(!is_busy, |ui| {
            if ui.button("Register").clicked() {
                let base = s.base.clone();
                let u = s.username.clone();
                let p = s.password.clone();
                let tx = s.ui_tx.clone();

                let _ = tx.send(UiEvent::RegisterStarted);

                s.rt.spawn(async move {
                    tracing::debug!("Starting registration for user: {}", u);
                    match api::auth::register(&base, &u, &p).await {
                        Ok(_) => {
                            let _ = tx.send(UiEvent::Info(
                                "Registration completed, logging in...".into()
                            ));
                            match api::auth::login(&base, &u, &p).await {
                                Ok(login_resp) => {
                                    tracing::info!(
                                        "Post-registration login successful - token: {}, user_id: {}, last_sequence: {}",
                                        login_resp.token, login_resp.user_id, login_resp.last_sequence
                                    );

                                    // Send all 3 parameters for registration + login
                                    let _ = tx.send(UiEvent::Logged(
                                        login_resp.token,
                                        login_resp.user_id,
                                        login_resp.last_sequence
                                    ));
                                }
                                Err(e) => {
                                    tracing::error!("Login after registration failed: {}", e);
                                    let _ = tx.send(UiEvent::Error(
                                        format!("Login failed after registration: {}", e)
                                    ));
                                }
                            }
                        }
                        Err(e) => {
                            tracing::error!("Registration failed: {}", e);
                            let _ = tx.send(UiEvent::Error(format!("Registration failed: {}", e)));
                        }
                    }
                });
            }
        });
    });

    // Mostra stato corrente
    match s.login_state {
        LoginState::LoggingIn => {
            ui.add_space(8.0);
            ui.spinner();
            ui.label("Effettuando login...");
        }
        LoginState::Registering => {
            ui.add_space(8.0);
            ui.spinner();
            ui.label("Registrando utente...");
        }
        _ => {}
    }
}
