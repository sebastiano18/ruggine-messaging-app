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
            let mut fonts = egui::FontDefinitions::default();
            egui_remixicon::add_to_fonts(&mut fonts);
            cc.egui_ctx.set_fonts(fonts);

            Box::new(app::App::new())
        }),
    )
}