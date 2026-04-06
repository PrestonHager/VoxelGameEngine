//! Shared editor document state (eframe and embedded runners).

use scene::{AssetKind, AssetRecord, CameraAuthoring, Level, PlacedObject};
use std::path::{Path, PathBuf};
use std::process::Child;
use tracing::info;
use uuid::Uuid;

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Default,
    serde::Serialize,
    serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum EditorMainTab {
    #[default]
    Level,
    Assets,
}

pub struct EditorModel {
    pub port: u16,
    pub auto_started: Option<Child>,
    pub log: Vec<String>,
    pub status: String,
    pub bootstrap_done: bool,
    pub level: Level,
    pub next_instance_id: u64,
    pub level_path: String,
    pub selected_instance: Option<u64>,
    pub main_tab: EditorMainTab,
    /// Embedded only: last engine viewport in **physical pixels** (x, y, w, h) relative to the editor window client origin.
    pub engine_viewport_px: Option<(i32, i32, u32, u32)>,
    /// Embedded: set after Play applies the level so the winit loop uses `Poll` instead of `WaitUntil` (which can defer the engine redraw).
    pub pending_engine_repaint: bool,
}

impl EditorModel {
    pub fn new(port: u16) -> Self {
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
            main_tab: EditorMainTab::default(),
            engine_viewport_px: None,
            pending_engine_repaint: false,
        }
    }

    /// Save then push the level to the embedded engine or external `engine-runner`.
    pub fn apply_level_to_engine(&mut self, embedded: Option<&mut engine_core::EngineState>) {
        match self.save_level_file() {
            Ok(()) => {
                if let Some(es) = embedded {
                    let level = self.level.clone();
                    es.apply_level(&level);
                    self.pending_engine_repaint = true;
                    self.status.clear();
                    self.push_log("Play: applied level to embedded engine.");
                } else {
                    match self.absolutize_level_path() {
                        Ok(abs) => match crate::launcher::push_level_path(self.port, &abs) {
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
                    }
                }
            }
            Err(e) => {
                self.status.clone_from(&e);
                self.push_log(e);
            }
        }
    }

    pub fn open_level_dialog(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Level JSON", &["json"])
            .pick_file()
        else {
            return;
        };
        self.level_path = path.to_string_lossy().into_owned();
        if let Err(e) = self.load_level_file() {
            self.status.clone_from(&e);
            self.push_log(e);
        } else {
            self.status.clear();
        }
    }

    pub fn save_level_as_dialog(&mut self) {
        let default_name = std::path::Path::new(&self.level_path)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("level.vge.json");
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Level JSON", &["json"])
            .set_file_name(default_name)
            .save_file()
        else {
            return;
        };
        self.level_path = path.to_string_lossy().into_owned();
        if let Err(e) = self.save_level_file() {
            self.status.clone_from(&e);
            self.push_log(e);
        } else {
            self.status.clear();
        }
    }

    fn asset_kind_from_path(p: &Path) -> Result<AssetKind, String> {
        let ext = p
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        match ext.as_str() {
            "lua" => Ok(AssetKind::Script),
            "vox" => Ok(AssetKind::Vox),
            "json" => Ok(AssetKind::Level),
            _ => Err(format!(
                "unsupported type '.{ext}' (use .lua, .vox, or level .json)"
            )),
        }
    }

    /// Register imported files on the level (paths should exist).
    pub fn import_asset_paths(&mut self, paths: Vec<PathBuf>) -> Result<(), String> {
        for p in paths {
            let kind = Self::asset_kind_from_path(&p)?;
            if kind == AssetKind::Vox {
                let bytes = std::fs::read(&p).map_err(|e| format!("read {}: {e}", p.display()))?;
                dot_vox::load_bytes(&bytes).map_err(|e| format!("{}: invalid MagicaVoxel .vox: {e}", p.display()))?;
            }
            let abs = p
                .canonicalize()
                .map_err(|e| format!("{}: {e}", p.display()))?;
            let id = Uuid::new_v4().to_string();
            let name = abs
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("asset")
                .to_string();
            self.level.assets.push(AssetRecord {
                id: id.clone(),
                name: name.clone(),
                kind,
                path: abs.to_string_lossy().into_owned(),
            });
            self.push_log(format!("Imported asset {name} ({kind:?}) id={id}"));
        }
        Ok(())
    }

    pub fn remove_asset_by_id(&mut self, asset_id: &str) {
        self.level.assets.retain(|a| a.id != asset_id);
        for o in &mut self.level.objects {
            if o.script_asset_id.as_deref() == Some(asset_id) {
                o.script_asset_id = None;
            }
        }
    }

    pub fn push_log(&mut self, line: impl Into<String>) {
        let s = line.into();
        info!("{s}");
        self.log.push(s);
        if self.log.len() > 200 {
            self.log.drain(0..50);
        }
    }

    /// External process engine (default `cargo run` workflow).
    pub fn bootstrap_external(&mut self) {
        if self.bootstrap_done {
            return;
        }
        self.bootstrap_done = true;
        match crate::launcher::ensure_engine_running(self.port) {
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

    pub fn bootstrap_embedded(&mut self) {
        if self.bootstrap_done {
            return;
        }
        self.bootstrap_done = true;
        tracing::info!(
            target: "vge_embedded",
            "bootstrap_embedded: UI will drive engine_viewport_px on Level tab"
        );
        self.push_log("Embedded mode: Vulkan view is a child of the editor window when the OS allows it.");
    }

    pub fn add_placed(&mut self, prefab_id: u32, base_name: &str) {
        let id = self.next_instance_id;
        self.next_instance_id += 1;
        let camera = (prefab_id == scene::ids::CAMERA).then_some(CameraAuthoring::default());
        self.level.objects.push(PlacedObject {
            instance_id: id,
            prefab_id,
            name: format!("{base_name} {id}"),
            position: [0.0, 2.0, 0.0],
            visible: true,
            camera,
            script_asset_id: None,
        });
        self.selected_instance = Some(id);
        self.push_log(format!("Added {base_name} (instance {id})."));
    }

    pub fn recompute_next_id(&mut self) {
        self.next_instance_id = self
            .level
            .objects
            .iter()
            .map(|o| o.instance_id)
            .max()
            .unwrap_or(0)
            .saturating_add(1);
    }

    pub fn absolutize_level_path(&self) -> Result<String, String> {
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

    pub fn save_level_file(&mut self) -> Result<(), String> {
        let json = self
            .level
            .to_json_pretty()
            .map_err(|e| format!("serialize: {e}"))?;
        std::fs::write(&self.level_path, json)
            .map_err(|e| format!("write {}: {e}", self.level_path))?;
        self.push_log(format!("Saved {}", self.level_path));
        Ok(())
    }

    pub fn load_level_file(&mut self) -> Result<(), String> {
        let s = std::fs::read_to_string(&self.level_path)
            .map_err(|e| format!("read {}: {e}", self.level_path))?;
        self.level = Level::from_json_str(&s).map_err(|e| format!("parse JSON: {e}"))?;
        self.recompute_next_id();
        self.selected_instance = None;
        self.push_log(format!("Loaded {}", self.level_path));
        Ok(())
    }
}
