//! Shared editor document state (eframe and embedded runners).

use scene::{AssetKind, AssetRecord, CameraAuthoring, Level, PlacedObject, ProjectDocument};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Child;
use std::time::{Duration, Instant};
use tracing::info;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EditorMainTab {
    #[default]
    Level,
    Assets,
    ModelEditor,
}

#[derive(Debug, Clone)]
pub struct VoxelCell {
    pub x: u8,
    pub y: u8,
    pub z: u8,
    pub color_index: u8,
}

#[derive(Debug, Clone)]
pub struct VoxelModelEditorState {
    pub edge: u8,
    pub sphere_radius: u8,
    pub color_index: u8,
    pub voxels: Vec<VoxelCell>,
    pub export_name: String,
    pub active_tool: VoxelPaintTool,
    pub active_plane: VoxelEditPlane,
    pub active_layer: u8,
    pub orbit_yaw: f32,
    pub orbit_pitch: f32,
    pub camera_distance: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoxelPaintTool {
    Paint,
    Erase,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoxelEditPlane {
    XY,
    XZ,
    YZ,
}

impl Default for VoxelModelEditorState {
    fn default() -> Self {
        Self {
            edge: 8,
            sphere_radius: 4,
            color_index: 200,
            voxels: Vec::new(),
            export_name: "model.vox".into(),
            active_tool: VoxelPaintTool::Paint,
            active_plane: VoxelEditPlane::XY,
            active_layer: 0,
            orbit_yaw: -0.6,
            orbit_pitch: -0.4,
            camera_distance: 28.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FpsOverlayCorner {
    #[default]
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct EditorPreferences {
    #[serde(default = "default_true")]
    pub embedded_by_default: bool,
    #[serde(default)]
    pub external_editor_path: String,
    #[serde(default = "default_true")]
    pub use_internal_editor_by_default: bool,
    #[serde(default)]
    pub show_fps_overlay: bool,
    #[serde(default)]
    pub fps_overlay_corner: FpsOverlayCorner,
}

impl Default for EditorPreferences {
    fn default() -> Self {
        Self {
            embedded_by_default: true,
            external_editor_path: String::new(),
            use_internal_editor_by_default: true,
            show_fps_overlay: false,
            fps_overlay_corner: FpsOverlayCorner::TopLeft,
        }
    }
}

fn default_true() -> bool {
    true
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
    pub project_file_path: String,
    pub current_project: Option<ProjectDocument>,
    pub preferences: EditorPreferences,
    pub external_editor_candidates: Vec<String>,
    pub code_editor_open: bool,
    pub code_editor_path: String,
    pub code_editor_text: String,
    pub code_editor_status: String,
    pub code_editor_dirty: bool,
    pub asset_browser_selected_rel_dir: String,
    pub asset_browser_new_file_name: String,
    pub asset_browser_new_folder_name: String,
    pub selected_instance: Option<u64>,
    pub main_tab: EditorMainTab,
    pub voxel_model_editor: VoxelModelEditorState,
    /// Embedded only: last engine viewport in **physical pixels** (x, y, w, h) relative to the editor window client origin.
    pub engine_viewport_px: Option<(i32, i32, u32, u32)>,
    /// Embedded: set after Play applies the level so the winit loop uses `Poll` instead of `WaitUntil` (which can defer the engine redraw).
    pub pending_engine_repaint: bool,
    /// Embedded play mode: when true, engine view should capture input and stay foreground.
    pub play_mode_active: bool,
    /// Embedded preview mode: when true, editor applies level edits continuously to runtime.
    pub preview_mode_active: bool,
    /// Embedded one-shot request: focus engine window + acquire input capture.
    pub play_mode_capture_request: bool,
    /// Embedded runtime metric: rendered engine FPS (smoothed over ~0.5s windows).
    pub render_fps: f32,
    /// Rate-limits disk reloads while a separate Preferences window may be open.
    prefs_last_refresh: Instant,
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
            project_file_path: String::new(),
            current_project: None,
            preferences: EditorPreferences::default(),
            external_editor_candidates: Self::discover_external_editor_candidates(),
            code_editor_open: false,
            code_editor_path: String::new(),
            code_editor_text: String::new(),
            code_editor_status: String::new(),
            code_editor_dirty: false,
            asset_browser_selected_rel_dir: ".".to_string(),
            asset_browser_new_file_name: String::new(),
            asset_browser_new_folder_name: String::new(),
            selected_instance: None,
            main_tab: EditorMainTab::default(),
            voxel_model_editor: VoxelModelEditorState::default(),
            engine_viewport_px: None,
            pending_engine_repaint: false,
            play_mode_active: false,
            preview_mode_active: true,
            play_mode_capture_request: false,
            render_fps: 0.0,
            prefs_last_refresh: Instant::now(),
        }
    }

    /// Save then push the level to the embedded engine or external `engine-runner`.
    pub fn apply_level_to_engine(&mut self, embedded: Option<&mut engine_core::EngineState>) {
        match self.save_level_file() {
            Ok(()) => {
                if let Some(es) = embedded {
                    let level = self.level.clone();
                    let asset_root = self.project_root_dir();
                    es.apply_level_with_asset_root(&level, asset_root.as_deref());
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

    pub fn begin_play_mode(&mut self) {
        self.play_mode_active = true;
        self.play_mode_capture_request = true;
        self.push_log("Play mode enabled (engine input captured). Press Esc or Stop to release.");
    }

    pub fn stop_play_mode(&mut self, reason: &str) {
        if self.play_mode_active || self.play_mode_capture_request {
            self.play_mode_active = false;
            self.play_mode_capture_request = false;
            self.push_log(reason.to_string());
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

    pub fn new_project_dialog(&mut self) {
        let Some(seed_path) = rfd::FileDialog::new()
            .add_filter("VGE Project", &["vge"])
            .set_file_name("New Project.vge")
            .save_file()
        else {
            return;
        };
        if let Err(e) = self.create_new_project_from_seed_path(&seed_path) {
            self.status = format!("create project: {e}");
            self.push_log(self.status.clone());
        } else {
            self.status.clear();
        }
    }

    pub fn open_project_dialog(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("VGE Project", &["vge"])
            .pick_file()
        else {
            return;
        };
        self.project_file_path = path.to_string_lossy().into_owned();
        if let Err(e) = self.load_project_file() {
            self.status = format!("open project: {e}");
            self.push_log(self.status.clone());
        } else {
            self.status.clear();
        }
    }

    pub fn open_preferences_window(&mut self) {
        match std::env::current_exe() {
            Ok(exe) => {
                let mut cmd = std::process::Command::new(exe);
                cmd.arg("preferences");
                match cmd.spawn() {
                    Ok(_) => {
                        self.status.clear();
                        self.push_log("Opened Preferences window.");
                    }
                    Err(e) => {
                        self.status = format!("open preferences: {e}");
                        self.push_log(self.status.clone());
                    }
                }
            }
            Err(e) => {
                self.status = format!("current_exe: {e}");
                self.push_log(self.status.clone());
            }
        }
    }

    pub fn open_asset_file(&mut self, abs_path: &Path) -> Result<(), String> {
        if abs_path
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| s.eq_ignore_ascii_case("vox"))
            .unwrap_or(false)
        {
            self.load_model_editor_from_vox(abs_path)?;
            self.main_tab = EditorMainTab::ModelEditor;
            return Ok(());
        }
        self.refresh_preferences_from_disk();
        if self.preferences.use_internal_editor_by_default {
            self.open_file_in_internal_editor(abs_path)
        } else if !self.preferences.external_editor_path.trim().is_empty() {
            self.open_file_in_external_editor(abs_path)
                .or_else(|_| self.open_file_in_internal_editor(abs_path))
        } else {
            self.open_file_in_internal_editor(abs_path)
        }
    }

    pub fn save_open_code_editor_file(&mut self) -> Result<(), String> {
        if self.code_editor_path.trim().is_empty() {
            return Err("no file open in internal editor".into());
        }
        std::fs::write(&self.code_editor_path, &self.code_editor_text)
            .map_err(|e| format!("write {}: {e}", self.code_editor_path))?;
        self.code_editor_dirty = false;
        self.code_editor_status = format!("Saved {}", self.code_editor_path);
        Ok(())
    }

    pub fn create_new_file_in_selected_dir(&mut self) -> Result<(), String> {
        let name = self.asset_browser_new_file_name.trim();
        if name.is_empty() {
            return Err("new file name is empty".into());
        }
        let root = self
            .project_root_dir()
            .ok_or_else(|| "open a project to create files".to_string())?;
        let rel_dir = self.selected_asset_rel_dir_normalized();
        let rel_file = if rel_dir == "." {
            name.to_string()
        } else {
            format!("{rel_dir}/{name}")
        };
        let abs = scene::resolve_project_path(&root, &rel_file)
            .map_err(|e| format!("file path invalid: {e}"))?;
        if let Some(parent) = abs.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("mkdir {}: {e}", parent.display()))?;
        }
        if !abs.exists() {
            std::fs::write(&abs, "").map_err(|e| format!("create {}: {e}", abs.display()))?;
        }
        self.asset_browser_new_file_name.clear();
        self.push_log(format!("Created file {}", abs.display()));
        Ok(())
    }

    pub fn create_new_folder_in_selected_dir(&mut self) -> Result<(), String> {
        let name = self.asset_browser_new_folder_name.trim();
        if name.is_empty() {
            return Err("new folder name is empty".into());
        }
        let root = self
            .project_root_dir()
            .ok_or_else(|| "open a project to create folders".to_string())?;
        let rel_dir = self.selected_asset_rel_dir_normalized();
        let rel = if rel_dir == "." {
            name.to_string()
        } else {
            format!("{rel_dir}/{name}")
        };
        let abs = scene::resolve_project_path(&root, &rel)
            .map_err(|e| format!("folder path invalid: {e}"))?;
        std::fs::create_dir_all(&abs).map_err(|e| format!("mkdir {}: {e}", abs.display()))?;
        self.asset_browser_new_folder_name.clear();
        self.push_log(format!("Created folder {}", abs.display()));
        Ok(())
    }

    pub fn selected_asset_rel_dir_normalized(&self) -> String {
        let raw = self.asset_browser_selected_rel_dir.trim();
        if raw.is_empty() || raw == "." {
            ".".to_string()
        } else {
            scene::validate_relative_project_path(raw).unwrap_or_else(|_| ".".to_string())
        }
    }

    pub fn ensure_project_scripts_registered(&mut self) -> Result<(), String> {
        let scripts = self.discover_project_script_files()?;
        for abs in scripts {
            let _ = self.ensure_script_asset_for_path(&abs)?;
        }
        Ok(())
    }

    pub fn browse_script_asset_for_object(&mut self) -> Result<Option<String>, String> {
        let mut dialog = rfd::FileDialog::new().add_filter("Lua script", &["lua"]);
        if let Some(root) = self.project_root_dir() {
            dialog = dialog.set_directory(root);
        }
        let Some(path) = dialog.pick_file() else {
            return Ok(None);
        };
        let id = self.ensure_script_asset_for_path(&path)?;
        Ok(Some(id))
    }

    pub fn ensure_script_asset_for_path(&mut self, path: &Path) -> Result<String, String> {
        let abs = path
            .canonicalize()
            .map_err(|e| format!("{}: {e}", path.display()))?;
        let ext = abs
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if ext != "lua" {
            return Err(format!("{} is not a .lua file", abs.display()));
        }

        let stored_path = if let Some(root) = self.project_root_dir() {
            scene::make_project_relative_path(&root, &abs)
                .map_err(|e| format!("script outside project root: {e}"))?
        } else {
            abs.to_string_lossy().into_owned()
        };

        if let Some(existing) = self
            .level
            .assets
            .iter()
            .find(|a| a.kind == AssetKind::Script && a.path == stored_path)
        {
            return Ok(existing.id.clone());
        }

        let id = Uuid::new_v4().to_string();
        let name = abs
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("script")
            .to_string();
        self.level.assets.push(AssetRecord {
            id: id.clone(),
            name,
            kind: AssetKind::Script,
            path: stored_path,
        });
        self.sync_project_assets_from_level();
        Ok(id)
    }

    pub fn save_project_as_dialog(&mut self) {
        let default_name = std::path::Path::new(&self.project_file_path)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("project.vge");
        let Some(path) = rfd::FileDialog::new()
            .add_filter("VGE Project", &["vge"])
            .set_file_name(default_name)
            .save_file()
        else {
            return;
        };
        self.project_file_path = path.to_string_lossy().into_owned();
        if let Err(e) = self.save_project_file() {
            self.status = format!("save project: {e}");
            self.push_log(self.status.clone());
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
        let project_root = self.project_root_dir();
        for p in paths {
            let kind = Self::asset_kind_from_path(&p)?;
            if kind == AssetKind::Vox {
                let bytes = std::fs::read(&p).map_err(|e| format!("read {}: {e}", p.display()))?;
                dot_vox::load_bytes(&bytes)
                    .map_err(|e| format!("{}: invalid MagicaVoxel .vox: {e}", p.display()))?;
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
            let stored_path = if let Some(root) = project_root.as_deref() {
                scene::make_project_relative_path(root, &abs)
                    .map_err(|e| format!("asset outside project root: {e}"))?
            } else {
                abs.to_string_lossy().into_owned()
            };
            self.level.assets.push(AssetRecord {
                id: id.clone(),
                name: name.clone(),
                kind,
                path: stored_path,
            });
            self.push_log(format!("Imported asset {name} ({kind:?}) id={id}"));
        }
        self.sync_project_assets_from_level();
        Ok(())
    }

    pub fn remove_asset_by_id(&mut self, asset_id: &str) {
        self.level.assets.retain(|a| a.id != asset_id);
        for o in &mut self.level.objects {
            if o.script_asset_id.as_deref() == Some(asset_id) {
                o.script_asset_id = None;
            }
            if o.model_asset_id.as_deref() == Some(asset_id) {
                o.model_asset_id = None;
            }
        }
        self.sync_project_assets_from_level();
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
                    "Started external engine (same binary, `engine-runner` subcommand) on port {}.",
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
        self.push_log(
            "Embedded mode: Vulkan view is a child of the editor window when the OS allows it.",
        );
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
            scale: [1.0, 1.0, 1.0],
            rotation: [0.0, 0.0, 0.0],
            visible: true,
            camera,
            script_asset_id: None,
            model_asset_id: None,
        });
        self.selected_instance = Some(id);
        self.push_log(format!("Added {base_name} (instance {id})."));
    }

    pub fn generate_model_cube(&mut self) {
        let edge = self.voxel_model_editor.edge.max(1);
        let color = self.voxel_model_editor.color_index.max(1);
        self.voxel_model_editor.voxels.clear();
        for z in 0..edge {
            for y in 0..edge {
                for x in 0..edge {
                    self.voxel_model_editor.voxels.push(VoxelCell {
                        x,
                        y,
                        z,
                        color_index: color,
                    });
                }
            }
        }
        self.push_log(format!(
            "Model editor: generated cube {}^3 ({} voxels).",
            edge,
            self.voxel_model_editor.voxels.len()
        ));
    }

    pub fn generate_model_sphere(&mut self) {
        let edge = self.voxel_model_editor.edge.max(3);
        let radius = self.voxel_model_editor.sphere_radius.max(1) as f32;
        let color = self.voxel_model_editor.color_index.max(1);
        let c = (edge as f32 - 1.0) * 0.5;
        self.voxel_model_editor.voxels.clear();
        for z in 0..edge {
            for y in 0..edge {
                for x in 0..edge {
                    let dx = x as f32 - c;
                    let dy = y as f32 - c;
                    let dz = z as f32 - c;
                    if dx * dx + dy * dy + dz * dz <= radius * radius {
                        self.voxel_model_editor.voxels.push(VoxelCell {
                            x,
                            y,
                            z,
                            color_index: color,
                        });
                    }
                }
            }
        }
        self.push_log(format!(
            "Model editor: generated sphere r={} ({} voxels).",
            radius as u32,
            self.voxel_model_editor.voxels.len()
        ));
    }

    pub fn clear_model_voxels(&mut self) {
        self.voxel_model_editor.voxels.clear();
    }

    pub fn paint_model_voxel(&mut self, x: u8, y: u8, z: u8) {
        let edge = self.voxel_model_editor.edge.max(1);
        if x >= edge || y >= edge || z >= edge {
            return;
        }
        if let Some(existing) = self
            .voxel_model_editor
            .voxels
            .iter_mut()
            .find(|v| v.x == x && v.y == y && v.z == z)
        {
            existing.color_index = self.voxel_model_editor.color_index.max(1);
            return;
        }
        self.voxel_model_editor.voxels.push(VoxelCell {
            x,
            y,
            z,
            color_index: self.voxel_model_editor.color_index.max(1),
        });
    }

    pub fn erase_model_voxel(&mut self, x: u8, y: u8, z: u8) {
        self.voxel_model_editor
            .voxels
            .retain(|v| !(v.x == x && v.y == y && v.z == z));
    }

    pub fn export_model_vox_dialog(&mut self) -> Result<Option<String>, String> {
        if self.voxel_model_editor.voxels.is_empty() {
            return Err("model editor has no voxels to export".into());
        }
        let mut dialog = rfd::FileDialog::new().add_filter("MagicaVoxel", &["vox"]);
        if let Some(root) = self.project_root_dir() {
            dialog = dialog.set_directory(root.join("assets"));
        }
        let default_name = if self.voxel_model_editor.export_name.trim().is_empty() {
            "model.vox"
        } else {
            self.voxel_model_editor.export_name.trim()
        };
        let Some(path) = dialog.set_file_name(default_name).save_file() else {
            return Ok(None);
        };
        let asset_id = self.export_model_vox_to_path(&path)?;
        Ok(Some(asset_id))
    }

    fn export_model_vox_to_path(&mut self, path: &Path) -> Result<String, String> {
        let bytes = build_magica_vox_bytes(&self.voxel_model_editor.voxels)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("mkdir {}: {e}", parent.display()))?;
        }
        std::fs::write(path, &bytes).map_err(|e| format!("write {}: {e}", path.display()))?;
        let _ = dot_vox::load_bytes(&bytes)
            .map_err(|e| format!("exported VOX validation failed for {}: {e}", path.display()))?;
        self.voxel_model_editor.export_name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("model.vox")
            .to_string();
        let asset_id = self.ensure_vox_asset_for_path(path)?;
        self.push_log(format!("Exported model VOX: {}", path.display()));
        Ok(asset_id)
    }

    fn ensure_vox_asset_for_path(&mut self, path: &Path) -> Result<String, String> {
        let abs = path
            .canonicalize()
            .map_err(|e| format!("{}: {e}", path.display()))?;
        let stored_path = if let Some(root) = self.project_root_dir() {
            scene::make_project_relative_path(&root, &abs)
                .map_err(|e| format!("VOX outside project root: {e}"))?
        } else {
            abs.to_string_lossy().into_owned()
        };
        if let Some(existing) = self
            .level
            .assets
            .iter()
            .find(|a| a.kind == AssetKind::Vox && a.path == stored_path)
        {
            return Ok(existing.id.clone());
        }
        let id = Uuid::new_v4().to_string();
        let name = abs
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("model")
            .to_string();
        self.level.assets.push(AssetRecord {
            id: id.clone(),
            name,
            kind: AssetKind::Vox,
            path: stored_path,
        });
        self.sync_project_assets_from_level();
        Ok(id)
    }

    pub fn add_user_model_from_dialog(&mut self) -> Result<(), String> {
        let mut dialog = rfd::FileDialog::new().add_filter("MagicaVoxel", &["vox"]);
        if let Some(root) = self.project_root_dir() {
            dialog = dialog.set_directory(root.join("assets"));
        }
        let Some(path) = dialog.pick_file() else {
            return Ok(());
        };
        let bytes = std::fs::read(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
        dot_vox::load_bytes(&bytes)
            .map_err(|e| format!("{}: invalid MagicaVoxel .vox: {e}", path.display()))?;
        let asset_id = self.ensure_vox_asset_for_path(&path)?;
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("User Model")
            .to_string();
        self.add_model_instance(&asset_id, &name)?;
        Ok(())
    }

    pub fn add_model_instance(
        &mut self,
        model_asset_id: &str,
        base_name: &str,
    ) -> Result<(), String> {
        let rec = self
            .level
            .assets
            .iter()
            .find(|a| a.id == model_asset_id)
            .ok_or_else(|| "model asset id not found".to_string())?;
        if rec.kind != AssetKind::Vox {
            return Err("asset is not a VOX model".into());
        }
        let id = self.next_instance_id;
        self.next_instance_id += 1;
        self.level.objects.push(PlacedObject {
            instance_id: id,
            prefab_id: scene::ids::CUBE,
            name: format!("{base_name} {id}"),
            position: [0.0, 2.0, 0.0],
            scale: [1.0, 1.0, 1.0],
            rotation: [0.0, 0.0, 0.0],
            visible: true,
            camera: None,
            script_asset_id: None,
            model_asset_id: Some(model_asset_id.to_string()),
        });
        self.selected_instance = Some(id);
        self.push_log(format!("Added model instance {base_name} (instance {id})."));
        Ok(())
    }

    pub fn load_model_editor_from_vox(&mut self, path: &Path) -> Result<(), String> {
        let bytes = std::fs::read(path).map_err(|e| format!("read {}: {e}", path.display()))?;
        let file = dot_vox::load_bytes(&bytes)
            .map_err(|e| format!("{}: invalid MagicaVoxel .vox: {e}", path.display()))?;
        let model = file
            .models
            .first()
            .ok_or_else(|| format!("{}: no model data in VOX file", path.display()))?;
        let edge = model
            .size
            .x
            .max(model.size.y)
            .max(model.size.z)
            .clamp(1, 255) as u8;
        self.voxel_model_editor.edge = edge;
        self.voxel_model_editor.active_layer = 0;
        self.voxel_model_editor.voxels.clear();
        for v in &model.voxels {
            if v.x < edge && v.y < edge && v.z < edge {
                self.voxel_model_editor.voxels.push(VoxelCell {
                    x: v.x,
                    y: v.y,
                    z: v.z,
                    color_index: v.i.max(1),
                });
            }
        }
        self.voxel_model_editor.export_name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("model.vox")
            .to_string();
        self.push_log(format!(
            "Loaded VOX into model editor: {} ({} voxels)",
            path.display(),
            self.voxel_model_editor.voxels.len()
        ));
        Ok(())
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
        let full = self.resolve_level_path_for_io()?;
        full.canonicalize()
            .map(|p| p.to_string_lossy().into_owned())
            .map_err(|e| format!("resolve path: {e}"))
    }

    pub fn save_level_file(&mut self) -> Result<(), String> {
        let json = self
            .level
            .to_json_pretty()
            .map_err(|e| format!("serialize: {e}"))?;
        let level_path = self.resolve_level_path_for_io()?;
        if let Some(parent) = level_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("mkdir {}: {e}", parent.display()))?;
        }
        std::fs::write(&level_path, json)
            .map_err(|e| format!("write {}: {e}", level_path.display()))?;
        self.update_project_default_level_from_model();
        if self.current_project.is_some() {
            self.save_project_file()?;
        }
        self.push_log(format!("Saved {}", self.level_path));
        Ok(())
    }

    pub fn load_level_file(&mut self) -> Result<(), String> {
        let level_path = self.resolve_level_path_for_io()?;
        let s = std::fs::read_to_string(&level_path)
            .map_err(|e| format!("read {}: {e}", level_path.display()))?;
        self.level = Level::from_json_str(&s).map_err(|e| format!("parse JSON: {e}"))?;
        self.recompute_next_id();
        self.selected_instance = None;
        self.push_log(format!("Loaded {}", self.level_path));
        self.sync_project_assets_from_level();
        Ok(())
    }

    pub fn save_project_file(&mut self) -> Result<(), String> {
        let mut project = self
            .current_project
            .clone()
            .ok_or_else(|| "no active project".to_string())?;
        if self.project_file_path.is_empty() {
            return Err("project file path is empty".into());
        }
        project.assets = self.level.assets.clone();
        if project.name.trim().is_empty() {
            project.name = "Project".into();
        }
        if let Some(root) = self.project_root_dir() {
            if let Ok(rel) =
                scene::make_project_relative_path(&root, &self.resolve_level_path_for_io()?)
            {
                project.default_level = Some(rel);
            }
        }
        let path = PathBuf::from(&self.project_file_path);
        project
            .save_to_path_atomic(&path)
            .map_err(|e| format!("write {}: {e}", path.display()))?;
        self.current_project = Some(project);
        self.push_log(format!("Saved project {}", self.project_file_path));
        Ok(())
    }

    pub fn load_project_file(&mut self) -> Result<(), String> {
        let path = PathBuf::from(&self.project_file_path);
        let project = ProjectDocument::load_from_path(&path)
            .map_err(|e| format!("read {}: {e}", path.display()))?;
        self.current_project = Some(project.clone());
        self.push_log(format!("Loaded project {}", self.project_file_path));
        if let Some(default_level) = project.default_level {
            self.level_path = default_level;
            if let Err(e) = self.load_level_file() {
                self.push_log(format!("project default level load failed: {e}"));
            }
        }
        Ok(())
    }

    pub fn project_root_dir(&self) -> Option<PathBuf> {
        if self.project_file_path.is_empty() {
            return None;
        }
        PathBuf::from(&self.project_file_path)
            .parent()
            .map(|p| p.to_path_buf())
    }

    fn open_file_in_internal_editor(&mut self, abs_path: &Path) -> Result<(), String> {
        let txt = std::fs::read_to_string(abs_path)
            .map_err(|e| format!("open {} as text: {e}", abs_path.display()))?;
        self.code_editor_path = abs_path.to_string_lossy().into_owned();
        self.code_editor_text = txt;
        self.code_editor_status.clear();
        self.code_editor_dirty = false;
        self.code_editor_open = true;
        Ok(())
    }

    fn open_file_in_external_editor(&mut self, abs_path: &Path) -> Result<(), String> {
        let editor = self.preferences.external_editor_path.trim();
        if editor.is_empty() {
            return Err("external editor path is empty".into());
        }
        let mut cmd = std::process::Command::new(editor);
        cmd.arg(abs_path);
        cmd.spawn()
            .map_err(|e| format!("launch external editor {}: {e}", editor))?;
        self.push_log(format!(
            "Opened in external editor: {}",
            abs_path.to_string_lossy()
        ));
        Ok(())
    }

    fn refresh_preferences_from_disk(&mut self) {
        self.preferences = crate::editor_state::load_startup_preferences();
    }

    pub fn reload_preferences_from_disk_if_due(&mut self) {
        const REFRESH_PERIOD: Duration = Duration::from_millis(500);
        if self.prefs_last_refresh.elapsed() >= REFRESH_PERIOD {
            self.refresh_preferences_from_disk();
            self.prefs_last_refresh = Instant::now();
        }
    }

    fn discover_project_script_files(&self) -> Result<Vec<PathBuf>, String> {
        let Some(root) = self.project_root_dir() else {
            return Ok(Vec::new());
        };
        let mut out = Vec::new();
        Self::walk_collect_lua(&root, &mut out)?;
        out.sort();
        out.dedup();
        Ok(out)
    }

    fn walk_collect_lua(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
        let rd = std::fs::read_dir(dir).map_err(|e| format!("read_dir {}: {e}", dir.display()))?;
        for entry in rd {
            let entry = entry.map_err(|e| format!("read_dir entry {}: {e}", dir.display()))?;
            let p = entry.path();
            if p.is_dir() {
                Self::walk_collect_lua(&p, out)?;
            } else if p
                .extension()
                .and_then(|s| s.to_str())
                .map(|s| s.eq_ignore_ascii_case("lua"))
                .unwrap_or(false)
            {
                out.push(p);
            }
        }
        Ok(())
    }

    fn resolve_level_path_for_io(&self) -> Result<PathBuf, String> {
        let p = PathBuf::from(&self.level_path);
        if p.is_absolute() {
            return Ok(p);
        }
        if let Some(root) = self.project_root_dir() {
            return scene::resolve_project_path(&root, &self.level_path)
                .map_err(|e| format!("project level path: {e}"));
        }
        std::env::current_dir()
            .map(|cwd| cwd.join(p))
            .map_err(|e| format!("cwd: {e}"))
    }

    fn sync_project_assets_from_level(&mut self) {
        if let Some(project) = self.current_project.as_mut() {
            project.assets = self.level.assets.clone();
        }
    }

    fn update_project_default_level_from_model(&mut self) {
        let root = self.project_root_dir();
        let level_abs = self.resolve_level_path_for_io();
        if let (Some(project), Some(root), Ok(level_abs)) =
            (self.current_project.as_mut(), root, level_abs)
        {
            if let Ok(rel) = scene::make_project_relative_path(&root, &level_abs) {
                project.default_level = Some(rel);
            }
        }
    }

    fn create_new_project_from_seed_path(&mut self, seed_path: &Path) -> Result<(), String> {
        let parent = seed_path
            .parent()
            .ok_or_else(|| format!("invalid project location: {}", seed_path.display()))?;
        let name = seed_path
            .file_stem()
            .and_then(|s| s.to_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("Project")
            .to_string();

        let project_root = parent.join(&name);
        let project_file = project_root.join(format!("{name}.vge"));
        let levels_dir = project_root.join("levels");
        let assets_dir = project_root.join("assets");
        let scripts_dir = project_root.join("scripts");

        std::fs::create_dir_all(&levels_dir)
            .map_err(|e| format!("mkdir {}: {e}", levels_dir.display()))?;
        std::fs::create_dir_all(&assets_dir)
            .map_err(|e| format!("mkdir {}: {e}", assets_dir.display()))?;
        std::fs::create_dir_all(&scripts_dir)
            .map_err(|e| format!("mkdir {}: {e}", scripts_dir.display()))?;

        self.level = Level::default();
        self.level.name = format!("{name} Level");
        self.level.assets.clear();
        self.recompute_next_id();
        self.selected_instance = None;
        self.level_path = "levels/main.vge.json".to_string();
        self.project_file_path = project_file.to_string_lossy().into_owned();
        self.current_project = Some(ProjectDocument::new(name.clone()));

        self.save_level_file()?;
        self.push_log(format!(
            "Created project scaffold at {}",
            project_root.display()
        ));
        Ok(())
    }

    pub fn discover_external_editor_candidates() -> Vec<String> {
        let mut out: Vec<String> = Vec::new();

        let mut push_if_exists = |p: PathBuf| {
            if p.is_file() {
                let s = p.to_string_lossy().into_owned();
                if !out.contains(&s) {
                    out.push(s);
                }
            }
        };

        let mut scan_path_for_names = |names: &[&str]| {
            if let Some(path_var) = std::env::var_os("PATH") {
                for dir in std::env::split_paths(&path_var) {
                    for name in names {
                        push_if_exists(dir.join(name));
                    }
                }
            }
        };

        #[cfg(windows)]
        {
            let names = [
                "Code.exe",
                "Code - Insiders.exe",
                "Cursor.exe",
                "sublime_text.exe",
            ];
            scan_path_for_names(&names);
            let local = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
            let prog_files = std::env::var_os("ProgramFiles").map(PathBuf::from);
            let prog_files_x86 = std::env::var_os("ProgramFiles(x86)").map(PathBuf::from);
            if let Some(local) = local {
                push_if_exists(local.join("Programs/Microsoft VS Code/Code.exe"));
                push_if_exists(local.join("Programs/Cursor/Cursor.exe"));
            }
            if let Some(pf) = prog_files {
                push_if_exists(pf.join("Sublime Text/sublime_text.exe"));
                push_if_exists(pf.join("Microsoft VS Code/Code.exe"));
            }
            if let Some(pf86) = prog_files_x86 {
                push_if_exists(pf86.join("Microsoft VS Code/Code.exe"));
                push_if_exists(pf86.join("Sublime Text/sublime_text.exe"));
            }
        }

        #[cfg(not(windows))]
        {
            let names = ["code", "cursor", "subl", "sublime_text", "code-insiders"];
            scan_path_for_names(&names);
            for fallback in [
                "/Applications/Visual Studio Code.app/Contents/Resources/app/bin/code",
                "/Applications/Cursor.app/Contents/MacOS/Cursor",
                "/usr/bin/code",
                "/usr/local/bin/code",
                "/snap/bin/code",
            ] {
                push_if_exists(PathBuf::from(fallback));
            }
        }

        out.sort();
        out.dedup();
        out
    }
}

fn build_magica_vox_bytes(voxels: &[VoxelCell]) -> Result<Vec<u8>, String> {
    if voxels.is_empty() {
        return Err("cannot export an empty voxel model".into());
    }
    if voxels.len() > u32::MAX as usize {
        return Err("too many voxels for VOX format".into());
    }
    let mut unique = BTreeSet::new();
    let mut max_x = 0u8;
    let mut max_y = 0u8;
    let mut max_z = 0u8;
    for v in voxels {
        unique.insert((v.x, v.y, v.z, v.color_index.max(1)));
        max_x = max_x.max(v.x);
        max_y = max_y.max(v.y);
        max_z = max_z.max(v.z);
    }
    let unique_voxels: Vec<(u8, u8, u8, u8)> = unique.into_iter().collect();

    fn le_u32(dst: &mut Vec<u8>, v: u32) {
        dst.extend_from_slice(&v.to_le_bytes());
    }
    fn push_chunk(dst: &mut Vec<u8>, id: &[u8; 4], content: &[u8], children: &[u8]) {
        dst.extend_from_slice(id);
        le_u32(dst, content.len() as u32);
        le_u32(dst, children.len() as u32);
        dst.extend_from_slice(content);
        dst.extend_from_slice(children);
    }

    let mut size_content = Vec::with_capacity(12);
    le_u32(&mut size_content, u32::from(max_x) + 1);
    le_u32(&mut size_content, u32::from(max_y) + 1);
    le_u32(&mut size_content, u32::from(max_z) + 1);

    let mut xyzi_content = Vec::with_capacity(4 + unique_voxels.len() * 4);
    le_u32(&mut xyzi_content, unique_voxels.len() as u32);
    for (x, y, z, ci) in unique_voxels {
        xyzi_content.extend_from_slice(&[x, y, z, ci]);
    }

    let mut rgba_content = Vec::with_capacity(256 * 4);
    for i in 0..256u16 {
        let c = i as u8;
        rgba_content.extend_from_slice(&[c, c, c, 255]);
    }

    let mut children = Vec::new();
    push_chunk(&mut children, b"SIZE", &size_content, &[]);
    push_chunk(&mut children, b"XYZI", &xyzi_content, &[]);
    push_chunk(&mut children, b"RGBA", &rgba_content, &[]);

    let mut out = Vec::new();
    out.extend_from_slice(b"VOX ");
    out.extend_from_slice(&150u32.to_le_bytes());
    push_chunk(&mut out, b"MAIN", &[], &children);
    Ok(out)
}

#[cfg(test)]
mod vox_tests {
    use super::*;

    #[test]
    fn writes_valid_vox_bytes() {
        let voxels = vec![
            VoxelCell {
                x: 0,
                y: 0,
                z: 0,
                color_index: 1,
            },
            VoxelCell {
                x: 1,
                y: 0,
                z: 0,
                color_index: 2,
            },
        ];
        let bytes = build_magica_vox_bytes(&voxels).expect("export bytes");
        let file = dot_vox::load_bytes(&bytes).expect("parse exported bytes");
        assert_eq!(file.models.len(), 1);
        assert_eq!(file.models[0].voxels.len(), 2);
    }
}
