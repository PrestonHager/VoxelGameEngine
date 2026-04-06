//! Versioned binary IPC messages between editor and engine.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Protocol version; bump when breaking wire format.
pub const PROTOCOL_VERSION: u16 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EditorToEngine {
    Ping {
        nonce: u32,
    },
    LoadWorld {
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
    #[error("decode error: {0}")]
    Decode(#[from] bincode::Error),
}

/// Frame: [version: u16 BE][payload_len: u32 BE][bincode payload]
pub fn encode_engine_message(msg: &EngineToEditor) -> Result<Vec<u8>, ProtocolError> {
    let payload = bincode::serialize(msg)?;
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
    if data.len() < 6 + len {
        return Err(ProtocolError::Truncated);
    }
    Ok(bincode::deserialize(&data[6..6 + len])?)
}

pub fn encode_editor_message(msg: &EditorToEngine) -> Result<Vec<u8>, ProtocolError> {
    let payload = bincode::serialize(msg)?;
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
}
