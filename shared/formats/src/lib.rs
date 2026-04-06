//! Serialized voxel and asset descriptors (on-disk / network).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkDescriptor {
    pub cx: i32,
    pub cy: i32,
    pub cz: i32,
    pub edge: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetId(pub u64);

/// Placeholder for future networked replication descriptors (Phase 5).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReplicationStub {
    pub owner_client: Option<u32>,
}
