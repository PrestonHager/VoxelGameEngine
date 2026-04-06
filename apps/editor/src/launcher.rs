//! Locate `engine-runner`, spawn it with `VGE_IPC_PORT`, wait for the TCP listener.

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

/// Resolve path to `engine-runner` next to this binary, or `VGE_ENGINE_EXE`, or `PATH`.
pub fn engine_runner_path() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("VGE_ENGINE_EXE") {
        let path = PathBuf::from(p);
        if path.is_file() {
            return Some(path);
        }
    }
    let dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
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

/// Spawn engine-runner with the given port; returns `None` if the executable was not found.
pub fn spawn_engine(port: u16) -> Result<Child, String> {
    let exe = engine_runner_path().ok_or_else(|| {
        "engine-runner not found next to editor and VGE_ENGINE_EXE unset".to_string()
    })?;
    Command::new(&exe)
        .env("VGE_IPC_PORT", port.to_string())
        .spawn()
        .map_err(|e| format!("spawn {}: {e}", exe.display()))
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

pub fn ping_engine(port: u16) -> Result<String, String> {
    let mut stream =
        TcpStream::connect_timeout(&ipc_addr(port), Duration::from_secs(2)).map_err(|e| {
            format!("connect {}: {e}", ipc_addr(port))
        })?;
    let msg = EditorToEngine::Ping { nonce: 42 };
    let bytes = encode_editor_message(&msg).map_err(|e| e.to_string())?;
    stream
        .write_all(&bytes)
        .map_err(|e| format!("write: {e}"))?;
    let mut buf = vec![0u8; 4096];
    let n = stream
        .read(&mut buf)
        .map_err(|e| format!("read: {e}"))?;
    let reply = decode_engine_message(&buf[..n]).map_err(|e| e.to_string())?;
    Ok(format!("{reply:?}"))
}
