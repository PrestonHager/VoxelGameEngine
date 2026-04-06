//! Headless editor stub: sends IPC ping to engine (`VGE_IPC_PORT` on runner).

use protocol::{decode_engine_message, encode_editor_message, EditorToEngine};
use std::io::{Read, Write};
use std::net::TcpStream;
use tracing::info;

fn main() {
    logging::init();
    let port: u16 = std::env::var("VGE_IPC_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(7878);
    let addr = format!("127.0.0.1:{port}");
    let mut stream = match TcpStream::connect(&addr) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("connect {addr}: {e} (start engine-runner with VGE_IPC_PORT={port})");
            return;
        }
    };

    let msg = EditorToEngine::Ping { nonce: 42 };
    let bytes = encode_editor_message(&msg).expect("encode");
    stream.write_all(&bytes).expect("write");

    let mut buf = vec![0u8; 4096];
    let n = stream.read(&mut buf).expect("read");
    let reply = decode_engine_message(&buf[..n]).expect("decode");
    info!(?reply, "engine reply");
}
