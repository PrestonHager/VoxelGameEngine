use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

/// High-level grouping in the editor palette.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrefabCategory {
    #[default]
    Primitive,
    Gameplay,
    Environment,
    Utility,
}

/// One entry in the built-in object library (IDs are stable for serialized levels).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrefabInfo {
    pub id: u32,
    pub name: String,
    pub category: PrefabCategory,
}

/// Built-in prefab IDs (match `PrefabInfo.id` in the catalog).
pub mod ids {
    pub const CUBE: u32 = 1;
    pub const SPHERE_PROXY: u32 = 2;
    pub const SPAWN_POINT: u32 = 3;
    pub const WAYPOINT: u32 = 4;
    pub const LIGHT_PROBE: u32 = 5;
    pub const TREE: u32 = 6;
    pub const ROCK: u32 = 7;
    pub const TERRAIN_CHUNK: u32 = 8;
}

static BUILTIN: OnceLock<Vec<PrefabInfo>> = OnceLock::new();

fn builtin_vec() -> &'static Vec<PrefabInfo> {
    BUILTIN.get_or_init(|| {
        vec![
            PrefabInfo {
                id: ids::CUBE,
                name: "Cube".into(),
                category: PrefabCategory::Primitive,
            },
            PrefabInfo {
                id: ids::SPHERE_PROXY,
                name: "Sphere (proxy)".into(),
                category: PrefabCategory::Primitive,
            },
            PrefabInfo {
                id: ids::SPAWN_POINT,
                name: "Spawn point".into(),
                category: PrefabCategory::Gameplay,
            },
            PrefabInfo {
                id: ids::WAYPOINT,
                name: "Waypoint".into(),
                category: PrefabCategory::Gameplay,
            },
            PrefabInfo {
                id: ids::LIGHT_PROBE,
                name: "Light probe".into(),
                category: PrefabCategory::Utility,
            },
            PrefabInfo {
                id: ids::TREE,
                name: "Tree".into(),
                category: PrefabCategory::Environment,
            },
            PrefabInfo {
                id: ids::ROCK,
                name: "Rock".into(),
                category: PrefabCategory::Environment,
            },
            PrefabInfo {
                id: ids::TERRAIN_CHUNK,
                name: "Terrain marker".into(),
                category: PrefabCategory::Environment,
            },
        ]
    })
}

/// Read-only access to the editor/engine object library.
pub struct PrefabLibrary;

impl PrefabLibrary {
    pub fn builtin() -> &'static [PrefabInfo] {
        builtin_vec().as_slice()
    }

    pub fn resolve(id: u32) -> Option<&'static PrefabInfo> {
        builtin_vec().iter().find(|p| p.id == id)
    }
}
