mod app;
mod state;
mod models;
mod api;
mod style;

fn main() -> eframe::Result<()> {
    eframe::run_native(
        "Ruggine",
        eframe::NativeOptions::default(),
        Box::new(|_cc| Box::new(app::App::new())),
    )
}
