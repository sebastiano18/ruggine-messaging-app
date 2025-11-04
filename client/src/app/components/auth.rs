use crate::api;
use crate::models::{LoginState, UiEvent};
use crate::state::AppState;
use eframe::egui::{self, TextEdit, RichText};
use egui::Id;

#[derive(Debug, Clone, Copy, PartialEq)]
enum AuthView {
    Login,
    Register,
}

pub fn panel(ui: &mut egui::Ui, s: &mut AppState) {
    // Prendi tutto lo spazio disponibile
    let available_rect = ui.available_rect_before_wrap();

    // Usa il colore di sfondo del tema corrente
    let bg_color = ui.visuals().window_fill();
    ui.painter().rect_filled(available_rect, 0.0, bg_color);

    // Determina quale vista mostrare - usa un campo separato, non LoginState
    // Usa la memoria dello stato UI per mantenere la vista corrente
    let current_view = ui.data_mut(|d| {
        d.get_temp::<AuthView>(egui::Id::new("auth_view"))
            .unwrap_or(AuthView::Login)
    });

    // Centra il contenuto
    egui::Area::new(Id::from("login_area"))
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ui.ctx(), |ui| {
            ui.vertical_centered(|ui| {
                if s.token.is_some() {
                    show_logged_in_view(ui, s);
                } else {
                    match current_view {
                        AuthView::Login => show_login_view(ui, s),
                        AuthView::Register => show_register_view(ui, s),
                    }
                }
            });
        });
}

fn show_logged_in_view(ui: &mut egui::Ui, s: &mut AppState) {
    ui.label(
        RichText::new(format!("{} Autenticato", egui_remixicon::icons::CHECKBOX_CIRCLE_LINE))
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
                ui.label(RichText::new(egui_remixicon::icons::USER_LINE).size(24.0));
                ui.add_space(12.0);
                ui.label(RichText::new(&s.username).size(24.0).strong());
            });

            ui.add_space(12.0);

            if let Some(user_id) = s.user_id {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(egui_remixicon::icons::FINGERPRINT_LINE).size(20.0));
                    ui.add_space(12.0);
                    ui.label(RichText::new(&user_id.to_string()[..8]).code().size(16.0));
                });
                ui.add_space(8.0);
            }

            if s.user_sequence_confirmed > 0 {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(egui_remixicon::icons::LINE_CHART_LINE).size(20.0));
                    ui.add_space(12.0);
                    ui.label(RichText::new(format!("Sequenza: #{}", s.user_sequence_confirmed)).size(16.0));
                });
                ui.add_space(8.0);
            }

            if !s.conversation_sequences.is_empty() {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(egui_remixicon::icons::MESSAGE_3_LINE).size(20.0));
                    ui.add_space(12.0);
                    ui.label(RichText::new(format!("{} conversazioni attive", s.conversation_sequences.len())).size(16.0));
                });
            }
        });

    ui.add_space(30.0);

    let logout_button = egui::Button::new(RichText::new(format!("{} Logout", egui_remixicon::icons::LOGOUT_BOX_R_LINE)).size(18.0))
        .fill(egui::Color32::from_rgb(180, 40, 40))
        .min_size(egui::vec2(250.0, 50.0));

    if ui.add(logout_button).clicked() {
        // Pulisci le credenziali prima del logout
        s.username.clear();
        s.password.clear();
        s.password_confirm.clear();

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
    let is_busy = matches!(s.login_state, LoginState::LoggingIn);

    ui.label(
        RichText::new(format!("{} Ruggine Chat", egui_remixicon::icons::CHAT_SMILE_FILL))
            .size(56.0)
            .strong()
            .color(egui::Color32::from_rgb(200, 100, 40))
    );

    ui.add_space(20.0);

    // Mostra messaggio di errore/info se presente
    if let Some(ref msg) = s.auth_message {
        let color = if s.auth_message_is_error {
            egui::Color32::from_rgb(220, 60, 60)  // Rosso per errori
        } else {
            egui::Color32::from_rgb(60, 180, 60)  // Verde per info
        };

        ui.vertical_centered(|ui| {
            ui.label(
                RichText::new(msg)
                    .size(16.0)
                    .color(color)
            );
        });
        ui.add_space(10.0);
    }

    ui.add_space(20.0);

    egui::Frame::none()
        .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
        .inner_margin(30.0)
        .rounding(8.0)
        .show(ui, |ui| {
            ui.set_min_width(500.0);
            ui.set_max_width(500.0);

            // Username
            ui.horizontal(|ui| {
                ui.add_space(20.0);
                ui.label(RichText::new(egui_remixicon::icons::USER_LINE).size(24.0));
                ui.add_space(15.0);
                ui.vertical(|ui| {
                    ui.label(RichText::new("Username").size(14.0).weak());
                    ui.add(
                        TextEdit::singleline(&mut s.username)
                            .desired_width(400.0)
                            .hint_text("Il tuo username")
                    );
                });
            });

            ui.add_space(24.0);

            // Password
            ui.horizontal(|ui| {
                ui.add_space(20.0);
                ui.label(RichText::new(egui_remixicon::icons::LOCK_PASSWORD_LINE).size(24.0));
                ui.add_space(15.0);
                ui.vertical(|ui| {
                    ui.label(RichText::new("Password").size(14.0).weak());
                    let password_response = ui.add(
                        TextEdit::singleline(&mut s.password)
                            .password(true)
                            .desired_width(400.0)
                            .hint_text("La tua password")
                    );

                    // Enter nel campo password = login (solo se campi pieni E il campo ha ancora focus)
                    if password_response.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        if !s.username.is_empty() && !s.password.is_empty() && !is_busy {
                            start_login(s);
                        }
                    }
                });
            });

            ui.add_space(35.0);

            // Bottone Login
            ui.vertical_centered(|ui| {
                let can_login = !s.username.is_empty() && !s.password.is_empty() && !is_busy;

                let login_button = egui::Button::new(
                    RichText::new(format!("{} Login", egui_remixicon::icons::LOGIN_BOX_LINE))
                        .size(18.0)
                )
                    .fill(egui::Color32::from_rgb(200, 100, 40))
                    .min_size(egui::vec2(400.0, 50.0));

                ui.add_enabled_ui(can_login, |ui| {
                    if ui.add(login_button).clicked() {
                        start_login(s);
                    }
                });
            });

            ui.add_space(20.0);

            // Link registrazione
            ui.vertical_centered(|ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Non hai un account?").size(14.0).weak());
                    ui.add_space(8.0);

                    let register_link = ui.link(
                        RichText::new(format!("{} Registrati", egui_remixicon::icons::USER_ADD_LINE))
                            .size(14.0)
                            .color(egui::Color32::from_rgb(200, 100, 40))
                    );

                    if register_link.clicked() {
                        s.password.clear();
                        s.clear_auth_message();
                        // Salva la vista nella memoria UI
                        ui.data_mut(|d| d.insert_temp(egui::Id::new("auth_view"), AuthView::Register));
                    }
                });
            });
        });

    ui.add_space(25.0);

    // Spinner centrato sotto il form
    if matches!(s.login_state, LoginState::LoggingIn) {
        ui.vertical_centered(|ui| {
            ui.add(egui::Spinner::new());
            ui.label(
                RichText::new("Autenticazione in corso...")
                    .size(16.0)
                    .color(egui::Color32::from_rgb(200, 100, 40))
            );
        });
    }
}

fn show_register_view(ui: &mut egui::Ui, s: &mut AppState) {
    let is_busy = matches!(s.login_state, LoginState::Registering);

    ui.label(
        RichText::new(format!("{} Crea Account", egui_remixicon::icons::USER_ADD_LINE))
            .size(56.0)
            .strong()
            .color(egui::Color32::from_rgb(200, 100, 40))
    );

    ui.add_space(20.0);

    // Mostra messaggio di errore/info se presente
    if let Some(ref msg) = s.auth_message {
        let color = if s.auth_message_is_error {
            egui::Color32::from_rgb(220, 60, 60)  // Rosso per errori
        } else {
            egui::Color32::from_rgb(60, 180, 60)  // Verde per info
        };

        ui.vertical_centered(|ui| {
            ui.label(
                RichText::new(msg)
                    .size(16.0)
                    .color(color)
            );
        });
        ui.add_space(10.0);
    }

    ui.add_space(20.0);

    egui::Frame::none()
        .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
        .inner_margin(30.0)
        .rounding(8.0)
        .show(ui, |ui| {
            ui.set_min_width(500.0);
            ui.set_max_width(500.0);

            // Username
            ui.horizontal(|ui| {
                ui.add_space(20.0);
                ui.label(RichText::new(egui_remixicon::icons::USER_LINE).size(24.0));
                ui.add_space(15.0);
                ui.vertical(|ui| {
                    ui.label(RichText::new("Username").size(14.0).weak());
                    ui.add(
                        TextEdit::singleline(&mut s.username)
                            .desired_width(400.0)
                            .hint_text("Scegli un username")
                    );
                });
            });

            ui.add_space(24.0);

            // Password
            ui.horizontal(|ui| {
                ui.add_space(20.0);
                ui.label(RichText::new(egui_remixicon::icons::LOCK_PASSWORD_LINE).size(24.0));
                ui.add_space(15.0);
                ui.vertical(|ui| {
                    ui.label(RichText::new("Password").size(14.0).weak());
                    ui.add(
                        TextEdit::singleline(&mut s.password)
                            .password(true)
                            .desired_width(400.0)
                            .hint_text("Scegli una password sicura")
                    );
                });
            });

            ui.add_space(24.0);

            // Conferma Password
            ui.horizontal(|ui| {
                ui.add_space(20.0);
                ui.label(RichText::new(egui_remixicon::icons::LOCK_PASSWORD_LINE).size(24.0));
                ui.add_space(15.0);
                ui.vertical(|ui| {
                    ui.label(RichText::new("Conferma Password").size(14.0).weak());

                    // Usa un campo temporaneo per la conferma password
                    let mut password_confirm = s.password_confirm.clone();
                    let response = ui.add(
                        TextEdit::singleline(&mut password_confirm)
                            .password(true)
                            .desired_width(400.0)
                            .hint_text("Ripeti la password")
                    );
                    s.password_confirm = password_confirm;

                    // Mostra validazione
                    if !s.password_confirm.is_empty() {
                        ui.add_space(4.0);
                        if s.password == s.password_confirm {
                            ui.label(
                                RichText::new(format!("{} Le password corrispondono", egui_remixicon::icons::CHECKBOX_CIRCLE_LINE))
                                    .size(12.0)
                                    .color(egui::Color32::from_rgb(40, 180, 80))
                            );
                        } else {
                            ui.label(
                                RichText::new(format!("{} Le password non corrispondono", egui_remixicon::icons::CLOSE_CIRCLE_LINE))
                                    .size(12.0)
                                    .color(egui::Color32::from_rgb(180, 40, 40))
                            );
                        }
                    }
                });
            });

            ui.add_space(35.0);

            // Bottone Registrati
            ui.vertical_centered(|ui| {
                let passwords_match = s.password == s.password_confirm && !s.password.is_empty();
                let can_register = !s.username.is_empty() && passwords_match && !is_busy;

                let register_button = egui::Button::new(
                    RichText::new(format!("{} Crea Account", egui_remixicon::icons::USER_ADD_LINE))
                        .size(18.0)
                )
                    .fill(egui::Color32::from_rgb(200, 100, 40))
                    .min_size(egui::vec2(400.0, 50.0));

                ui.add_enabled_ui(can_register, |ui| {
                    if ui.add(register_button).clicked() {
                        start_registration(s);
                    }
                });
            });

            ui.add_space(20.0);

            // Link torna al login
            ui.vertical_centered(|ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Hai già un account?").size(14.0).weak());
                    ui.add_space(8.0);

                    let login_link = ui.link(
                        RichText::new(format!("{} Torna al Login", egui_remixicon::icons::LOGIN_BOX_LINE))
                            .size(14.0)
                            .color(egui::Color32::from_rgb(200, 100, 40))
                    );

                    if login_link.clicked() {
                        s.password.clear();
                        s.password_confirm.clear();
                        s.clear_auth_message();
                        // Torna alla vista login
                        ui.data_mut(|d| d.insert_temp(egui::Id::new("auth_view"), AuthView::Login));
                    }
                });
            });
        });

    ui.add_space(25.0);

    // Spinner centrato sotto il form
    if is_busy {
        ui.vertical_centered(|ui| {
            ui.add(egui::Spinner::new());
            ui.label(
                RichText::new("Creazione account in corso...")
                    .size(16.0)
                    .color(egui::Color32::from_rgb(200, 100, 40))
            );
        });
    }
}

fn start_login(s: &mut AppState) {
    // Pulisci eventuali messaggi precedenti
    s.clear_auth_message();

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

                // Determina il messaggio di errore basato sul tipo
                let error_msg = if e.to_string().contains("401") || e.to_string().contains("Unauthorized") {
                    format!("{} Nome utente o password errati", egui_remixicon::icons::CLOSE_CIRCLE_LINE)
                } else {
                    format!("{} Errore di connessione al server", egui_remixicon::icons::CLOSE_CIRCLE_LINE)
                };

                let _ = tx.send(UiEvent::Error(error_msg));
                let _ = tx.send(UiEvent::LoggedOut);
            }
        }
    });
}

fn start_registration(s: &mut AppState) {
    // Pulisci eventuali messaggi precedenti
    s.clear_auth_message();

    let base = s.base.clone();
    let u = s.username.clone();
    let p = s.password.clone();
    let tx = s.ui_tx.clone();

    let _ = tx.send(UiEvent::RegisterStarted);

    s.rt.spawn(async move {
        tracing::debug!("Starting registration for user: {}", u);
        match api::auth::register(&base, &u, &p).await {
            Ok(_) => {
                let _ = tx.send(UiEvent::Info(format!("{} Registrazione completata, login automatico...", egui_remixicon::icons::CHECKBOX_CIRCLE_LINE)));
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
                        let _ = tx.send(UiEvent::Info(format!("{} Account creato ma login automatico fallito, prova a fare login manualmente", egui_remixicon::icons::CLOSE_CIRCLE_LINE)));
                        let _ = tx.send(UiEvent::LoggedOut);
                    }
                }
            }
            Err(e) => {
                tracing::error!("Registration failed: {}", e);

                // Determina il messaggio di errore basato sul tipo
                let error_msg = if e.to_string().contains("409") || e.to_string().contains("Conflict") {
                    format!("{} Nome utente già registrato, scegline un altro", egui_remixicon::icons::CLOSE_CIRCLE_LINE)
                } else if e.to_string().contains("400") || e.to_string().contains("Bad Request") {
                    format!("{} Nome utente o password non validi", egui_remixicon::icons::CLOSE_CIRCLE_LINE)
                } else {
                    format!("{} Errore durante la registrazione, riprova", egui_remixicon::icons::CLOSE_CIRCLE_LINE)
                };

                let _ = tx.send(UiEvent::Error(error_msg));
                let _ = tx.send(UiEvent::LoggedOut);
            }
        }
    });
}