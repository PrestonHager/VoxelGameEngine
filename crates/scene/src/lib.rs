//! Prefab catalog and serializable levels for the editor MVP and engine sync.
//!
//! **Documentation:** see the repository `README.md` (Editor + prefab table) and Sphinx
//! `docs/source/editor.rst` / `docs/source/prefabs.rst` after running `sphinx-build`.

mod level;
mod prefabs;

pub use level::{Level, PlacedObject, TerrainLayer, TerrainMode, LEVEL_FORMAT_VERSION};
pub use prefabs::{ids, PrefabCategory, PrefabInfo, PrefabLibrary};
