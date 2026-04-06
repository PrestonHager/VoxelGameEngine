//! Prefab catalog and serializable levels for the editor MVP and engine sync.
//!
//! **Documentation:** see the repository `README.md` (Editor + prefab table) and Sphinx
//! `docs/source/editor.rst` / `docs/source/prefabs.rst` after running `sphinx-build`.

mod level;
mod prefabs;
mod project;

pub use level::{
    AssetKind, AssetRecord, CameraAuthoring, Level, PlacedObject, TerrainLayer, TerrainMode,
    LEVEL_FORMAT_VERSION,
};
pub use prefabs::{ids, PrefabCategory, PrefabInfo, PrefabLibrary};
pub use project::{
    discover_project_root_from_level_path, make_project_relative_path, resolve_project_path,
    validate_relative_project_path, ProjectDocument, ProjectError, PROJECT_FORMAT_VERSION,
};
