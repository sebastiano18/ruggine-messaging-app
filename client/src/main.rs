mod app;
mod state;
mod models;
mod api;
mod style;

fn main() -> eframe::Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    eframe::run_native(
        "Ruggine",
        eframe::NativeOptions::default(),
        Box::new(|cc| {
            // Configura font con supporto emoji
            setup_fonts(&cc.egui_ctx);
            Box::new(app::App::new())
        }),
    )
}

fn setup_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    
    // Aggiungi supporto emoji usando la font Segoe UI Emoji su Windows
    // o font di sistema alternative
    fonts.families
        .entry(egui::FontFamily::Proportional)
        .or_default()
        .insert(0, "emoji".to_owned());
    
    // Usa la font emoji di sistema
    #[cfg(target_os = "windows")]
    {
        // Su Windows, usa Segoe UI Emoji
        if let Some(emoji_font) = load_system_emoji_font() {
            fonts.font_data.insert("emoji".to_owned(), emoji_font);
        }
    }
    
    ctx.set_fonts(fonts);
}

#[cfg(target_os = "windows")]
fn load_system_emoji_font() -> Option<egui::FontData> {
    use std::fs;
    
    // Prova diverse posizioni della font emoji su Windows
    let emoji_paths = [
        "C:\\Windows\\Fonts\\seguiemj.ttf",  // Segoe UI Emoji
        "C:\\Windows\\Fonts\\segoeui.ttf",   // Segoe UI (fallback)
    ];
    
    for path in &emoji_paths {
        if let Ok(font_bytes) = fs::read(path) {
            tracing::info!("Caricata font emoji da: {}", path);
            return Some(egui::FontData::from_owned(font_bytes));
        }
    }
    
    tracing::warn!("Nessuna font emoji trovata");
    None
}

#[cfg(not(target_os = "windows"))]
fn load_system_emoji_font() -> Option<egui::FontData> {
    None
}