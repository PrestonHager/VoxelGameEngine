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

const FORMAT_VERSION: u32 = 1;
const APP_CONFIG_DIR: &str = "VoxelGameEngine";
const STATE_FILE: &str = "editor_state.json";

#[derive(Serialize, Deserialize)]
pub struct EditorSessionState {
    pub format_version: u32,
    pub level_path: String,
    pub main_tab: EditorMainTab,
}

/// Resolved config directory for this app, if the OS provides one.
pub fn config_dir_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join(APP_CONFIG_DIR))
}

pub fn state_file_path() -> Option<PathBuf> {
    config_dir_path().map(|d| d.join(STATE_FILE))
}

pub fn load() -> Option<EditorSessionState> {
    let path = state_file_path()?;
    let data = fs::read_to_string(&path).ok()?;
    serde_json::from_str(&data).ok()
}

/// Restore last tab and level path; loads the level from disk when the file exists.
pub fn apply_loaded_session(model: &mut EditorModel) {
    let Some(s) = load() else {
        return;
    };
    if s.format_version > FORMAT_VERSION {
        model.push_log(
            "Ignoring editor_state.json: file is from a newer editor version.".to_string(),
        );
        return;
    }

    model.main_tab = s.main_tab;

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
        main_tab: model.main_tab,
    };
    let json = serde_json::to_string_pretty(&state)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let mut f = fs::File::create(path)?;
    f.write_all(json.as_bytes())?;
    Ok(())
}
