use eframe::egui::{self, Color32, Rounding, Stroke, Vec2, Frame};

pub(crate) fn apply_azure_theme(ui: &mut egui::Ui) {
    let visuals = ui.visuals_mut();

    // Base chiara azzurrina
    visuals.dark_mode = false;
    visuals.panel_fill  = Color32::from_rgb(235, 244, 252); // sfondo pannello (azzurrino chiarissimo)
    visuals.window_fill = Color32::from_rgb(242, 248, 255); // finestra
    visuals.faint_bg_color = Color32::from_rgb(225, 238, 250);

    // Colori testo / hyperlink / selezione
    visuals.override_text_color = Some(Color32::from_rgb(20, 40, 70));
    visuals.hyperlink_color = Color32::from_rgb(30, 90, 200);
    visuals.selection.bg_fill = Color32::from_rgb(180, 210, 255);
    visuals.selection.stroke  = Stroke::new(1.0, Color32::from_rgb(70, 120, 200));

    // Rounding morbido
    let rounding = Rounding::same(8.0);

    // Palette widget (idle/hover/active)
    let mut idle   = visuals.widgets.inactive;
    let mut hover  = visuals.widgets.hovered;
    let mut active = visuals.widgets.active;
    let border = Stroke::new(1.0, Color32::from_rgb(140, 180, 230));

    idle.rounding   = rounding;
    hover.rounding  = rounding;
    active.rounding = rounding;

    idle.bg_fill   = Color32::from_rgb(228, 240, 252);
    hover.bg_fill  = Color32::from_rgb(214, 232, 250);
    active.bg_fill = Color32::from_rgb(200, 224, 248);

    idle.weak_bg_fill   = Color32::from_rgb(240, 246, 255);
    hover.weak_bg_fill  = Color32::from_rgb(230, 242, 255);
    active.weak_bg_fill = Color32::from_rgb(220, 236, 252);

    idle.bg_stroke   = border;
    hover.bg_stroke  = Stroke::new(1.5, Color32::from_rgb(110, 160, 230));
    active.bg_stroke = Stroke::new(2.0,  Color32::from_rgb(90, 140, 220));

    idle.fg_stroke   = Stroke::new(1.0, Color32::from_rgb(30, 50, 90));
    hover.fg_stroke  = Stroke::new(1.0, Color32::from_rgb(20, 40, 80));
    active.fg_stroke = Stroke::new(1.0, Color32::from_rgb(10, 30, 70));

    visuals.widgets.inactive = idle;
    visuals.widgets.hovered  = hover;
    visuals.widgets.active   = active;

    // Scrollbar e slider
    visuals.extreme_bg_color = Color32::from_rgb(210, 228, 248);
    visuals.code_bg_color    = Color32::from_rgb(240, 246, 255);

    // Spaziature leggere (un filo più ariose)
    let style = ui.style_mut();
    style.spacing.item_spacing = Vec2::new(8.0, 8.0);
    style.spacing.button_padding = Vec2::new(10.0, 8.0);
}
