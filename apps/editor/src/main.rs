//! Editor shell: prefab library + level authoring + optional embedded Vulkan view (`--embedded` or `VGE_EMBEDDED`).

mod config;
mod editor_state;
mod embedded;
mod launcher;
mod model;
mod ui;

use eframe::egui;
use model::EditorModel;
use ui::draw_editor_ui;

fn main() -> eframe::Result {
    logging::init();
    let port: u16 = std::env::var("VGE_IPC_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(7878);

    if config::embedded_mode_requested() {
        tracing::info!("starting embedded editor (use --embedded or VGE_EMBEDDED=1; see config.rs docs)");
        if let Err(e) = embedded::run_embedded(port) {
            eprintln!("embedded editor failed: {e}");
            std::process::exit(1);
        }
        return Ok(());
    }

    tracing::info!(
        target: "vge_embedded",
        "using eframe + external engine-runner (not embedded). For in-process Vulkan + child window, use: cargo run -p editor -- --embedded   or   PowerShell: $env:VGE_EMBEDDED='1'; cargo run -p editor"
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
