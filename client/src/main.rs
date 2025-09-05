mod app;
mod state;
mod models;
mod net;
mod style;

fn main() -> eframe::Result<()> {
    eframe::run_native(
        "Ruggine",
        eframe::NativeOptions::default(),
        Box::new(|_cc| Box::new(app::App::new())),
    )
}
