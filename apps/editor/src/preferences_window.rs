use crate::editor_state;
use crate::model::{EditorModel, EditorPreferences, FpsOverlayCorner};
use eframe::egui;

pub fn run() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([700.0, 300.0])
            .with_title("Editor Preferences"),
        ..Default::default()
    };
    eframe::run_native(
        "Editor Preferences",
        options,
        Box::new(|_cc| Ok(Box::new(PreferencesApp::new()))),
    )
}

struct PreferencesApp {
    prefs: EditorPreferences,
    candidates: Vec<String>,
    status: String,
}

impl PreferencesApp {
    fn new() -> Self {
        Self {
            prefs: editor_state::load_startup_preferences(),
            candidates: EditorModel::discover_external_editor_candidates(),
            status: String::new(),
        }
    }

    fn save(&mut self) {
        match editor_state::save_preferences(&self.prefs) {
            Ok(()) => {
                self.status = "Saved.".to_string();
            }
            Err(e) => {
                self.status = format!("Save failed: {e}");
            }
        }
    }
}

impl eframe::App for PreferencesApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Preferences");
            ui.separator();

            ui.checkbox(
                &mut self.prefs.embedded_by_default,
                "Open engine in embedded view by default",
            );
            ui.checkbox(
                &mut self.prefs.use_internal_editor_by_default,
                "Use in-editor code editor by default",
            );
            ui.checkbox(
                &mut self.prefs.show_fps_overlay,
                "Show FPS overlay in embedded viewport",
            );
            ui.horizontal(|ui| {
                ui.label("FPS overlay corner");
                egui::ComboBox::from_id_salt("prefs_fps_corner")
                    .selected_text(match self.prefs.fps_overlay_corner {
                        FpsOverlayCorner::TopLeft => "Top left",
                        FpsOverlayCorner::TopRight => "Top right",
                        FpsOverlayCorner::BottomLeft => "Bottom left",
                        FpsOverlayCorner::BottomRight => "Bottom right",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut self.prefs.fps_overlay_corner,
                            FpsOverlayCorner::TopLeft,
                            "Top left",
                        );
                        ui.selectable_value(
                            &mut self.prefs.fps_overlay_corner,
                            FpsOverlayCorner::TopRight,
                            "Top right",
                        );
                        ui.selectable_value(
                            &mut self.prefs.fps_overlay_corner,
                            FpsOverlayCorner::BottomLeft,
                            "Bottom left",
                        );
                        ui.selectable_value(
                            &mut self.prefs.fps_overlay_corner,
                            FpsOverlayCorner::BottomRight,
                            "Bottom right",
                        );
                    });
            });
            ui.label(
                egui::RichText::new(
                    "CLI/env flags still override this default (`--no-embedded`, `--embedded`, `VGE_EMBEDDED`).",
                )
                .small()
                .weak(),
            );

            ui.add_space(10.0);
            ui.label("External code editor path");
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt("prefs_external_editor_candidates")
                    .selected_text(if self.prefs.external_editor_path.is_empty() {
                        "Detected editors…".to_string()
                    } else {
                        self.prefs.external_editor_path.clone()
                    })
                    .show_ui(ui, |ui| {
                        for p in &self.candidates {
                            if ui.selectable_label(false, p).clicked() {
                                self.prefs.external_editor_path = p.clone();
                            }
                        }
                    });
            });
            ui.horizontal(|ui| {
                ui.add_sized(
                    [560.0, 24.0],
                    egui::TextEdit::singleline(&mut self.prefs.external_editor_path)
                        .hint_text("Path to editor executable"),
                );
                if ui.button("Browse…").clicked() {
                    if let Some(path) = rfd::FileDialog::new().pick_file() {
                        self.prefs.external_editor_path = path.to_string_lossy().into_owned();
                    }
                }
            });

            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui.button("Refresh detections").clicked() {
                    self.candidates = EditorModel::discover_external_editor_candidates();
                }
                if ui.button("Save").clicked() {
                    self.save();
                }
                if ui.button("Close").clicked() {
                    self.save();
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
            if !self.status.is_empty() {
                ui.label(egui::RichText::new(&self.status).small().weak());
            }
        });
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        let _ = editor_state::save_preferences(&self.prefs);
    }
}

