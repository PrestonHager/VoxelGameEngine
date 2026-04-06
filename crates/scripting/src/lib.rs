//! Pluggable scripting: Lua (`mlua`) host; file watching for hot reload.
//!
//! See [`hooks::ScriptHost`] for ECS-bound instance hooks and the `vge`-style API table.

mod hooks;

pub use hooks::{CursorCommands, ScriptHost, ScriptInput};

use mlua::Lua;
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::Path;
use thiserror::Error;
use tracing::{error, info};

#[derive(Debug, Error)]
pub enum ScriptError {
    #[error("lua: {0}")]
    Lua(#[from] mlua::Error),
    #[error("notify: {0}")]
    Notify(#[from] notify::Error),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

pub trait ScriptBackend {
    fn eval_chunk(&self, source: &str) -> Result<(), ScriptError>;
}

pub struct LuaBackend {
    lua: Lua,
}

impl LuaBackend {
    pub fn new() -> Self {
        Self { lua: Lua::new() }
    }
}

impl Default for LuaBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl ScriptBackend for LuaBackend {
    fn eval_chunk(&self, source: &str) -> Result<(), ScriptError> {
        match self.lua.load(source).exec() {
            Ok(()) => Ok(()),
            Err(e) => {
                error!("Lua error (isolated): {e}");
                Ok(())
            }
        }
    }
}

/// Run a `.lua` file; call again after disk change for hot reload.
pub fn run_lua_file(backend: &LuaBackend, path: &Path) -> Result<(), ScriptError> {
    let src = std::fs::read_to_string(path)?;
    info!("loading script {}", path.display());
    backend.eval_chunk(&src)
}

/// Keep watcher alive; logs paths on change (wire to `run_lua_file` in engine).
pub struct ScriptHotWatch {
    _watcher: RecommendedWatcher,
}

impl ScriptHotWatch {
    pub fn new(dir: impl AsRef<Path>, root_script: impl AsRef<Path>) -> Result<Self, ScriptError> {
        let script_path = root_script.as_ref().to_path_buf();
        let mut watcher = RecommendedWatcher::new(
            move |res: notify::Result<notify::Event>| {
                if let Ok(ev) = res {
                    if ev
                        .paths
                        .iter()
                        .any(|p| p.extension().and_then(|s| s.to_str()) == Some("lua"))
                    {
                        info!("script reload: {}", script_path.display());
                        let _ = run_lua_file(&LuaBackend::new(), &script_path);
                    }
                }
            },
            Config::default(),
        )?;
        watcher.watch(dir.as_ref(), RecursiveMode::Recursive)?;
        Ok(Self { _watcher: watcher })
    }
}
