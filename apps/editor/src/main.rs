//! Editor shell: egui UI, optional auto-start of `engine-runner`, IPC ping.
//! The engine’s Vulkan output stays in its own window until a child-surface render path exists.

mod launcher;

use eframe::egui;
use std::process::Child;
use tracing::info;

fn main() -> eframe::Result {
    logging::init();
    let port: u16 = std::env::var("VGE_IPC_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(7878);

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([560.0, 460.0])
            .with_title("Voxel Editor"),
        ..Default::default()
    };

    eframe::run_native(
        "Voxel Editor",
        options,
        Box::new(move |_cc| {
            Ok(Box::new(EditorApp {
                port,
                auto_started: None,
                log: Vec::new(),
                status: String::new(),
                bootstrap_done: false,
            }))
        }),
    )
}

struct EditorApp {
    port: u16,
    auto_started: Option<Child>,
    log: Vec<String>,
    status: String,
    bootstrap_done: bool,
}

impl EditorApp {
    fn push_log(&mut self, line: impl Into<String>) {
        let s = line.into();
        info!("{s}");
        self.log.push(s);
        if self.log.len() > 200 {
            self.log.drain(0..50);
        }
    }

    fn bootstrap(&mut self) {
        if self.bootstrap_done {
            return;
        }
        self.bootstrap_done = true;
        match launcher::ensure_engine_running(self.port) {
            Ok(None) => self.push_log(format!("Engine already listening on port {}.", self.port)),
            Ok(Some(child)) => {
                self.auto_started = Some(child);
                self.push_log(format!(
                    "Started engine-runner next to this binary (port {}).",
                    self.port
                ));
            }
            Err(e) => {
                self.status.clone_from(&e);
                self.push_log(e);
            }
        }
    }
}

impl eframe::App for EditorApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.bootstrap();

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Voxel Editor");
            ui.label(format!("IPC port: {} (set VGE_IPC_PORT to change)", self.port));
            ui.separator();
            ui.label(
                egui::RichText::new(
                    "The backend opens its own window for Vulkan. True in-process viewport \
                     embedding is not implemented yet.",
                )
                .weak(),
            );
            ui.separator();
            if !self.status.is_empty() {
                ui.colored_label(
                    egui::Color32::from_rgb(220, 140, 80),
                    &self.status,
                );
            }
            ui.horizontal(|ui| {
                if ui.button("Ping engine").clicked() {
                    match launcher::ping_engine(self.port) {
                        Ok(reply) => {
                            self.status.clear();
                            self.push_log(format!("Ping OK: {reply}"));
                        }
                        Err(e) => {
                            self.status.clone_from(&e);
                            self.push_log(format!("Ping failed: {e}"));
                        }
                    }
                }
                if ui.button("Retry start engine").clicked() {
                    self.bootstrap_done = false;
                    if let Some(mut c) = self.auto_started.take() {
                        let _ = c.kill();
                    }
                    self.status.clear();
                    self.bootstrap();
                }
            });
            ui.separator();
            egui::ScrollArea::vertical().show(ui, |ui| {
                for line in &self.log {
                    ui.monospace(line);
                }
            });
        });
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if let Some(mut c) = self.auto_started.take() {
            let _ = c.kill();
        }
    }
}
