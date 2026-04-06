//! Application loop: fixed-step ECS tick, camera, optional voxel mesh sampling for instances.

use ecs::{Position, PrefabRef, Velocity, World};
use glam::{IVec3, Mat4, Vec3};
use meshing::dual_contouring::{extract_from_chunk_scalar, MeshBuffers};
use physics::PhysicsWorld;
use scene::{Level, PlacedObject, TerrainLayer, TerrainMode};
use voxel::{Chunk, ChunkWorld};

const FIXED_DT: f32 = 1.0 / 60.0;
/// Upper bound for instance data passed to the renderer; keep in sync with `render-vulkan` (`MAX_INSTANCES`).
const MAX_INSTANCES: usize = 16_384;

pub struct EngineState {
    pub world: World,
    pub voxel_world: ChunkWorld,
    pub physics: PhysicsWorld,
    pub mesh_scratch: MeshBuffers,
    pub camera_pos: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub time: f32,
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
            // small hill (authoring-friendly default detail)
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
    /// Replace ECS + terrain from a serialized level (editor push / load).
    pub fn from_level(level: &Level) -> Self {
        let mut world = World::default();
        for o in &level.objects {
            if !o.visible {
                continue;
            }
            world.spawn_prefab(
                Position(Vec3::from_array(o.position)),
                PrefabRef(o.prefab_id),
            );
        }

        let mut vw = ChunkWorld::new(16);
        apply_terrain_layer(&mut vw, &level.terrain);

        Self {
            world,
            voxel_world: vw,
            physics: PhysicsWorld::demo_stack(),
            mesh_scratch: MeshBuffers::default(),
            camera_pos: Vec3::new(12.0, 14.0, 18.0),
            yaw: -0.55,
            pitch: -0.35,
            time: 0.0,
        }
    }

    pub fn apply_level(&mut self, level: &Level) {
        *self = Self::from_level(level);
    }
}

impl Default for EngineState {
    fn default() -> Self {
        let mut level = Level {
            name: "Demo".into(),
            ..Default::default()
        };
        level.objects.push(PlacedObject {
            instance_id: 1,
            prefab_id: scene::ids::SPAWN_POINT,
            name: "Spawn".into(),
            position: [3.0, 1.0, 2.0],
            visible: true,
        });
        let mut s = Self::from_level(&level);
        // One dynamic body for integration demo (not yet in level format).
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
    }

    pub fn view_projection(&self, aspect: f32) -> Mat4 {
        let proj = Mat4::perspective_rh(45f32.to_radians(), aspect, 0.1, 200.0);
        let (sy, cy) = self.pitch.sin_cos();
        let (sx, cx) = self.yaw.sin_cos();
        let forward = Vec3::new(cx * cy, sy, sx * cy);
        let target = self.camera_pos + forward;
        let view = Mat4::look_at_rh(self.camera_pos, target, Vec3::Y);
        proj * view
    }

    /// Rebuild scalar mesh for chunk under camera and return instance offsets (world space).
    pub fn voxel_instances_for_stream(&mut self) -> Vec<[f32; 3]> {
        let e = self.voxel_world.edge as i32;
        let cc = IVec3::new(
            (self.camera_pos.x / e as f32).floor() as i32,
            (self.camera_pos.y / e as f32).floor() as i32,
            (self.camera_pos.z / e as f32).floor() as i32,
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
