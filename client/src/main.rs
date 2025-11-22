mod app;
mod state;
mod models;
mod api;
mod style;

fn main() -> eframe::Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Ruggine - Chat Application")
            .with_inner_size([1280.0, 720.0])          // Dimensione iniziale: 1280x720
            .with_min_inner_size([800.0, 600.0])       // Dimensione minima: 800x600
            .with_resizable(true),                      // Permetti ridimensionamento
        ..Default::default()
    };

    eframe::run_native(
        "Ruggine",
        options,
        Box::new(|cc| {
            let mut fonts = egui::FontDefinitions::default();
            egui_remixicon::add_to_fonts(&mut fonts);
            cc.egui_ctx.set_fonts(fonts);

            Box::new(app::App::new())
        }),
    )
}