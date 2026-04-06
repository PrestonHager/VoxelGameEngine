//! Editor shell: prefab library + level authoring + embedded Vulkan by default (`--no-embedded` opt-out).

mod config;
mod editor_state;
mod embedded;
mod engine_runner;
mod launcher;
mod model;
mod preferences_window;
mod ui;

use eframe::egui;
use model::EditorModel;
use ui::draw_editor_ui;

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 2 && args[1] == "engine-runner" {
        engine_runner::run();
        return Ok(());
    }
    if args.len() >= 2 && args[1] == "preferences" {
        logging::init();
        return preferences_window::run();
    }

    logging::init();
    let prefs = editor_state::load_startup_preferences();
    let port: u16 = std::env::var("VGE_IPC_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(7878);

    if config::embedded_mode_requested(prefs.embedded_by_default) {
        tracing::info!(
            "starting embedded editor (default mode; pass --no-embedded for separate engine window)"
        );
        if let Err(e) = embedded::run_embedded(port) {
            eprintln!("embedded editor failed: {e}");
            std::process::exit(1);
        }
        return Ok(());
    }

    tracing::info!(
        target: "vge_embedded",
        "using eframe + external engine-runner (--no-embedded). Embedded is default; unset --no-embedded to use in-process Vulkan view."
    );
    tracing::debug!(
        target: "vge_embedded",
        args = ?std::env::args().collect::<Vec<_>>(),
        "process argv (embedded not requested)"
    );

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 780.0])
            .with_title("Voxel Editor"),
        ..Default::default()
    };

    eframe::run_native(
        "Voxel Editor",
        options,
        Box::new(move |_cc| Ok(Box::new(EditorApp::new(port)))),
    )
}

struct EditorApp {
    model: EditorModel,
}

impl EditorApp {
    fn new(port: u16) -> Self {
        let mut model = EditorModel::new(port);
        editor_state::apply_loaded_session(&mut model);
        Self { model }
    }
}

impl eframe::App for EditorApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        draw_editor_ui(ctx, &mut self.model, None);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if let Err(e) = editor_state::save_from_model(&self.model) {
            tracing::warn!("failed to save editor session: {e}");
        }
        if let Some(mut c) = self.model.auto_started.take() {
            let _ = c.kill();
        }
    }
}
