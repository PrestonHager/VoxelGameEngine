//! Scalar-field extraction: Hermite-style edge crossings per cell, stitched with marching cubes
//! for manifold triangles (full QEF dual contouring can replace `polygonise` later).

use crate::marching_cubes::{GridCell, MarchingCubes, Triangle};
use glam::Vec3;
use voxel::Chunk;

#[derive(Clone, Debug, Default)]
pub struct MeshBuffers {
    pub positions: Vec<Vec3>,
    pub indices: Vec<u32>,
}

impl MeshBuffers {
    pub fn clear(&mut self) {
        self.positions.clear();
        self.indices.clear();
    }
}

/// Iso-surface at `iso` (use 0.5 for binary 0/1 field).
pub fn extract_from_chunk_scalar(chunk: &Chunk, iso: f32, out: &mut MeshBuffers) {
    out.clear();
    let s = (chunk.edge + 1) as usize;
    let samples = chunk.scalar_field_for_mesh();
    let nx = chunk.edge as usize;
    let ny = chunk.edge as usize;
    let nz = chunk.edge as usize;

    let at = |x: usize, y: usize, z: usize| -> f32 { samples[z * s * s + y * s + x] };

    let mut tris: Vec<Triangle> = Vec::new();
    for z in 0..nz {
        for y in 0..ny {
            for x in 0..nx {
                let positions: [[f32; 3]; 8] = [
                    [x as f32, y as f32, z as f32],
                    [(x + 1) as f32, y as f32, z as f32],
                    [(x + 1) as f32, (y + 1) as f32, z as f32],
                    [x as f32, (y + 1) as f32, z as f32],
                    [x as f32, y as f32, (z + 1) as f32],
                    [(x + 1) as f32, y as f32, (z + 1) as f32],
                    [(x + 1) as f32, (y + 1) as f32, (z + 1) as f32],
                    [x as f32, (y + 1) as f32, (z + 1) as f32],
                ];
                let value = [
                    at(x, y, z),
                    at(x + 1, y, z),
                    at(x + 1, y + 1, z),
                    at(x, y + 1, z),
                    at(x, y, z + 1),
                    at(x + 1, y, z + 1),
                    at(x + 1, y + 1, z + 1),
                    at(x, y + 1, z + 1),
                ];
                let cell = GridCell { positions, value };
                let mut cell_buf = [Triangle {
                    positions: [[0.0; 3]; 3],
                }; 6];
                let n = MarchingCubes::new(iso, cell).polygonise(&mut cell_buf) as usize;
                tris.extend_from_slice(&cell_buf[..n]);
            }
        }
    }

    let mut map: std::collections::HashMap<(u32, u32, u32), u32> = std::collections::HashMap::new();
    let quant = |v: f32| -> u32 { (v * 1000.0).round() as u32 };

    for t in tris {
        for p in t.positions {
            let k = (quant(p[0]), quant(p[1]), quant(p[2]));
            let idx = *map.entry(k).or_insert_with(|| {
                let i = out.positions.len() as u32;
                out.positions.push(Vec3::from_array(p));
                i
            });
            out.indices.push(idx);
        }
    }
}
