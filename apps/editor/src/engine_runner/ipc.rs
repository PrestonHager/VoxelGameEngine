//! Background TCP listener for editor IPC (versioned `protocol` frames).

use protocol::{
    decode_editor_message, encode_engine_message, EditorToEngine, EngineToEditor,
    MAX_IPC_FRAME_BYTES,
};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::mpsc::Sender;
use tracing::{error, info};

/// Operations the engine main thread should apply (IPC thread only enqueues).
#[derive(Debug)]
pub enum EngineIpcOp {
    LoadLevelFromPath(String),
}

pub fn spawn_listener(port: u16, tx: Sender<EngineIpcOp>) {
    std::thread::spawn(move || {
        let addr = format!("127.0.0.1:{port}");
        let listener = match TcpListener::bind(&addr) {
            Ok(l) => l,
            Err(e) => {
                error!("IPC bind {addr}: {e}");
                return;
            }
        };
        info!("IPC listening on {addr}");
        for stream in listener.incoming().flatten() {
            let mut stream = stream;
            let mut buf = vec![0u8; MAX_IPC_FRAME_BYTES];
            let n = match stream.read(&mut buf) {
                Ok(n) => n,
                Err(e) => {
                    error!("IPC read: {e}");
                    continue;
                }
            };
            if n < 6 {
                continue;
            }
            let reply = match decode_editor_message(&buf[..n]) {
                Ok(EditorToEngine::Ping { nonce }) => EngineToEditor::Pong { nonce },
                Ok(EditorToEngine::LoadLevelFromPath { path }) => {
                    if tx.send(EngineIpcOp::LoadLevelFromPath(path)).is_err() {
                        EngineToEditor::LogLine("engine shutting down".into())
                    } else {
                        EngineToEditor::Echo("load_level_queued".into())
                    }
                }
                Ok(m) => EngineToEditor::Echo(format!("{m:?}")),
                Err(e) => EngineToEditor::LogLine(format!("decode: {e}")),
            };
            if let Ok(bytes) = encode_engine_message(&reply) {
                let _ = stream.write_all(&bytes);
            }
        }
    });
}
