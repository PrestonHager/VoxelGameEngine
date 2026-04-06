//! Application loop: fixed-step ECS tick, camera, optional voxel mesh sampling for instances.

use ecs::{CameraRig, Position, PrefabRef, Rotation, Scale, Velocity, World};
use glam::{IVec3, Mat4, Vec3};
use meshing::dual_contouring::{extract_from_chunk_scalar, MeshBuffers};
use physics::PhysicsWorld;
use scene::{CameraAuthoring, Level, PlacedObject, TerrainLayer, TerrainMode};
use scripting::{CursorCommands, ScriptHost};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use voxel::{Chunk, ChunkWorld};

const FIXED_DT: f32 = 1.0 / 60.0;
const MAX_INSTANCES: usize = 524_288;
const MAX_MODEL_VOXELS_PER_OBJECT: usize = 250_000;

fn show_debug_terrain_points() -> bool {
    std::env::var_os("VGE_SHOW_DEBUG_TERRAIN_POINTS").is_some()
}

fn spawn_model_voxels(
    world: &mut World,
    level: &Level,
    obj: &PlacedObject,
    asset_root: Option<&Path>,
) -> Result<(usize, Option<ecs::Entity>), String> {
    let Some(asset_id) = obj.model_asset_id.as_deref() else {
        return Ok((0, None));
    };
    let path = level
        .resolve_vox_asset_path_with_base(asset_id, asset_root)
        .ok_or_else(|| format!("missing VOX asset id: {asset_id}"))?;
    let bytes = fs::read(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let file = dot_vox::load_bytes(&bytes)
        .map_err(|e| format!("parse VOX {}: {e}", path.to_string_lossy()))?;
    let Some(model) = file.models.first() else {
        return Ok((0, None));
    };
    let mut spawned = 0usize;
    let mut first_entity: Option<ecs::Entity> = None;
    let sx = obj.scale[0].max(0.0001);
    let sy = obj.scale[1].max(0.0001);
    let sz = obj.scale[2].max(0.0001);
    let rot = glam::Quat::from_euler(
        glam::EulerRot::XYZ,
        obj.rotation[0],
        obj.rotation[1],
        obj.rotation[2],
    );
    for (i, v) in model.voxels.iter().enumerate() {
        if i >= MAX_MODEL_VOXELS_PER_OBJECT {
            tracing::warn!(
                target: "engine_core",
                "model voxel cap reached ({}), truncating '{}'",
                MAX_MODEL_VOXELS_PER_OBJECT,
                path.display()
            );
            break;
        }
        let local = Vec3::new(v.x as f32 * sx, v.y as f32 * sy, v.z as f32 * sz);
        let world_pos = rot.mul_vec3(local) + Vec3::from_array(obj.position);
        let p = world_pos;
        let e = world.spawn_with(Position(p), None);
        if first_entity.is_none() {
            first_entity = Some(e);
        }
        let _ = world.set_scale(e, Scale(Vec3::new(sx, sy, sz)));
        let _ = world.set_rotation(e, Rotation(Vec3::from_array(obj.rotation)));
        spawned += 1;
    }
    Ok((spawned, first_entity))
}

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
    pub last_cursor_pos: Option<(f64, f64)>,
    pub mouse_pos: Option<(f32, f32)>,
    pub mouse_delta: (f32, f32),
    pub key_w_down: bool,
    pub key_a_down: bool,
    pub key_s_down: bool,
    pub key_d_down: bool,
    pub key_space_down: bool,
    pub key_shift_down: bool,
    pub pending_cursor_commands: CursorCommands,
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
            let e = if o.prefab_id == scene::ids::CAMERA || o.camera.is_some() {
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
                let e = world.spawn_camera(Position(Vec3::from_array(o.position)), rig);
                let _ = world.set_scale(e, Scale(Vec3::from_array(o.scale)));
                let _ = world.set_rotation(e, Rotation(Vec3::from_array(o.rotation)));
                e
            } else if o.model_asset_id.is_some() {
                let (count, first_entity) = match spawn_model_voxels(&mut world, level, o, asset_root) {
                    Ok(x) => x,
                    Err(err) => {
                        tracing::warn!(target: "engine_core", "model spawn failed: {err}");
                        (0, None)
                    }
                };
                if count == 0 {
                    // Fallback for invalid/empty model assets so object still exists.
                    let e = world.spawn_prefab(
                        Position(Vec3::from_array(o.position)),
                        PrefabRef(o.prefab_id),
                    );
                    let _ = world.set_scale(e, Scale(Vec3::from_array(o.scale)));
                    let _ = world.set_rotation(e, Rotation(Vec3::from_array(o.rotation)));
                    e
                } else {
                    // Bind scripts/instance mapping to first model voxel so no extra root cube is rendered.
                    first_entity.expect("count>0 implies at least one entity")
                }
            } else {
                let e = world.spawn_prefab(
                    Position(Vec3::from_array(o.position)),
                    PrefabRef(o.prefab_id),
                );
                let _ = world.set_scale(e, Scale(Vec3::from_array(o.scale)));
                let _ = world.set_rotation(e, Rotation(Vec3::from_array(o.rotation)));
                e
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
            last_cursor_pos: None,
            mouse_pos: None,
            mouse_delta: (0.0, 0.0),
            key_w_down: false,
            key_a_down: false,
            key_s_down: false,
            key_d_down: false,
            key_space_down: false,
            key_shift_down: false,
            pending_cursor_commands: CursorCommands::default(),
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
            scale: [1.0, 1.0, 1.0],
            rotation: [0.0, 0.0, 0.0],
            visible: true,
            camera: Some(CameraAuthoring {
                fov_deg: 45.0,
                yaw_deg: -31.5,
                pitch_deg: -20.0,
                active: true,
            }),
            script_asset_id: None,
            model_asset_id: None,
        });
        level.objects.push(PlacedObject {
            instance_id: 1,
            prefab_id: scene::ids::SPAWN_POINT,
            name: "Spawn".into(),
            position: [3.0, 1.0, 2.0],
            scale: [1.0, 1.0, 1.0],
            rotation: [0.0, 0.0, 0.0],
            visible: true,
            camera: None,
            script_asset_id: None,
            model_asset_id: None,
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
        let (mouse_dx, mouse_dy) = self.mouse_delta;
        let mouse_pos = self.mouse_pos;
        self.mouse_delta = (0.0, 0.0);
        if let Some(s) = &self.script {
            let map = &self.entity_by_instance;
            if let Err(e) = s.tick(
                &mut self.world,
                map,
                FIXED_DT,
                mouse_dx,
                mouse_dy,
                mouse_pos,
                self.key_w_down,
                self.key_a_down,
                self.key_s_down,
                self.key_d_down,
                self.key_space_down,
                self.key_shift_down,
            ) {
                s.push_host_log(format!("[lua-error] {e}"));
                tracing::warn!(target = "script", "lua tick: {e}");
            }
            let cmds = s.drain_cursor_commands();
            self.pending_cursor_commands.center_mouse |= cmds.center_mouse;
            if cmds.cursor_visible.is_some() {
                self.pending_cursor_commands.cursor_visible = cmds.cursor_visible;
            }
        }
    }

    pub fn on_cursor_moved(&mut self, x: f64, y: f64) {
        if let Some((lx, ly)) = self.last_cursor_pos {
            self.mouse_delta.0 += (x - lx) as f32;
            self.mouse_delta.1 += (y - ly) as f32;
        }
        self.mouse_pos = Some((x as f32, y as f32));
        self.last_cursor_pos = Some((x, y));
    }

    pub fn on_mouse_motion(&mut self, dx: f64, dy: f64) {
        self.mouse_delta.0 += dx as f32;
        self.mouse_delta.1 += dy as f32;
    }

    pub fn set_key_down(&mut self, key: &str, down: bool) {
        if key.eq_ignore_ascii_case("w") {
            self.key_w_down = down;
        } else if key.eq_ignore_ascii_case("a") {
            self.key_a_down = down;
        } else if key.eq_ignore_ascii_case("s") {
            self.key_s_down = down;
        } else if key.eq_ignore_ascii_case("d") {
            self.key_d_down = down;
        } else if key.eq_ignore_ascii_case("space") {
            self.key_space_down = down;
        } else if key.eq_ignore_ascii_case("shift") {
            self.key_shift_down = down;
        }
    }

    pub fn drain_script_logs(&self) -> Vec<String> {
        if let Some(s) = &self.script {
            s.drain_logs()
        } else {
            Vec::new()
        }
    }

    pub fn take_cursor_commands(&mut self) -> CursorCommands {
        let out = self.pending_cursor_commands;
        self.pending_cursor_commands = CursorCommands::default();
        out
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

    pub fn free_view_projection(&self, aspect: f32) -> Mat4 {
        let proj = Mat4::perspective_rh(45f32.to_radians(), aspect, 0.1, 200.0);
        let (sy, cy) = self.pitch.sin_cos();
        let (sx, cx) = self.yaw.sin_cos();
        let forward = Vec3::new(cx * cy, sy, sx * cy);
        let target = self.camera_pos + forward;
        let view = Mat4::look_at_rh(self.camera_pos, target, Vec3::Y);
        proj * view
    }

    pub fn camera_sample_position(&self) -> Vec3 {
        self.world
            .camera_views()
            .find(|(_, _, c)| c.active)
            .map(|(_, p, _)| p.0)
            .unwrap_or(self.camera_pos)
    }

    pub fn voxel_instances_for_stream(&mut self) -> Vec<[f32; 9]> {
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

        let mut inst: Vec<[f32; 9]> = self
            .world
            .positions_non_camera()
            .map(|(e, p)| {
                let r = self.world.rotation_of(e).map(|r| r.0).unwrap_or(Vec3::ZERO);
                let s = self
                    .world
                    .scale_of(e)
                    .map(|s| s.0)
                    .unwrap_or(Vec3::ONE);
                [p.0.x, p.0.y, p.0.z, r.x, r.y, r.z, s.x, s.y, s.z]
            })
            .collect();

        if show_debug_terrain_points() {
            let step = (self.mesh_scratch.positions.len() / 256).max(1);
            for (i, p) in self.mesh_scratch.positions.iter().enumerate() {
                if i % step == 0 {
                    let w = origin + *p;
                    inst.push([w.x, w.y, w.z, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0]);
                }
            }
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
            scale: [1.0, 1.0, 1.0],
            rotation: [0.0, 0.0, 0.0],
            visible: true,
            camera: None,
            script_asset_id: None,
            model_asset_id: None,
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
