//! Chunked voxel worlds and sparse octree for static prefabs.

use glam::IVec3;
use std::collections::{HashMap, HashSet};

pub mod svo;

/// Chunk coordinate in chunk space.
pub type ChunkCoord = IVec3;

/// Edge length in voxels (e.g. 32).
#[derive(Clone, Debug)]
pub struct Chunk {
    pub edge: u32,
    /// Linear material ids: air = 0.
    pub data: Vec<u16>,
}

impl Chunk {
    pub fn new(edge: u32, fill: u16) -> Self {
        let n = (edge * edge * edge) as usize;
        Self {
            edge,
            data: vec![fill; n],
        }
    }

    pub fn index(&self, x: u32, y: u32, z: u32) -> usize {
        let e = self.edge as usize;
        (z as usize) * e * e + (y as usize) * e + (x as usize)
    }

    pub fn get(&self, x: u32, y: u32, z: u32) -> u16 {
        self.data[self.index(x, y, z)]
    }

    pub fn set(&mut self, x: u32, y: u32, z: u32, v: u16) {
        let i = self.index(x, y, z);
        self.data[i] = v;
    }

    /// Corner-centered scalar samples for meshing: (edge+1)³ floats, iso ~ 0.5.
    pub fn scalar_field_for_mesh(&self) -> Vec<f32> {
        let s = (self.edge + 1) as usize;
        let mut out = vec![0.0f32; s * s * s];
        let e = self.edge as usize;
        for z in 0..s {
            for y in 0..s {
                for x in 0..s {
                    let cx = x.min(e - 1);
                    let cy = y.min(e - 1);
                    let cz = z.min(e - 1);
                    let solid = self.data[self.index(cx as u32, cy as u32, cz as u32)] != 0;
                    out[z * s * s + y * s + x] = if solid { 1.0 } else { 0.0 };
                }
            }
        }
        out
    }

    /// Mark chunk dirty for mesh rebuild after edits.
    pub fn is_solid(&self, x: u32, y: u32, z: u32) -> bool {
        self.get(x, y, z) != 0
    }
}

#[derive(Default)]
pub struct ChunkWorld {
    pub chunks: HashMap<ChunkCoord, Chunk>,
    pub edge: u32,
}

impl ChunkWorld {
    pub fn new(edge: u32) -> Self {
        Self {
            chunks: HashMap::new(),
            edge,
        }
    }

    pub fn ensure_chunk(&mut self, c: ChunkCoord) -> &mut Chunk {
        self.chunks
            .entry(c)
            .or_insert_with(|| Chunk::new(self.edge, 0))
    }

    pub fn set_voxel_world(&mut self, wx: i32, wy: i32, wz: i32, mat: u16) {
        let e = self.edge as i32;
        let cx = wx.div_euclid(e);
        let cy = wy.div_euclid(e);
        let cz = wz.div_euclid(e);
        let c = IVec3::new(cx, cy, cz);
        let chunk = self.ensure_chunk(c);
        let lx = wx.rem_euclid(e) as u32;
        let ly = wy.rem_euclid(e) as u32;
        let lz = wz.rem_euclid(e) as u32;
        chunk.set(lx, ly, lz, mat);
    }

    /// Chunk coordinates within `radius` (Chebyshev) of camera chunk position.
    pub fn stream_chunks(center_chunk: IVec3, radius: i32) -> Vec<IVec3> {
        let mut v = Vec::new();
        for dz in -radius..=radius {
            for dy in -radius..=radius {
                for dx in -radius..=radius {
                    v.push(center_chunk + IVec3::new(dx, dy, dz));
                }
            }
        }
        v
    }

    pub fn dirty_around(center_chunk: IVec3, radius: i32) -> HashSet<ChunkCoord> {
        Self::stream_chunks(center_chunk, radius)
            .into_iter()
            .collect()
    }
}
