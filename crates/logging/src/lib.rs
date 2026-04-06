//! Engine-wide `tracing` initialization.

use tracing_subscriber::{fmt, prelude::*, EnvFilter};

/// Install a default subscriber (respects `RUST_LOG`).
pub fn init() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer())
        .init();
}
