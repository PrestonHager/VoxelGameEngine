//! Editor shell: egui UI, prefab library + level authoring, optional auto-start of `engine-runner`.

mod launcher;

use eframe::egui;
use scene::{Level, PlacedObject, PrefabCategory, PrefabLibrary, TerrainMode};
use std::path::PathBuf;
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
            .with_inner_size([960.0, 640.0])
            .with_title("Voxel Editor"),
        ..Default::default()
    };

    eframe::run_native(
        "Voxel Editor",
        options,
        Box::new(move |_cc| {
            Ok(Box::new(EditorApp::new(port)))
        }),
    )
}

struct EditorApp {
    port: u16,
    auto_started: Option<Child>,
    log: Vec<String>,
    status: String,
    bootstrap_done: bool,
    level: Level,
    next_instance_id: u64,
    level_path: String,
    selected_instance: Option<u64>,
}

impl EditorApp {
    fn new(port: u16) -> Self {
        Self {
            port,
            auto_started: None,
            log: Vec::new(),
            status: String::new(),
            bootstrap_done: false,
            level: Level::default(),
            next_instance_id: 1,
            level_path: "demo_level.vge.json".into(),
            selected_instance: None,
        }
    }

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

    fn add_placed(&mut self, prefab_id: u32, base_name: &str) {
        let id = self.next_instance_id;
        self.next_instance_id += 1;
        self.level.objects.push(PlacedObject {
            instance_id: id,
            prefab_id,
            name: format!("{base_name} {id}"),
            position: [0.0, 2.0, 0.0],
            visible: true,
        });
        self.selected_instance = Some(id);
        self.push_log(format!("Added {base_name} (instance {id})."));
    }

    fn recompute_next_id(&mut self) {
        self.next_instance_id = self
            .level
            .objects
            .iter()
            .map(|o| o.instance_id)
            .max()
            .unwrap_or(0)
            .saturating_add(1);
    }

    fn absolutize_level_path(&self) -> Result<String, String> {
        let p = PathBuf::from(&self.level_path);
        let full = if p.is_absolute() {
            p
        } else {
            std::env::current_dir()
                .map_err(|e| format!("cwd: {e}"))?
                .join(p)
        };
        full.canonicalize()
            .map(|p| p.to_string_lossy().into_owned())
            .map_err(|e| format!("resolve path: {e}"))
    }

    fn save_level_file(&mut self) -> Result<(), String> {
        let json = self
            .level
            .to_json_pretty()
            .map_err(|e| format!("serialize: {e}"))?;
        std::fs::write(&self.level_path, json).map_err(|e| format!("write {}: {e}", self.level_path))?;
        self.push_log(format!("Saved {}", self.level_path));
        Ok(())
    }

    fn load_level_file(&mut self) -> Result<(), String> {
        let s = std::fs::read_to_string(&self.level_path)
            .map_err(|e| format!("read {}: {e}", self.level_path))?;
        self.level = Level::from_json_str(&s).map_err(|e| format!("parse JSON: {e}"))?;
        self.recompute_next_id();
        self.selected_instance = None;
        self.push_log(format!("Loaded {}", self.level_path));
        Ok(())
    }
}

impl eframe::App for EditorApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.bootstrap();

        egui::SidePanel::left("prefabs")
            .default_width(200.0)
            .show(ctx, |ui| {
                ui.heading("Library");
                ui.label(egui::RichText::new("Built-in prefabs (ECS scene objects)").weak());
                ui.separator();
                for cat in [
                    PrefabCategory::Primitive,
                    PrefabCategory::Gameplay,
                    PrefabCategory::Environment,
                    PrefabCategory::Utility,
                ] {
                    ui.collapsing(format!("{cat:?}"), |ui| {
                        for p in PrefabLibrary::builtin()
                            .iter()
                            .filter(|p| p.category == cat)
                        {
                            if ui.button(&p.name).clicked() {
                                self.add_placed(p.id, &p.name);
                            }
                        }
                    });
                }
            });

        egui::SidePanel::right("scene")
            .default_width(280.0)
            .show(ctx, |ui| {
                ui.heading("Scene");
                ui.label("Placed objects");
                ui.separator();
                egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
                    for o in &self.level.objects {
                        let sel = self.selected_instance == Some(o.instance_id);
                        let label = format!("{} (#{})", o.name, o.instance_id);
                        if ui.selectable_label(sel, label).clicked() {
                            self.selected_instance = Some(o.instance_id);
                        }
                    }
                });
                if ui.button("Delete selected").clicked() {
                    if let Some(id) = self.selected_instance {
                        self.level.objects.retain(|o| o.instance_id != id);
                        self.selected_instance = None;
                        self.push_log(format!("Removed instance {id}."));
                    }
                }

                if let Some(id) = self.selected_instance {
                    if let Some(o) = self.level.objects.iter_mut().find(|o| o.instance_id == id) {
                        ui.separator();
                        ui.label(format!("Edit #{}", id));
                        ui.horizontal(|ui| {
                            ui.label("name");
                            ui.text_edit_singleline(&mut o.name);
                        });
                        ui.horizontal(|ui| {
                            ui.label("x");
                            ui.add(egui::DragValue::new(&mut o.position[0]).speed(0.1));
                            ui.label("y");
                            ui.add(egui::DragValue::new(&mut o.position[1]).speed(0.1));
                            ui.label("z");
                            ui.add(egui::DragValue::new(&mut o.position[2]).speed(0.1));
                        });
                        ui.checkbox(&mut o.visible, "visible");
                    }
                }

                ui.separator();
                ui.collapsing("Terrain (MVP)", |ui| {
                    ui.label(format!("Mode: {:?}", TerrainMode::Flat));
                    ui.horizontal(|ui| {
                        ui.label("surface material");
                        ui.add(egui::DragValue::new(&mut self.level.terrain.surface_material).speed(1.0));
                    });
                    ui.horizontal(|ui| {
                        ui.label("base height (voxels)");
                        ui.add(egui::DragValue::new(&mut self.level.terrain.base_height_voxels).speed(1.0));
                    });
                });
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Voxel Editor");
            ui.label(format!("IPC port: {} (set VGE_IPC_PORT to change)", self.port));
            ui.separator();
            ui.label(
                egui::RichText::new(
                    "The backend opens its own window for Vulkan. Push writes the level file and \
                     tells the engine to reload it from disk.",
                )
                .weak(),
            );
            ui.separator();

            ui.horizontal(|ui| {
                ui.label("Level name");
                ui.text_edit_singleline(&mut self.level.name);
            });
            ui.horizontal(|ui| {
                ui.label("Level file");
                ui.text_edit_singleline(&mut self.level_path);
            });

            if !self.status.is_empty() {
                ui.colored_label(egui::Color32::from_rgb(220, 140, 80), &self.status);
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
                if ui.button("Save level").clicked() {
                    if let Err(e) = self.save_level_file() {
                        self.status = e.clone();
                        self.push_log(e);
                    } else {
                        self.status.clear();
                    }
                }
                if ui.button("Load level").clicked() {
                    match self.load_level_file() {
                        Ok(()) => self.status.clear(),
                        Err(e) => {
                            self.status.clone_from(&e);
                            self.push_log(e);
                        }
                    }
                }
                if ui.button("Push to engine").clicked() {
                    match self.save_level_file() {
                        Ok(()) => match self.absolutize_level_path() {
                            Ok(abs) => match launcher::push_level_path(self.port, &abs) {
                                Ok(reply) => {
                                    self.status.clear();
                                    self.push_log(format!("Push OK: {reply} ({abs})"));
                                }
                                Err(e) => {
                                    self.status.clone_from(&e);
                                    self.push_log(e);
                                }
                            },
                            Err(e) => {
                                self.status.clone_from(&e);
                                self.push_log(e);
                            }
                        },
                        Err(e) => {
                            self.status.clone_from(&e);
                            self.push_log(e);
                        }
                    }
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
