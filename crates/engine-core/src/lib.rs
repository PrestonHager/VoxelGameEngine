//! Application loop: fixed-step ECS tick, camera, optional voxel mesh sampling for instances.

use ecs::{CameraRig, Position, PrefabRef, Velocity, World};
use glam::{IVec3, Mat4, Vec3};
use meshing::dual_contouring::{extract_from_chunk_scalar, MeshBuffers};
use physics::PhysicsWorld;
use scene::{CameraAuthoring, Level, PlacedObject, TerrainLayer, TerrainMode};
use scripting::ScriptHost;
use std::collections::HashMap;
use std::path::Path;
use voxel::{Chunk, ChunkWorld};

const FIXED_DT: f32 = 1.0 / 60.0;
const MAX_INSTANCES: usize = 16_384;

pub struct EngineState {
    pub world: World,
    pub voxel_world: ChunkWorld,
    pub physics: PhysicsWorld,
    pub mesh_scratch: MeshBuffers,
    /// Free-flight fallback when no active `Camera` prefab exists.
    pub camera_pos: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub time: f32,
    /// Maps level `instance_id` → ECS entity (for Lua hooks).
    pub entity_by_instance: HashMap<u64, ecs::Entity>,
    pub script: Option<ScriptHost>,
}

fn apply_terrain_layer(vw: &mut ChunkWorld, terrain: &TerrainLayer) {
    match terrain.mode {
        TerrainMode::Flat => {
            let mat = terrain.surface_material.max(1);
            let top = terrain.base_height_voxels.max(0);
            for x in 0..32i32 {
                for z in 0..32i32 {
                    for y in 0..=top {
                        vw.set_voxel_world(x, y, z, mat);
                    }
                }
            }
            for x in 4..12 {
                for z in 4..12 {
                    let wy = top + 1;
                    vw.set_voxel_world(x, wy, z, mat);
                    if (x + z) % 3 == 0 {
                        vw.set_voxel_world(x, wy + 1, z, mat);
                    }
                }
            }
        }
    }
}

impl EngineState {
    pub fn from_level(level: &Level) -> Self {
        Self::from_level_with_asset_root(level, None)
    }

    pub fn from_level_with_asset_root(level: &Level, asset_root: Option<&Path>) -> Self {
        let mut world = World::default();
        let mut entity_by_instance = HashMap::new();

        for o in &level.objects {
            if !o.visible {
                continue;
            }
            let e = if o.prefab_id == scene::ids::CAMERA {
                let rig = o
                    .camera
                    .as_ref()
                    .map(|c| CameraRig {
                        fov_deg: c.fov_deg,
                        near: 0.1,
                        far: 200.0,
                        yaw: c.yaw_deg.to_radians(),
                        pitch: c.pitch_deg.to_radians(),
                        active: c.active,
                    })
                    .unwrap_or_default();
                world.spawn_camera(Position(Vec3::from_array(o.position)), rig)
            } else {
                world.spawn_prefab(
                    Position(Vec3::from_array(o.position)),
                    PrefabRef(o.prefab_id),
                )
            };
            entity_by_instance.insert(o.instance_id, e);
        }

        let mut vw = ChunkWorld::new(16);
        apply_terrain_layer(&mut vw, &level.terrain);

        let script = ScriptHost::from_level_with_base(level, asset_root);

        Self {
            world,
            voxel_world: vw,
            physics: PhysicsWorld::demo_stack(),
            mesh_scratch: MeshBuffers::default(),
            camera_pos: Vec3::new(12.0, 14.0, 18.0),
            yaw: -0.55,
            pitch: -0.35,
            time: 0.0,
            entity_by_instance,
            script,
        }
    }

    pub fn apply_level(&mut self, level: &Level) {
        *self = Self::from_level_with_asset_root(level, None);
    }

    pub fn apply_level_with_asset_root(&mut self, level: &Level, asset_root: Option<&Path>) {
        *self = Self::from_level_with_asset_root(level, asset_root);
    }
}

impl Default for EngineState {
    fn default() -> Self {
        let mut level = Level {
            name: "Demo".into(),
            ..Default::default()
        };
        level.objects.push(PlacedObject {
            instance_id: 10,
            prefab_id: scene::ids::CAMERA,
            name: "Main camera".into(),
            position: [12.0, 14.0, 18.0],
            visible: true,
            camera: Some(CameraAuthoring {
                fov_deg: 45.0,
                yaw_deg: -31.5,
                pitch_deg: -20.0,
                active: true,
            }),
            script_asset_id: None,
        });
        level.objects.push(PlacedObject {
            instance_id: 1,
            prefab_id: scene::ids::SPAWN_POINT,
            name: "Spawn".into(),
            position: [3.0, 1.0, 2.0],
            visible: true,
            camera: None,
            script_asset_id: None,
        });
        let mut s = Self::from_level(&level);
        s.world.spawn_with(
            Position(Vec3::new(0.0, 0.0, 0.0)),
            Some(Velocity(Vec3::new(0.2, 0.1, 0.0))),
        );
        s
    }
}

impl EngineState {
    pub fn tick(&mut self) {
        self.world.system_integrate(FIXED_DT);
        self.physics.step();
        self.time += FIXED_DT;
        if let Some(s) = &self.script {
            let map = &self.entity_by_instance;
            if let Err(e) = s.tick(&mut self.world, map, FIXED_DT) {
                tracing::warn!(target = "script", "lua tick: {e}");
            }
        }
    }

    /// First active camera rig in the world, if any.
    pub fn active_camera_view(&self, aspect: f32) -> Option<Mat4> {
        for (_, p, c) in self.world.camera_views() {
            if !c.active {
                continue;
            }
            let proj = Mat4::perspective_rh(c.fov_deg.to_radians(), aspect, c.near, c.far);
            let (sy, cy) = c.pitch.sin_cos();
            let (sx, cx) = c.yaw.sin_cos();
            let forward = Vec3::new(cx * cy, sy, sx * cy);
            let target = p.0 + forward;
            let view = Mat4::look_at_rh(p.0, target, Vec3::Y);
            return Some(proj * view);
        }
        None
    }

    pub fn view_projection(&self, aspect: f32) -> Mat4 {
        self.active_camera_view(aspect).unwrap_or_else(|| {
            let proj = Mat4::perspective_rh(45f32.to_radians(), aspect, 0.1, 200.0);
            let (sy, cy) = self.pitch.sin_cos();
            let (sx, cx) = self.yaw.sin_cos();
            let forward = Vec3::new(cx * cy, sy, sx * cy);
            let target = self.camera_pos + forward;
            let view = Mat4::look_at_rh(self.camera_pos, target, Vec3::Y);
            proj * view
        })
    }

    pub fn camera_sample_position(&self) -> Vec3 {
        self.world
            .camera_views()
            .find(|(_, _, c)| c.active)
            .map(|(_, p, _)| p.0)
            .unwrap_or(self.camera_pos)
    }

    pub fn voxel_instances_for_stream(&mut self) -> Vec<[f32; 3]> {
        let cam = self.camera_sample_position();
        let e = self.voxel_world.edge as i32;
        let cc = IVec3::new(
            (cam.x / e as f32).floor() as i32,
            (cam.y / e as f32).floor() as i32,
            (cam.z / e as f32).floor() as i32,
        );
        let _near = voxel::ChunkWorld::stream_chunks(cc, 1);

        let chunk = self
            .voxel_world
            .chunks
            .get(&cc)
            .cloned()
            .unwrap_or_else(|| {
                let mut c = Chunk::new(self.voxel_world.edge, 0);
                let n = c.edge.min(10);
                for z in 0..n {
                    for x in 0..n {
                        c.set(x, 0, z, 1);
                    }
                }
                c
            });

        extract_from_chunk_scalar(&chunk, 0.5, &mut self.mesh_scratch);

        let origin = Vec3::new((cc.x * e) as f32, (cc.y * e) as f32, (cc.z * e) as f32);

        let mut inst: Vec<[f32; 3]> = self
            .world
            .positions()
            .map(|(_, p)| [p.0.x, p.0.y, p.0.z])
            .collect();

        let step = (self.mesh_scratch.positions.len() / 256).max(1);
        for (i, p) in self.mesh_scratch.positions.iter().enumerate() {
            if i % step == 0 {
                let w = origin + *p;
                inst.push([w.x, w.y, w.z]);
            }
        }

        if inst.is_empty() {
            inst.push([0.0, 0.0, 0.0]);
        }
        inst.truncate(MAX_INSTANCES);
        inst
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::IVec3;
    use scene::{ids, AssetKind, AssetRecord, PlacedObject, TerrainLayer, TerrainMode};

    fn minimal_level() -> Level {
        let mut level = Level {
            name: "Unit".into(),
            ..Default::default()
        };
        level.terrain = TerrainLayer {
            mode: TerrainMode::Flat,
            surface_material: 3,
            base_height_voxels: 1,
        };
        level.objects.push(PlacedObject {
            instance_id: 100,
            prefab_id: ids::SPAWN_POINT,
            name: "Spawn".into(),
            position: [0.0, 0.0, 0.0],
            visible: true,
            camera: None,
            script_asset_id: None,
        });
        level.assets.push(AssetRecord {
            id: "lua1".into(),
            name: "noop".into(),
            kind: AssetKind::Script,
            path: "missing.lua".into(),
        });
        level
    }

    #[test]
    fn from_level_spawns_visible_objects() {
        let level = minimal_level();
        let state = EngineState::from_level(&level);
        assert_eq!(state.entity_by_instance.len(), 1);
        assert!(state.entity_by_instance.contains_key(&100));
    }

    #[test]
    fn from_level_applies_flat_terrain() {
        let level = minimal_level();
        let state = EngineState::from_level(&level);
        let chunk = state
            .voxel_world
            .chunks
            .get(&IVec3::ZERO)
            .expect("terrain fills origin chunk");
        assert!(chunk.get(0, 0, 0) > 0);
        assert!(chunk.get(0, 1, 0) > 0);
    }
}
