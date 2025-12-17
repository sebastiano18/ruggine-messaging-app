use eframe::egui;
use crate::state::AppState;
use std::collections::HashSet;

/// Renderer per le notifiche toast
pub struct ToastRenderer;

impl ToastRenderer {
    pub fn new() -> Self {
        Self
    }

    /// Renderizza tutti i toast attivi
    pub fn render(&self, ctx: &egui::Context, state: &mut AppState) {
        const TOP_MARGIN: f32 = 100.0;
        const SLIDE_IN_DURATION: f32 = 0.7;
        const SLIDE_IN_OFFSET: f32 = 40.0;
        const RIGHT_PADDING: f32 = 48.0;
        const TOAST_SPACING: f32 = 8.0;
        const MAX_WIDTH: f32 = 200.0;

        let _screen_rect = ctx.screen_rect();
        let is_dark = ctx.style().visuals.dark_mode;

        let mut y_offset: f32 = 0.0;
        let mut to_remove = HashSet::new();

        // Itera in ordine inverso: i toast più recenti appaiono in cima
        for toast in state.toasts.iter().rev() {
            let (bg, text_color, icon, icon_color) = match toast.kind {
                crate::state::ToastKind::Info => {
                    if is_dark {
                        (
                            egui::Color32::from_rgb(28, 100, 28),
                            egui::Color32::from_rgb(240, 255, 240),
                            egui_remixicon::icons::INFORMATION_LINE,
                            egui::Color32::from_rgb(120, 220, 120),
                        )
                    } else {
                        (
                            egui::Color32::from_rgb(225, 245, 225),
                            egui::Color32::from_rgb(20, 80, 20),
                            egui_remixicon::icons::INFORMATION_LINE,
                            egui::Color32::from_rgb(30, 130, 30),
                        )
                    }
                }
                crate::state::ToastKind::Error => {
                    if is_dark {
                        (
                            egui::Color32::from_rgb(120, 35, 35),
                            egui::Color32::from_rgb(255, 240, 240),
                            egui_remixicon::icons::ERROR_WARNING_LINE,
                            egui::Color32::from_rgb(255, 120, 120),
                        )
                    } else {
                        (
                            egui::Color32::from_rgb(200, 80, 80),
                            egui::Color32::from_rgb(255, 255, 255),
                            egui_remixicon::icons::ERROR_WARNING_LINE,
                            egui::Color32::from_rgb(255, 230, 230),
                        )
                    }
                }
            };

            let close_color = if is_dark {
                egui::Color32::from_rgba_premultiplied(220, 220, 220, 200)
            } else {
                egui::Color32::from_rgba_premultiplied(80, 80, 80, 180)
            };

            let ttl = toast.created.elapsed().as_secs_f32();

            // Animazione slide-in con easing
            let slide_t = (ttl / SLIDE_IN_DURATION).clamp(0.0, 1.0);
            let slide_t = 1.0 - (1.0 - slide_t).powi(3); // ease-out cubic
            let slide_offset = SLIDE_IN_OFFSET * (1.0 - slide_t);
            let pos_y = TOP_MARGIN + y_offset - slide_offset;

            let area_id = egui::Id::new("toast").with(toast.id);
            let response = egui::Area::new(area_id)
                .order(egui::Order::Foreground)
                .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-RIGHT_PADDING, pos_y))
                .movable(false)
                .interactable(false)
                .show(ctx, |ui| {
                    // Frame semplice e solido
                    egui::Frame::none()
                        .fill(bg)
                        .stroke(egui::Stroke::new(1.0, egui::Color32::from_black_alpha(30)))
                        .rounding(8.0)
                        .inner_margin(egui::Margin::symmetric(12.0, 8.0))
                        .show(ui, |ui| {
                            ui.set_max_width(MAX_WIDTH);

                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 10.0;

                                // Icona
                                ui.label(
                                    egui::RichText::new(icon)
                                        .size(18.0)
                                        .color(icon_color)
                                );

                                // Testo - usa tutto lo spazio disponibile
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(&toast.message)
                                            .color(text_color)
                                    )
                                        .wrap(true)
                                );

                                // Spazio flessibile per spingere la X a destra
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    let close_response = ui.add(
                                        egui::Button::new(
                                            egui::RichText::new(egui_remixicon::icons::CLOSE_LINE)
                                                .size(14.0)
                                                .color(close_color)
                                        )
                                            .frame(false)
                                            .fill(egui::Color32::TRANSPARENT)
                                    );

                                    if close_response.clicked() {
                                        to_remove.insert(toast.id);
                                    }
                                });
                            });
                        });
                });

            y_offset += response.response.rect.height() + TOAST_SPACING;
        }

        // Rimuovi toast chiusi
        if !to_remove.is_empty() {
            state.toasts.retain(|t| !to_remove.contains(&t.id));
        }

        // Repaint solo durante l'animazione
        if !state.toasts.is_empty() {
            let has_animating = state.toasts.iter().any(|t| {
                t.created.elapsed().as_secs_f32() < SLIDE_IN_DURATION
            });

            if has_animating {
                ctx.request_repaint_after(std::time::Duration::from_millis(16));
            }
        }
    }
}