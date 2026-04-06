//! Persisted editor session in the OS user config directory (cross-platform).
//!
//! - **Linux:** `~/.config/VoxelGameEngine/editor_state.json` (or `XDG_CONFIG_HOME`).
//! - **macOS:** `~/Library/Application Support/VoxelGameEngine/editor_state.json`.
//! - **Windows:** `%APPDATA%\VoxelGameEngine\editor_state.json`.

use crate::model::{EditorMainTab, EditorModel};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use thiserror::Error;
use tracing::warn;

const FORMAT_VERSION: u32 = 1;
const APP_CONFIG_DIR: &str = "VoxelGameEngine";
const STATE_FILE: &str = "editor_state.json";

#[derive(Serialize, Deserialize)]
pub struct EditorSessionState {
    pub format_version: u32,
    pub level_path: String,
    #[serde(default)]
    pub project_file_path: String,
    pub main_tab: EditorMainTab,
}

#[derive(Debug, Error)]
pub enum EditorStateError {
    #[error("io: {0}")]
    Io(#[from] io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
}

/// Resolved config directory for this app, if the OS provides one.
pub fn config_dir_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join(APP_CONFIG_DIR))
}

pub fn state_file_path() -> Option<PathBuf> {
    config_dir_path().map(|d| d.join(STATE_FILE))
}

/// Load persisted session, if the state file exists and parses.
///
/// Missing file or empty file yields `Ok(None)`. I/O or JSON errors are returned so corrupt
/// data is visible to callers.
pub fn load() -> Result<Option<EditorSessionState>, EditorStateError> {
    let path = match state_file_path() {
        Some(p) => p,
        None => return Ok(None),
    };
    let data = match fs::read_to_string(&path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(EditorStateError::Io(e)),
        Ok(s) => s,
    };
    if data.trim().is_empty() {
        return Ok(None);
    }
    Ok(Some(serde_json::from_str(&data)?))
}

/// Restore last tab and level path; loads the level from disk when the file exists.
pub fn apply_loaded_session(model: &mut EditorModel) {
    let s = match load() {
        Ok(Some(s)) => s,
        Ok(None) => return,
        Err(e) => {
            warn!(
                target = "editor_state",
                "failed to load editor session: {e}"
            );
            model.push_log(format!("Could not load editor session: {e}"));
            return;
        }
    };
    if s.format_version > FORMAT_VERSION {
        model.push_log(
            "Ignoring editor_state.json: file is from a newer editor version.".to_string(),
        );
        return;
    }

    model.main_tab = s.main_tab;
    model.project_file_path = s.project_file_path.trim().to_string();
    if !model.project_file_path.is_empty() {
        if let Err(e) = model.load_project_file() {
            model.push_log(format!("Could not load project file from session: {e}"));
        }
    }

    let path = s.level_path.trim().to_string();
    if path.is_empty() {
        return;
    }

    model.level_path = path.clone();

    if Path::new(&path).is_file() {
        if let Err(e) = model.load_level_file() {
            model.status.clone_from(&e);
            model.push_log(e);
        } else {
            model.push_log(format!("Restored project: {path}"));
        }
    } else {
        let msg = format!("Saved level path not found: {path}");
        model.status.clone_from(&msg);
        model.push_log(msg);
    }
}

pub fn save_from_model(model: &EditorModel) -> io::Result<()> {
    let path = state_file_path().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "no user config directory (dirs::config_dir returned None)",
        )
    })?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let state = EditorSessionState {
        format_version: FORMAT_VERSION,
        level_path: model.level_path.clone(),
        project_file_path: model.project_file_path.clone(),
        main_tab: model.main_tab,
    };
    let json = serde_json::to_string_pretty(&state)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    let tmp_path = path.with_extension("json.tmp");
    let mut f = fs::File::create(&tmp_path)?;
    f.write_all(json.as_bytes())?;
    f.sync_all()?;
    drop(f);
    fs::rename(&tmp_path, &path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_json_roundtrip() {
        let s = EditorSessionState {
            format_version: FORMAT_VERSION,
            level_path: "C:\\proj\\level.json".into(),
            project_file_path: "C:\\proj\\project.vge".into(),
            main_tab: EditorMainTab::Level,
        };
        let json = serde_json::to_string_pretty(&s).unwrap();
        let back: EditorSessionState = serde_json::from_str(&json).unwrap();
        assert_eq!(back.format_version, s.format_version);
        assert_eq!(back.level_path, s.level_path);
        assert_eq!(back.project_file_path, s.project_file_path);
        assert_eq!(back.main_tab, s.main_tab);
    }

    #[test]
    fn future_format_version_is_detectable() {
        let v: serde_json::Value = serde_json::json!({
            "format_version": 999,
            "level_path": "",
            "project_file_path": "",
            "main_tab": "level"
        });
        let s: EditorSessionState = serde_json::from_value(v).unwrap();
        assert!(s.format_version > FORMAT_VERSION);
    }
}
