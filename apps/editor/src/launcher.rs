//! Spawn the external engine host: same binary as the editor with `engine-runner` subcommand
//! (`editor engine-runner`), with `VGE_IPC_PORT` set. Waits for the TCP listener.

use protocol::{decode_engine_message, encode_editor_message, EditorToEngine};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Command};
use std::thread;
use std::time::{Duration, Instant};

const CONNECT_TIMEOUT: Duration = Duration::from_millis(200);
const SPAWN_WAIT: Duration = Duration::from_secs(15);
const POLL: Duration = Duration::from_millis(80);

pub fn ipc_addr(port: u16) -> SocketAddr {
    SocketAddr::from(([127, 0, 0, 1], port))
}

/// Returns true if something accepts TCP on `port` (engine IPC thread is up).
pub fn engine_listening(port: u16) -> bool {
    TcpStream::connect_timeout(&ipc_addr(port), CONNECT_TIMEOUT).is_ok()
}

/// Resolve the executable used to run the external engine host.
///
/// Prefer **`VGE_ENGINE_EXE`** if set, then **this binary** (`editor` — use `engine-runner`
/// as the first CLI argument), then a legacy **`engine-runner`** file next to the editor.
pub fn engine_runner_path() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("VGE_ENGINE_EXE") {
        let path = PathBuf::from(p);
        if path.is_file() {
            return Some(path);
        }
    }
    let this = std::env::current_exe().ok()?;
    if this.is_file() {
        return Some(this);
    }
    let dir = this.parent()?.to_path_buf();
    let names: &[&str] = if cfg!(windows) {
        &["engine-runner.exe"]
    } else {
        &["engine-runner"]
    };
    for name in names {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Spawn the engine host with the given port; returns `None` if the executable was not found.
pub fn spawn_engine(port: u16) -> Result<Child, String> {
    let exe = engine_runner_path().ok_or_else(|| {
        "could not resolve engine executable (editor binary or VGE_ENGINE_EXE)".to_string()
    })?;
    let mut cmd = Command::new(&exe);
    cmd.env("VGE_IPC_PORT", port.to_string());
    // Unified binary is `editor` / `editor.exe`; legacy standalone was `engine-runner`.
    if is_unified_editor_binary(&exe) {
        cmd.arg("engine-runner");
    }
    cmd.spawn()
        .map_err(|e| format!("spawn {}: {e}", exe.display()))
}

fn is_unified_editor_binary(exe: &std::path::Path) -> bool {
    exe.file_stem()
        .and_then(|s| s.to_str())
        .map(|s| s.eq_ignore_ascii_case("editor"))
        .unwrap_or(false)
}

/// Block until `engine_listening` or timeout after spawn.
pub fn wait_for_engine(port: u16, deadline: Duration) -> bool {
    let end = Instant::now() + deadline;
    while Instant::now() < end {
        if engine_listening(port) {
            return true;
        }
        thread::sleep(POLL);
    }
    false
}

/// Spawn if needed, then wait for port.
pub fn ensure_engine_running(port: u16) -> Result<Option<Child>, String> {
    if engine_listening(port) {
        return Ok(None);
    }
    let mut child = spawn_engine(port)?;
    if wait_for_engine(port, SPAWN_WAIT) {
        Ok(Some(child))
    } else {
        let result = child.kill();
        if let Err(e) = result {
            eprintln!("kill engine-runner: {e}");
        }
        let msg = format!(
            "engine-runner did not open port {port} within {:?}",
            SPAWN_WAIT
        );
        eprintln!("{msg}");
        Err(msg)
    }
}

fn exchange(port: u16, msg: &EditorToEngine) -> Result<String, String> {
    let mut stream = TcpStream::connect_timeout(&ipc_addr(port), Duration::from_secs(2))
        .map_err(|e| format!("connect {}: {e}", ipc_addr(port)))?;
    let bytes = encode_editor_message(msg).map_err(|e| e.to_string())?;
    stream
        .write_all(&bytes)
        .map_err(|e| format!("write: {e}"))?;
    let mut buf = vec![0u8; 4096];
    let n = stream.read(&mut buf).map_err(|e| format!("read: {e}"))?;
    let reply = decode_engine_message(&buf[..n]).map_err(|e| e.to_string())?;
    Ok(format!("{reply:?}"))
}

pub fn ping_engine(port: u16) -> Result<String, String> {
    exchange(port, &EditorToEngine::Ping { nonce: 42 })
}

/// Save JSON level path the engine can read (use an absolute path when possible).
pub fn push_level_path(port: u16, path: &str) -> Result<String, String> {
    exchange(
        port,
        &EditorToEngine::LoadLevelFromPath {
            path: path.to_string(),
        },
    )
}
