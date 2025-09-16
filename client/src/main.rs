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
        Box::new(|_cc| Box::new(app::App::new())),
    )
}