//! Sparse voxel octree for static prefabs (naive mesh path via leaf enumeration).

use glam::Vec3;

#[derive(Clone, Debug)]
pub enum SvoNode {
    Internal([Box<SvoNode>; 8]),
    Leaf(u16), // 0 = empty
}

impl Default for SvoNode {
    fn default() -> Self {
        SvoNode::Leaf(0)
    }
}

#[derive(Clone, Debug)]
pub struct SparseVoxelOctree {
    pub root: SvoNode,
    pub depth: u8,
}

impl SparseVoxelOctree {
    pub fn new(depth: u8) -> Self {
        Self {
            root: SvoNode::Leaf(0),
            depth,
        }
    }

    /// Set material at integer coords in [0, 2^depth).
    pub fn set(&mut self, x: u32, y: u32, z: u32, mat: u16) {
        let d = self.depth as u32;
        let max = 1u32 << d;
        assert!(x < max && y < max && z < max);
        Self::set_rec(&mut self.root, d, 0, 0, 0, max, x, y, z, mat);
    }

    #[allow(clippy::too_many_arguments)]
    fn set_rec(
        node: &mut SvoNode,
        level: u32,
        ox: u32,
        oy: u32,
        oz: u32,
        size: u32,
        x: u32,
        y: u32,
        z: u32,
        mat: u16,
    ) {
        if level == 0 {
            *node = SvoNode::Leaf(mat);
            return;
        }
        let h = size / 2;
        let mut ix = 0u8;
        if x >= ox + h {
            ix |= 1;
        }
        if y >= oy + h {
            ix |= 2;
        }
        if z >= oz + h {
            ix |= 4;
        }
        let (nox, noy, noz) = match ix {
            0 => (ox, oy, oz),
            1 => (ox + h, oy, oz),
            2 => (ox, oy + h, oz),
            3 => (ox + h, oy + h, oz),
            4 => (ox, oy, oz + h),
            5 => (ox + h, oy, oz + h),
            6 => (ox, oy + h, oz + h),
            _ => (ox + h, oy + h, oz + h),
        };

        if matches!(node, SvoNode::Leaf(_)) {
            let leaf_mat = match node {
                SvoNode::Leaf(m) => *m,
                _ => 0,
            };
            *node = SvoNode::Internal(std::array::from_fn(|_| Box::new(SvoNode::Leaf(leaf_mat))));
        }

        if let SvoNode::Internal(children) = node {
            let i = ix as usize;
            Self::set_rec(&mut children[i], level - 1, nox, noy, noz, h, x, y, z, mat);
        }
    }

    pub fn get(&self, x: u32, y: u32, z: u32) -> u16 {
        let d = self.depth as u32;
        let max = 1u32 << d;
        assert!(x < max && y < max && z < max);
        Self::get_rec(&self.root, d, 0, 0, 0, max, x, y, z)
    }

    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::only_used_in_recursion)]
    fn get_rec(
        node: &SvoNode,
        level: u32,
        ox: u32,
        oy: u32,
        oz: u32,
        size: u32,
        x: u32,
        y: u32,
        z: u32,
    ) -> u16 {
        match node {
            SvoNode::Leaf(m) => *m,
            SvoNode::Internal(ch) => {
                let h = size / 2;
                let mut ix = 0usize;
                if x >= ox + h {
                    ix |= 1;
                }
                if y >= oy + h {
                    ix |= 2;
                }
                if z >= oz + h {
                    ix |= 4;
                }
                let (nox, noy, noz) = match ix {
                    0 => (ox, oy, oz),
                    1 => (ox + h, oy, oz),
                    2 => (ox, oy + h, oz),
                    3 => (ox + h, oy + h, oz),
                    4 => (ox, oy, oz + h),
                    5 => (ox + h, oy, oz + h),
                    6 => (ox, oy + h, oz + h),
                    _ => (ox + h, oy + h, oz + h),
                };
                Self::get_rec(&ch[ix], level - 1, nox, noy, noz, h, x, y, z)
            }
        }
    }

    /// Naive surface: emit axis-aligned quad centers for occupied voxels adjacent to empty.
    pub fn naive_surface_positions(&self) -> Vec<Vec3> {
        let d = self.depth as u32;
        let max = 1u32 << d;
        let mut out = Vec::new();
        for z in 0..max {
            for y in 0..max {
                for x in 0..max {
                    let m = self.get(x, y, z);
                    if m == 0 {
                        continue;
                    }
                    let p = Vec3::new(x as f32, y as f32, z as f32);
                    if x == 0 || self.get(x - 1, y, z) == 0 {
                        out.push(p + Vec3::new(-0.5, 0.0, 0.0));
                    }
                    if x + 1 == max || self.get(x + 1, y, z) == 0 {
                        out.push(p + Vec3::new(0.5, 0.0, 0.0));
                    }
                    if y == 0 || self.get(x, y - 1, z) == 0 {
                        out.push(p + Vec3::new(0.0, -0.5, 0.0));
                    }
                    if y + 1 == max || self.get(x, y + 1, z) == 0 {
                        out.push(p + Vec3::new(0.0, 0.5, 0.0));
                    }
                    if z == 0 || self.get(x, y, z - 1) == 0 {
                        out.push(p + Vec3::new(0.0, 0.0, -0.5));
                    }
                    if z + 1 == max || self.get(x, y, z + 1) == 0 {
                        out.push(p + Vec3::new(0.0, 0.0, 0.5));
                    }
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_get_roundtrip() {
        let mut t = SparseVoxelOctree::new(3);
        t.set(1, 2, 3, 9);
        assert_eq!(t.get(1, 2, 3), 9);
    }
}
