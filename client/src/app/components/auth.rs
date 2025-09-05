use eframe::egui::{self, TextEdit};
use crate::models::{LoginState, UiEvent};
use crate::api;
use crate::state::AppState;
use crate::style::apply_azure_theme;

pub fn panel(ui: &mut egui::Ui, s: &mut AppState) {
    apply_azure_theme(ui);

    ui.heading("Accesso");
    ui.separator();

    // === Caso: già loggato -> mostra solo Logout ===
    if let Some(tok) = s.token.as_ref() {
        ui.label(format!("Loggato come: {}", s.username));
        if let Some(user_id) = s.user_id {
            ui.label(format!("User ID: {}", user_id)); // Uuid implementa Display
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
    let is_busy = matches!(s.login_state, LoginState::LoggingIn | LoginState::Registering);

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
                    println!("DEBUG: Iniziando login per utente: {}", u);
                    match api::auth::login(&base, &u, &p).await {
                        Ok(login_resp) => {
                            println!(
                                "DEBUG: Login risposta - token: {}, user_id: {}, username: {}",
                                login_resp.token, login_resp.user_id, login_resp.username
                            );
                            // user_id è Uuid
                            let _ = tx.send(UiEvent::Logged(login_resp.token, login_resp.user_id));
                        }
                        Err(e) => {
                            println!("DEBUG: Errore login: {}", e);
                            let _ = tx.send(UiEvent::Error(format!("login failed: {e}")));
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
                    println!("DEBUG: Iniziando registrazione per utente: {}", u);
                    match api::auth::register(&base, &u, &p).await {
                        Ok(_) => {
                            let _ = tx.send(UiEvent::Info(
                                "Registrazione completata, effettuando login...".into()
                            ));
                            match api::auth::login(&base, &u, &p).await {
                                Ok(login_resp) => {
                                    println!(
                                        "DEBUG: Post-registrazione login - token: {}, user_id: {}",
                                        login_resp.token, login_resp.user_id
                                    );
                                    let _ = tx.send(UiEvent::Logged(
                                        login_resp.token,
                                        login_resp.user_id, // Uuid
                                    ));
                                }
                                Err(e) => {
                                    println!("DEBUG: Errore login post-registrazione: {}", e);
                                    let _ = tx.send(UiEvent::Error(
                                        format!("login failed after registration: {e}")
                                    ));
                                }
                            }
                        }
                        Err(e) => {
                            println!("DEBUG: Errore registrazione: {}", e);
                            let _ = tx.send(UiEvent::Error(format!("register failed: {e}")));
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
