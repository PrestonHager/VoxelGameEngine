//! Asset directory watching for hot reload (meshes, textures, scripts).

use crossbeam_channel::Receiver;
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::Path;
use thiserror::Error;
use tracing::warn;

#[derive(Debug, Error)]
pub enum AssetWatchError {
    #[error("notify error: {0}")]
    Notify(#[from] notify::Error),
}

pub struct HotReloader {
    _watcher: RecommendedWatcher,
    pub events: Receiver<notify::Event>,
}

impl HotReloader {
    /// Watch `root` recursively; coalesce events via crossbeam channel.
    pub fn watch(root: impl AsRef<Path>) -> Result<Self, AssetWatchError> {
        let (tx, rx) = crossbeam_channel::unbounded();
        let mut watcher = RecommendedWatcher::new(
            move |res| {
                if let Ok(ev) = res {
                    let _ = tx.send(ev);
                }
            },
            Config::default(),
        )?;
        watcher.watch(root.as_ref(), RecursiveMode::Recursive)?;
        Ok(Self {
            _watcher: watcher,
            events: rx,
        })
    }

    /// Drain pending events (non-blocking) for a simple game loop hook.
    pub fn poll_logs(&self) {
        while let Ok(ev) = self.events.try_recv() {
            warn!(?ev.paths, "asset change");
        }
    }
}
