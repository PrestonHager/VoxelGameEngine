use serde::{Deserialize, Serialize};

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

/// One placed object in a level (editor instance id is stable in the UI until removed).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlacedObject {
    pub instance_id: u64,
    pub prefab_id: u32,
    pub name: String,
    pub position: [f32; 3],
    #[serde(default = "default_true")]
    pub visible: bool,
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
}

impl Default for Level {
    fn default() -> Self {
        Self {
            format_version: LEVEL_FORMAT_VERSION,
            name: "Untitled".into(),
            objects: Vec::new(),
            terrain: TerrainLayer::default(),
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
        level.objects.push(PlacedObject {
            instance_id: 1,
            prefab_id: 1,
            name: "Cube A".into(),
            position: [1.0, 2.0, 3.0],
            visible: true,
        });
        let s = level.to_json_pretty().unwrap();
        let back = Level::from_json_str(&s).unwrap();
        assert_eq!(back.name, level.name);
        assert_eq!(back.objects.len(), 1);
        assert_eq!(back.objects[0].position, [1.0, 2.0, 3.0]);
    }
}
