//! Versioned binary IPC messages between editor and engine.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Protocol version; bump when breaking wire format.
pub const PROTOCOL_VERSION: u16 = 1;

/// Maximum total size of one IPC frame (matches typical engine `read` buffers, e.g. `ipc.rs`).
pub const MAX_IPC_FRAME_BYTES: usize = 64 * 1024;

/// Maximum payload length after the 6-byte header (`version` + `len`).
pub const MAX_FRAME_PAYLOAD_LEN: usize = MAX_IPC_FRAME_BYTES - 6;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EditorToEngine {
    Ping {
        nonce: u32,
    },
    LoadWorld {
        path: String,
    },
    /// Engine reads JSON (`scene::Level`) from this path and replaces the running scene.
    LoadLevelFromPath {
        path: String,
    },
    SetBlock {
        x: i32,
        y: i32,
        z: i32,
        material: u16,
    },
    Play,
    Stop,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EngineToEditor {
    Pong { nonce: u32 },
    Echo(String),
    LogLine(String),
}

#[derive(Debug, Error)]
pub enum ProtocolError {
    #[error("version mismatch: got {got}, expected {expected}")]
    VersionMismatch { got: u16, expected: u16 },
    #[error("truncated frame")]
    Truncated,
    #[error("frame payload length {len} exceeds maximum {max}")]
    FrameTooLarge { len: usize, max: usize },
    #[error("decode error: {0}")]
    Decode(#[from] bincode::Error),
}

fn ensure_payload_len(len: usize) -> Result<(), ProtocolError> {
    if len > MAX_FRAME_PAYLOAD_LEN {
        return Err(ProtocolError::FrameTooLarge {
            len,
            max: MAX_FRAME_PAYLOAD_LEN,
        });
    }
    Ok(())
}

/// Frame: [version: u16 BE][payload_len: u32 BE][bincode payload]
pub fn encode_engine_message(msg: &EngineToEditor) -> Result<Vec<u8>, ProtocolError> {
    let payload = bincode::serialize(msg)?;
    ensure_payload_len(payload.len())?;
    let mut out = Vec::with_capacity(2 + 4 + payload.len());
    out.extend_from_slice(&PROTOCOL_VERSION.to_be_bytes());
    out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    out.extend_from_slice(&payload);
    Ok(out)
}

pub fn decode_editor_message(data: &[u8]) -> Result<EditorToEngine, ProtocolError> {
    if data.len() < 6 {
        return Err(ProtocolError::Truncated);
    }
    let ver = u16::from_be_bytes([data[0], data[1]]);
    if ver != PROTOCOL_VERSION {
        return Err(ProtocolError::VersionMismatch {
            got: ver,
            expected: PROTOCOL_VERSION,
        });
    }
    let len = u32::from_be_bytes([data[2], data[3], data[4], data[5]]) as usize;
    if len > MAX_FRAME_PAYLOAD_LEN {
        return Err(ProtocolError::FrameTooLarge {
            len,
            max: MAX_FRAME_PAYLOAD_LEN,
        });
    }
    if data.len() < 6 + len {
        return Err(ProtocolError::Truncated);
    }
    let msg = bincode::deserialize(&data[6..6 + len])?;
    Ok(msg)
}

/// Decode engine->editor reply (same framing).
pub fn decode_engine_message(data: &[u8]) -> Result<EngineToEditor, ProtocolError> {
    if data.len() < 6 {
        return Err(ProtocolError::Truncated);
    }
    let ver = u16::from_be_bytes([data[0], data[1]]);
    if ver != PROTOCOL_VERSION {
        return Err(ProtocolError::VersionMismatch {
            got: ver,
            expected: PROTOCOL_VERSION,
        });
    }
    let len = u32::from_be_bytes([data[2], data[3], data[4], data[5]]) as usize;
    if len > MAX_FRAME_PAYLOAD_LEN {
        return Err(ProtocolError::FrameTooLarge {
            len,
            max: MAX_FRAME_PAYLOAD_LEN,
        });
    }
    if data.len() < 6 + len {
        return Err(ProtocolError::Truncated);
    }
    Ok(bincode::deserialize(&data[6..6 + len])?)
}

pub fn encode_editor_message(msg: &EditorToEngine) -> Result<Vec<u8>, ProtocolError> {
    let payload = bincode::serialize(msg)?;
    ensure_payload_len(payload.len())?;
    let mut out = Vec::with_capacity(2 + 4 + payload.len());
    out.extend_from_slice(&PROTOCOL_VERSION.to_be_bytes());
    out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    out.extend_from_slice(&payload);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ping_roundtrip() {
        let m = EditorToEngine::Ping { nonce: 7 };
        let b = encode_editor_message(&m).unwrap();
        assert_eq!(decode_editor_message(&b).unwrap(), m);
    }

    #[test]
    fn load_level_roundtrip() {
        let m = EditorToEngine::LoadLevelFromPath {
            path: "/tmp/level.json".into(),
        };
        let b = encode_editor_message(&m).unwrap();
        assert_eq!(decode_editor_message(&b).unwrap(), m);
    }

    #[test]
    fn load_world_roundtrip() {
        let m = EditorToEngine::LoadWorld {
            path: "world.bin".into(),
        };
        let b = encode_editor_message(&m).unwrap();
        assert_eq!(decode_editor_message(&b).unwrap(), m);
    }

    #[test]
    fn version_mismatch() {
        let mut b = encode_editor_message(&EditorToEngine::Ping { nonce: 1 }).unwrap();
        b[0] = 0xff;
        b[1] = 0xff;
        match decode_editor_message(&b) {
            Err(ProtocolError::VersionMismatch { got, expected }) => {
                assert_eq!(expected, PROTOCOL_VERSION);
                assert_ne!(got, PROTOCOL_VERSION);
            }
            other => panic!("expected VersionMismatch, got {other:?}"),
        }
    }

    #[test]
    fn truncated_header() {
        assert!(matches!(
            decode_editor_message(&[1, 0]),
            Err(ProtocolError::Truncated)
        ));
    }

    #[test]
    fn truncated_payload() {
        let mut b = encode_editor_message(&EditorToEngine::Ping { nonce: 3 }).unwrap();
        b.truncate(b.len() - 1);
        assert!(matches!(
            decode_editor_message(&b),
            Err(ProtocolError::Truncated)
        ));
    }

    #[test]
    fn declared_len_larger_than_buffer() {
        // Valid header claims a huge payload; buffer is shorter than 6 + len (and len is capped).
        let mut buf = vec![0u8; 32];
        buf[0..2].copy_from_slice(&PROTOCOL_VERSION.to_be_bytes());
        buf[2..6].copy_from_slice(&(1000u32).to_be_bytes());
        assert!(matches!(
            decode_editor_message(&buf),
            Err(ProtocolError::Truncated)
        ));
    }

    #[test]
    fn frame_len_exceeds_max() {
        let mut buf = vec![0u8; 12];
        buf[0..2].copy_from_slice(&PROTOCOL_VERSION.to_be_bytes());
        buf[2..6].copy_from_slice(&((MAX_FRAME_PAYLOAD_LEN + 1) as u32).to_be_bytes());
        assert!(matches!(
            decode_editor_message(&buf),
            Err(ProtocolError::FrameTooLarge { .. })
        ));
    }

    #[test]
    fn engine_message_roundtrip() {
        let m = EngineToEditor::Pong { nonce: 99 };
        let b = encode_engine_message(&m).unwrap();
        assert_eq!(decode_engine_message(&b).unwrap(), m);
    }
}
