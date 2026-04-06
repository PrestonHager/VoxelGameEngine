//! Mesh extraction: dual-contouring-oriented scalar pipeline + marching cubes isosurface.

mod marching_cubes;

pub mod dual_contouring;

pub use dual_contouring::MeshBuffers;

use glam::Vec3;
use voxel::Chunk;

/// Greedy axis-aligned mesh for blocky chunks (debug / fast path).
pub fn greedy_block_mesh(chunk: &Chunk) -> (Vec<Vec3>, Vec<u32>) {
    let e = chunk.edge as i32;
    let mut verts = Vec::new();
    let mut indices = Vec::new();

    let solid = |x: i32, y: i32, z: i32| -> bool {
        if x < 0 || y < 0 || z < 0 || x >= e || y >= e || z >= e {
            return false;
        }
        chunk.get(x as u32, y as u32, z as u32) != 0
    };

    let mut emit_quad = |p0: Vec3, p1: Vec3, p2: Vec3, p3: Vec3| {
        let base = verts.len() as u32;
        verts.push(p0);
        verts.push(p1);
        verts.push(p2);
        verts.push(p3);
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    };

    for z in 0..e {
        for y in 0..e {
            for x in 0..e {
                if !solid(x, y, z) {
                    continue;
                }
                let ox = x as f32;
                let oy = y as f32;
                let oz = z as f32;
                if !solid(x - 1, y, z) {
                    emit_quad(
                        Vec3::new(ox, oy, oz),
                        Vec3::new(ox, oy + 1.0, oz),
                        Vec3::new(ox, oy + 1.0, oz + 1.0),
                        Vec3::new(ox, oy, oz + 1.0),
                    );
                }
                if !solid(x + 1, y, z) {
                    emit_quad(
                        Vec3::new(ox + 1.0, oy, oz + 1.0),
                        Vec3::new(ox + 1.0, oy + 1.0, oz + 1.0),
                        Vec3::new(ox + 1.0, oy + 1.0, oz),
                        Vec3::new(ox + 1.0, oy, oz),
                    );
                }
                if !solid(x, y - 1, z) {
                    emit_quad(
                        Vec3::new(ox, oy, oz + 1.0),
                        Vec3::new(ox + 1.0, oy, oz + 1.0),
                        Vec3::new(ox + 1.0, oy, oz),
                        Vec3::new(ox, oy, oz),
                    );
                }
                if !solid(x, y + 1, z) {
                    emit_quad(
                        Vec3::new(ox, oy + 1.0, oz),
                        Vec3::new(ox + 1.0, oy + 1.0, oz),
                        Vec3::new(ox + 1.0, oy + 1.0, oz + 1.0),
                        Vec3::new(ox, oy + 1.0, oz + 1.0),
                    );
                }
                if !solid(x, y, z - 1) {
                    emit_quad(
                        Vec3::new(ox + 1.0, oy, oz),
                        Vec3::new(ox + 1.0, oy + 1.0, oz),
                        Vec3::new(ox, oy + 1.0, oz),
                        Vec3::new(ox, oy, oz),
                    );
                }
                if !solid(x, y, z + 1) {
                    emit_quad(
                        Vec3::new(ox, oy, oz + 1.0),
                        Vec3::new(ox, oy + 1.0, oz + 1.0),
                        Vec3::new(ox + 1.0, oy + 1.0, oz + 1.0),
                        Vec3::new(ox + 1.0, oy, oz + 1.0),
                    );
                }
            }
        }
    }

    (verts, indices)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dual_contouring::{extract_from_chunk_scalar, MeshBuffers};
    use voxel::Chunk;

    #[test]
    fn greedy_nonempty_on_floor() {
        let mut c = Chunk::new(4, 0);
        c.set(1, 0, 1, 1);
        let (v, i) = greedy_block_mesh(&c);
        assert!(!v.is_empty() && !i.is_empty());
    }

    #[test]
    fn scalar_extract_runs() {
        let mut c = Chunk::new(8, 0);
        c.set(2, 0, 2, 1);
        let mut buf = MeshBuffers::default();
        extract_from_chunk_scalar(&c, 0.5, &mut buf);
        // May be empty for tiny islands; ensure no panic
        let _ = buf.positions.len();
    }
}
