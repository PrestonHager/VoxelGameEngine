use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const LEVEL_FORMAT_VERSION: u32 = 1;

fn default_format_version() -> u32 {
    LEVEL_FORMAT_VERSION
}

fn default_true() -> bool {
    true
}

fn default_base_height() -> i32 {
    0
}

fn default_cam_fov() -> f32 {
    45.0
}

fn default_scale() -> [f32; 3] {
    [1.0, 1.0, 1.0]
}

fn default_rotation() -> [f32; 3] {
    [0.0, 0.0, 0.0]
}

/// Optional camera rig when `prefab_id` is `ids::CAMERA`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CameraAuthoring {
    #[serde(default = "default_cam_fov")]
    pub fov_deg: f32,
    #[serde(default)]
    pub yaw_deg: f32,
    #[serde(default)]
    pub pitch_deg: f32,
    #[serde(default = "default_true")]
    pub active: bool,
}

impl Default for CameraAuthoring {
    fn default() -> Self {
        Self {
            fov_deg: default_cam_fov(),
            yaw_deg: 0.0,
            pitch_deg: -20.0,
            active: true,
        }
    }
}

/// How terrain is generated when a level is applied (MVP: flat slab).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerrainMode {
    #[default]
    Flat,
}

/// Authoring-time terrain description; engine maps this to voxel chunks.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TerrainLayer {
    #[serde(default)]
    pub mode: TerrainMode,
    #[serde(default)]
    pub surface_material: u16,
    #[serde(default = "default_base_height")]
    pub base_height_voxels: i32,
}

impl Default for TerrainLayer {
    fn default() -> Self {
        Self {
            mode: TerrainMode::default(),
            surface_material: 1,
            base_height_voxels: default_base_height(),
        }
    }
}

/// Imported file referenced by the level (paths are usually absolute after import from the editor).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AssetRecord {
    pub id: String,
    /// Display name in the asset browser.
    pub name: String,
    #[serde(rename = "type")]
    pub kind: AssetKind,
    /// Absolute or project-relative path on disk.
    pub path: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetKind {
    Level,
    Vox,
    Script,
}

/// One placed object in a level (editor instance id is stable in the UI until removed).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlacedObject {
    pub instance_id: u64,
    pub prefab_id: u32,
    pub name: String,
    pub position: [f32; 3],
    #[serde(default = "default_scale")]
    pub scale: [f32; 3],
    /// Euler radians: pitch (x), yaw (y), roll (z).
    #[serde(default = "default_rotation")]
    pub rotation: [f32; 3],
    #[serde(default = "default_true")]
    pub visible: bool,
    #[serde(default)]
    pub camera: Option<CameraAuthoring>,
    /// References `AssetRecord.id` where `kind == script` (optional per-object Lua).
    #[serde(default)]
    pub script_asset_id: Option<String>,
    /// References `AssetRecord.id` where `kind == vox` (optional per-object model).
    #[serde(default)]
    pub model_asset_id: Option<String>,
}

/// Serializable level: objects + terrain; used for save/load and IPC file path loads.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Level {
    #[serde(default = "default_format_version")]
    pub format_version: u32,
    pub name: String,
    #[serde(default)]
    pub objects: Vec<PlacedObject>,
    #[serde(default)]
    pub terrain: TerrainLayer,
    #[serde(default)]
    pub assets: Vec<AssetRecord>,
}

impl Default for Level {
    fn default() -> Self {
        Self {
            format_version: LEVEL_FORMAT_VERSION,
            name: "Untitled".into(),
            objects: Vec::new(),
            terrain: TerrainLayer::default(),
            assets: Vec::new(),
        }
    }
}

impl Level {
    pub fn from_json_str(s: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(s)
    }

    pub fn to_json_pretty(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Resolve a stored asset path for loading (engine / tooling).
    ///
    /// If `base_dir` is provided, relative paths are resolved against it.
    /// Otherwise relative paths are resolved from current working directory.
    pub fn resolve_asset_path_with_base(
        &self,
        asset_id: &str,
        base_dir: Option<&Path>,
    ) -> Option<PathBuf> {
        let rec = self.assets.iter().find(|a| a.id == asset_id)?;
        let p = Path::new(&rec.path);
        if p.is_absolute() {
            Some(p.to_path_buf())
        } else {
            let base = match base_dir {
                Some(dir) => dir.to_path_buf(),
                None => std::env::current_dir().ok()?,
            };
            base.join(p).canonicalize().ok()
        }
    }

    pub fn resolve_asset_path(&self, asset_id: &str) -> Option<PathBuf> {
        self.resolve_asset_path_with_base(asset_id, None)
    }

    /// Script assets only (for per-object Lua).
    pub fn resolve_script_asset_path_with_base(
        &self,
        asset_id: &str,
        base_dir: Option<&Path>,
    ) -> Option<PathBuf> {
        let rec = self.assets.iter().find(|a| a.id == asset_id)?;
        if rec.kind != AssetKind::Script {
            return None;
        }
        self.resolve_asset_path_with_base(asset_id, base_dir)
    }

    pub fn resolve_script_asset_path(&self, asset_id: &str) -> Option<PathBuf> {
        self.resolve_script_asset_path_with_base(asset_id, None)
    }

    /// VOX assets only (for model instances).
    pub fn resolve_vox_asset_path_with_base(
        &self,
        asset_id: &str,
        base_dir: Option<&Path>,
    ) -> Option<PathBuf> {
        let rec = self.assets.iter().find(|a| a.id == asset_id)?;
        if rec.kind != AssetKind::Vox {
            return None;
        }
        self.resolve_asset_path_with_base(asset_id, base_dir)
    }

    pub fn resolve_vox_asset_path(&self, asset_id: &str) -> Option<PathBuf> {
        self.resolve_vox_asset_path_with_base(asset_id, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_roundtrip() {
        let mut level = Level {
            name: "Test".into(),
            ..Default::default()
        };
        level.terrain = TerrainLayer {
            mode: TerrainMode::Flat,
            surface_material: 7,
            base_height_voxels: 2,
        };
        level.assets.push(AssetRecord {
            id: "a1".into(),
            name: "Mesh".into(),
            kind: AssetKind::Vox,
            path: "models/a.vox".into(),
        });
        level.assets.push(AssetRecord {
            id: "s1".into(),
            name: "Logic".into(),
            kind: AssetKind::Script,
            path: "scripts/hook.lua".into(),
        });
        level.objects.push(PlacedObject {
            instance_id: 1,
            prefab_id: 1,
            name: "Cube A".into(),
            position: [1.0, 2.0, 3.0],
            scale: [1.0, 1.0, 1.0],
            rotation: [0.0, 0.0, 0.0],
            visible: true,
            camera: None,
            script_asset_id: Some("s1".into()),
            model_asset_id: Some("a1".into()),
        });
        let s = level.to_json_pretty().unwrap();
        let back = Level::from_json_str(&s).unwrap();
        assert_eq!(back.name, level.name);
        assert_eq!(back.terrain.surface_material, 7);
        assert_eq!(back.terrain.base_height_voxels, 2);
        assert_eq!(back.assets.len(), 2);
        assert_eq!(back.objects.len(), 1);
        assert_eq!(back.objects[0].position, [1.0, 2.0, 3.0]);
        assert_eq!(back.objects[0].script_asset_id.as_deref(), Some("s1"));
        assert_eq!(back.objects[0].model_asset_id.as_deref(), Some("a1"));
    }

    #[test]
    fn resolve_script_asset_path_rejects_non_script_assets() {
        let level = Level {
            assets: vec![AssetRecord {
                id: "tex1".into(),
                name: "Not a script".into(),
                kind: AssetKind::Vox,
                path: "/abs/fake.vox".into(),
            }],
            ..Default::default()
        };
        assert!(level.resolve_script_asset_path("tex1").is_none());
    }

    #[test]
    fn resolve_script_asset_path_accepts_script_kind() {
        let abs = std::env::temp_dir().join("vge_scene_test_hook.lua");
        let level = Level {
            assets: vec![AssetRecord {
                id: "s1".into(),
                name: "Hook".into(),
                kind: AssetKind::Script,
                path: abs.to_string_lossy().into_owned(),
            }],
            ..Default::default()
        };
        let p = level.resolve_script_asset_path("s1").expect("script path");
        assert_eq!(p, abs);
    }

    #[test]
    fn resolve_vox_asset_path_rejects_non_vox_assets() {
        let level = Level {
            assets: vec![AssetRecord {
                id: "s1".into(),
                name: "Script".into(),
                kind: AssetKind::Script,
                path: "/abs/fake.lua".into(),
            }],
            ..Default::default()
        };
        assert!(level.resolve_vox_asset_path("s1").is_none());
    }
}
