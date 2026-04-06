use crate::{AssetKind, AssetRecord};
use serde::{Deserialize, Serialize};
use std::ffi::OsStr;
use std::path::{Component, Path, PathBuf};
use thiserror::Error;

pub const PROJECT_FORMAT_VERSION: u32 = 1;
const PROJECT_MAGIC: &[u8; 8] = b"VGEPRJ\0\n";

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct ProjectDocument {
    #[serde(default = "default_project_format_version")]
    pub format_version: u32,
    pub name: String,
    #[serde(default)]
    pub default_level: Option<String>,
    #[serde(default)]
    pub assets: Vec<AssetRecord>,
    #[serde(default)]
    pub vsync_enabled: bool,
}

fn default_project_format_version() -> u32 {
    PROJECT_FORMAT_VERSION
}

#[derive(Debug, Error)]
pub enum ProjectError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("bincode: {0}")]
    Bincode(#[from] bincode::Error),
    #[error("invalid project header")]
    InvalidHeader,
    #[error("project path must be relative")]
    AbsolutePath,
    #[error("project path must not traverse outside root")]
    PathTraversal,
    #[error("project path contains invalid prefix")]
    InvalidPrefix,
    #[error("project path is empty")]
    EmptyPath,
}

fn normalize_relative_path(path: &Path) -> Result<PathBuf, ProjectError> {
    if path.as_os_str().is_empty() {
        return Err(ProjectError::EmptyPath);
    }
    if path.is_absolute() {
        return Err(ProjectError::AbsolutePath);
    }

    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::Normal(seg) => out.push(seg),
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    return Err(ProjectError::PathTraversal);
                }
            }
            Component::Prefix(_) => return Err(ProjectError::InvalidPrefix),
            Component::RootDir => return Err(ProjectError::AbsolutePath),
        }
    }

    if out.as_os_str().is_empty() {
        return Err(ProjectError::EmptyPath);
    }
    Ok(out)
}

pub fn validate_relative_project_path(path: &str) -> Result<String, ProjectError> {
    let norm = normalize_relative_path(Path::new(path))?;
    Ok(norm.to_string_lossy().replace('\\', "/"))
}

pub fn resolve_project_path(project_root: &Path, rel_path: &str) -> Result<PathBuf, ProjectError> {
    let rel = normalize_relative_path(Path::new(rel_path))?;
    Ok(project_root.join(rel))
}

pub fn make_project_relative_path(
    project_root: &Path,
    abs_path: &Path,
) -> Result<String, ProjectError> {
    // Try fast path first.
    let rel = match abs_path.strip_prefix(project_root) {
        Ok(rel) => rel.to_path_buf(),
        Err(_) => {
            // On Windows, canonicalized paths may use verbatim prefixes (\\?\) which makes a
            // lexical strip_prefix against a non-canonical root fail. Normalize both sides.
            let root_norm = project_root
                .canonicalize()
                .unwrap_or_else(|_| project_root.to_path_buf());
            let abs_norm = abs_path
                .canonicalize()
                .unwrap_or_else(|_| abs_path.to_path_buf());
            abs_norm
                .strip_prefix(&root_norm)
                .map_err(|_| ProjectError::PathTraversal)?
                .to_path_buf()
        }
    };
    validate_relative_project_path(&rel.to_string_lossy())
}

impl ProjectDocument {
    pub fn new(name: String) -> Self {
        Self {
            format_version: PROJECT_FORMAT_VERSION,
            name,
            default_level: None,
            assets: Vec::new(),
            vsync_enabled: false,
        }
    }

    pub fn validate(&self) -> Result<(), ProjectError> {
        if let Some(level) = &self.default_level {
            let _ = validate_relative_project_path(level)?;
        }
        for a in &self.assets {
            let _ = validate_relative_project_path(&a.path)?;
            match a.kind {
                AssetKind::Level | AssetKind::Vox | AssetKind::Script => {}
            }
        }
        Ok(())
    }

    pub fn from_binary_bytes(bytes: &[u8]) -> Result<Self, ProjectError> {
        if bytes.len() < PROJECT_MAGIC.len() || &bytes[..PROJECT_MAGIC.len()] != PROJECT_MAGIC {
            return Err(ProjectError::InvalidHeader);
        }
        let mut doc: Self = bincode::deserialize(&bytes[PROJECT_MAGIC.len()..])?;
        doc.validate()?;
        if doc.format_version == 0 {
            doc.format_version = PROJECT_FORMAT_VERSION;
        }
        Ok(doc)
    }

    pub fn to_binary_bytes(&self) -> Result<Vec<u8>, ProjectError> {
        self.validate()?;
        let payload = bincode::serialize(self)?;
        let mut out = Vec::with_capacity(PROJECT_MAGIC.len() + payload.len());
        out.extend_from_slice(PROJECT_MAGIC);
        out.extend_from_slice(&payload);
        Ok(out)
    }

    pub fn load_from_path(path: &Path) -> Result<Self, ProjectError> {
        let bytes = std::fs::read(path)?;
        Self::from_binary_bytes(&bytes)
    }

    pub fn save_to_path_atomic(&self, path: &Path) -> Result<(), ProjectError> {
        let bytes = self.to_binary_bytes()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let ext = path.extension().and_then(OsStr::to_str).unwrap_or("vge");
        let tmp = path.with_extension(format!("{ext}.tmp"));
        std::fs::write(&tmp, bytes)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }
}

pub fn discover_project_root_from_level_path(level_path: &Path) -> Option<PathBuf> {
    let mut cursor = level_path.parent()?;
    loop {
        let has_vge = std::fs::read_dir(cursor).ok()?.flatten().any(|e| {
            e.path()
                .extension()
                .and_then(OsStr::to_str)
                .map(|ext| ext.eq_ignore_ascii_case("vge"))
                .unwrap_or(false)
        });
        if has_vge {
            return Some(cursor.to_path_buf());
        }
        cursor = cursor.parent()?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn project_binary_roundtrip() {
        let mut p = ProjectDocument::new("Demo".into());
        p.default_level = Some("levels/main.vge.json".into());
        p.assets.push(AssetRecord {
            id: Uuid::new_v4().to_string(),
            name: "logic".into(),
            kind: AssetKind::Script,
            path: "scripts/logic.lua".into(),
        });
        let bytes = p.to_binary_bytes().expect("encode");
        let back = ProjectDocument::from_binary_bytes(&bytes).expect("decode");
        assert_eq!(back.name, "Demo");
        assert_eq!(back.default_level.as_deref(), Some("levels/main.vge.json"));
        assert_eq!(back.assets.len(), 1);
        assert!(!back.vsync_enabled);
    }

    #[test]
    fn rejects_path_traversal() {
        assert!(validate_relative_project_path("../secret.lua").is_err());
        assert!(validate_relative_project_path("levels/../../secret.lua").is_err());
    }

    #[test]
    fn resolves_relative_path() {
        let root = Path::new("/tmp/project");
        let p = resolve_project_path(root, "assets/tex/a.png").expect("resolve");
        assert_eq!(p, Path::new("/tmp/project/assets/tex/a.png"));
    }

    #[test]
    fn make_project_relative_path_handles_canonical_mismatch() {
        let base = std::env::temp_dir().join(format!("vge-project-{}", Uuid::new_v4()));
        let root = base.join("Demo Project");
        let scripts = root.join("scripts");
        std::fs::create_dir_all(&scripts).expect("mkdir scripts");
        let script = scripts.join("camera.lua");
        std::fs::write(&script, "-- test").expect("write script");

        let root_noncanonical = root.join(".");
        let script_canonical = script.canonicalize().expect("canonical script");
        let rel =
            make_project_relative_path(&root_noncanonical, &script_canonical).expect("relative");
        assert_eq!(rel, "scripts/camera.lua");

        let _ = std::fs::remove_dir_all(base);
    }
}
