//! Minimal archetype-style ECS: entities with generation, typed component columns per archetype.

use glam::Vec3;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Entity {
    pub index: u32,
    pub generation: u32,
}

#[derive(Clone, Copy, Debug)]
struct EntityMeta {
    generation: u32,
    alive: bool,
    /// 0 = pos, 1 = pos+vel, 2 = pos+prefab, 3 = pos+camera
    archetype_id: u8,
    row: usize,
}

/// Position in world space (demo component).
#[derive(Clone, Copy, Debug, Default)]
pub struct Position(pub Vec3);

/// Simple velocity for demo systems.
#[derive(Clone, Copy, Debug, Default)]
pub struct Velocity(pub Vec3);

/// Stable prefab / object-type id from the scene library (rendering uses this later).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PrefabRef(pub u32);

/// View parameters for a placed camera entity (editor prefab + engine view matrix).
#[derive(Clone, Copy, Debug)]
pub struct CameraRig {
    pub fov_deg: f32,
    pub near: f32,
    pub far: f32,
    pub yaw: f32,
    pub pitch: f32,
    pub active: bool,
}

impl Default for CameraRig {
    fn default() -> Self {
        Self {
            fov_deg: 45.0,
            near: 0.1,
            far: 200.0,
            yaw: 0.0,
            pitch: -0.35,
            active: true,
        }
    }
}

#[derive(Default)]
struct Archetype {
    entities: Vec<Entity>,
    positions: Vec<Position>,
    velocities: Vec<Velocity>,
}

impl Archetype {
    fn has_velocity(&self) -> bool {
        !self.entities.is_empty() && self.velocities.len() == self.entities.len()
    }
}

#[derive(Default)]
struct ArchetypePosPrefab {
    entities: Vec<Entity>,
    positions: Vec<Position>,
    prefabs: Vec<PrefabRef>,
}

#[derive(Default)]
struct ArchetypePosCamera {
    entities: Vec<Entity>,
    positions: Vec<Position>,
    cameras: Vec<CameraRig>,
}

/// World stores entities across simple archetypes.
#[derive(Default)]
pub struct World {
    entities: Vec<EntityMeta>,
    free_list: Vec<u32>,
    next_index: u32,
    archetype_pos_only: Archetype,
    archetype_pos_vel: Archetype,
    archetype_pos_prefab: ArchetypePosPrefab,
    archetype_pos_camera: ArchetypePosCamera,
}

impl World {
    pub fn clear(&mut self) {
        *self = World::default();
    }

    fn alloc_entity_slot(&mut self) -> (usize, bool) {
        if let Some(idx) = self.free_list.pop() {
            (idx as usize, true)
        } else {
            let idx = self.next_index;
            self.next_index += 1;
            while self.entities.len() <= idx as usize {
                self.entities.push(EntityMeta {
                    generation: 0,
                    alive: false,
                    archetype_id: 0,
                    row: 0,
                });
            }
            (idx as usize, false)
        }
    }

    fn finalize_entity_meta(&mut self, index: usize, reused: bool) -> Entity {
        let meta = &mut self.entities[index];
        if reused {
            meta.generation = meta.generation.wrapping_add(1);
        }
        meta.alive = true;
        Entity {
            index: index as u32,
            generation: meta.generation,
        }
    }

    pub fn spawn_empty(&mut self) -> Entity {
        self.spawn_with(Position::default(), None)
    }

    pub fn spawn_with(&mut self, pos: Position, vel: Option<Velocity>) -> Entity {
        let (index, reused) = self.alloc_entity_slot();
        let entity = self.finalize_entity_meta(index, reused);

        let meta = &mut self.entities[index];
        if let Some(v) = vel {
            let arch = &mut self.archetype_pos_vel;
            meta.archetype_id = 1;
            meta.row = arch.entities.len();
            arch.entities.push(entity);
            arch.positions.push(pos);
            arch.velocities.push(v);
        } else {
            let arch = &mut self.archetype_pos_only;
            meta.archetype_id = 0;
            meta.row = arch.entities.len();
            arch.entities.push(entity);
            arch.positions.push(pos);
        }

        entity
    }

    pub fn spawn_prefab(&mut self, pos: Position, prefab: PrefabRef) -> Entity {
        let (index, reused) = self.alloc_entity_slot();
        let entity = self.finalize_entity_meta(index, reused);

        let meta = &mut self.entities[index];
        let arch = &mut self.archetype_pos_prefab;
        meta.archetype_id = 2;
        meta.row = arch.entities.len();
        arch.entities.push(entity);
        arch.positions.push(pos);
        arch.prefabs.push(prefab);

        entity
    }

    pub fn spawn_camera(&mut self, pos: Position, rig: CameraRig) -> Entity {
        let (index, reused) = self.alloc_entity_slot();
        let entity = self.finalize_entity_meta(index, reused);

        let meta = &mut self.entities[index];
        let arch = &mut self.archetype_pos_camera;
        meta.archetype_id = 3;
        meta.row = arch.entities.len();
        arch.entities.push(entity);
        arch.positions.push(pos);
        arch.cameras.push(rig);

        entity
    }

    pub fn despawn(&mut self, entity: Entity) -> bool {
        let Some(meta_ref) = self.entities.get(entity.index as usize) else {
            return false;
        };
        if !meta_ref.alive || meta_ref.generation != entity.generation {
            return false;
        }
        let arch_id = meta_ref.archetype_id;
        let row = meta_ref.row;

        match arch_id {
            0 => Self::remove_row(&mut self.archetype_pos_only, row, false, &mut self.entities),
            1 => Self::remove_row(&mut self.archetype_pos_vel, row, true, &mut self.entities),
            2 => Self::remove_row_prefab(&mut self.archetype_pos_prefab, row, &mut self.entities),
            3 => Self::remove_row_camera(&mut self.archetype_pos_camera, row, &mut self.entities),
            _ => return false,
        }

        let meta = &mut self.entities[entity.index as usize];
        meta.alive = false;
        self.free_list.push(entity.index);
        true
    }

    fn remove_row(arch: &mut Archetype, row: usize, with_vel: bool, all_meta: &mut [EntityMeta]) {
        let last = arch.entities.len() - 1;
        if row > last {
            return;
        }
        if row != last {
            let moved = arch.entities[last];
            arch.entities.swap_remove(row);
            arch.positions.swap_remove(row);
            if with_vel {
                arch.velocities.swap_remove(row);
            }
            if let Some(m) = all_meta.get_mut(moved.index as usize) {
                m.row = row;
            }
        } else {
            arch.entities.pop();
            arch.positions.pop();
            if with_vel {
                arch.velocities.pop();
            }
        }
    }

    fn remove_row_prefab(arch: &mut ArchetypePosPrefab, row: usize, all_meta: &mut [EntityMeta]) {
        let last = arch.entities.len() - 1;
        if row > last {
            return;
        }
        if row != last {
            let moved = arch.entities[last];
            arch.entities.swap_remove(row);
            arch.positions.swap_remove(row);
            arch.prefabs.swap_remove(row);
            if let Some(m) = all_meta.get_mut(moved.index as usize) {
                m.row = row;
            }
        } else {
            arch.entities.pop();
            arch.positions.pop();
            arch.prefabs.pop();
        }
    }

    fn remove_row_camera(arch: &mut ArchetypePosCamera, row: usize, all_meta: &mut [EntityMeta]) {
        let last = arch.entities.len() - 1;
        if row > last {
            return;
        }
        if row != last {
            let moved = arch.entities[last];
            arch.entities.swap_remove(row);
            arch.positions.swap_remove(row);
            arch.cameras.swap_remove(row);
            if let Some(m) = all_meta.get_mut(moved.index as usize) {
                m.row = row;
            }
        } else {
            arch.entities.pop();
            arch.positions.pop();
            arch.cameras.pop();
        }
    }

    pub fn system_integrate(&mut self, dt: f32) {
        let arch = &mut self.archetype_pos_vel;
        if !arch.has_velocity() {
            return;
        }
        for i in 0..arch.entities.len() {
            arch.positions[i].0 += arch.velocities[i].0 * dt;
        }
    }

    pub fn position_of(&self, entity: Entity) -> Option<Position> {
        let m = self.entities.get(entity.index as usize)?;
        if !m.alive || m.generation != entity.generation {
            return None;
        }
        Some(match m.archetype_id {
            0 => self.archetype_pos_only.positions[m.row],
            1 => self.archetype_pos_vel.positions[m.row],
            2 => self.archetype_pos_prefab.positions[m.row],
            3 => self.archetype_pos_camera.positions[m.row],
            _ => return None,
        })
    }

    pub fn set_position(&mut self, entity: Entity, pos: Position) -> bool {
        let Some(m) = self.entities.get(entity.index as usize) else {
            return false;
        };
        if !m.alive || m.generation != entity.generation {
            return false;
        }
        let row = m.row;
        match m.archetype_id {
            0 => {
                self.archetype_pos_only.positions[row] = pos;
                true
            }
            1 => {
                self.archetype_pos_vel.positions[row] = pos;
                true
            }
            2 => {
                self.archetype_pos_prefab.positions[row] = pos;
                true
            }
            3 => {
                self.archetype_pos_camera.positions[row] = pos;
                true
            }
            _ => false,
        }
    }

    pub fn set_velocity(&mut self, entity: Entity, vel: Velocity) -> bool {
        let Some(m) = self.entities.get(entity.index as usize) else {
            return false;
        };
        if !m.alive || m.generation != entity.generation || m.archetype_id != 1 {
            return false;
        }
        self.archetype_pos_vel.velocities[m.row] = vel;
        true
    }

    pub fn camera_angles_of(&self, entity: Entity) -> Option<(f32, f32)> {
        let m = self.entities.get(entity.index as usize)?;
        if !m.alive || m.generation != entity.generation || m.archetype_id != 3 {
            return None;
        }
        let c = self.archetype_pos_camera.cameras[m.row];
        Some((c.yaw, c.pitch))
    }

    pub fn set_camera_angles(&mut self, entity: Entity, yaw: f32, pitch: f32) -> bool {
        let Some(m) = self.entities.get(entity.index as usize) else {
            return false;
        };
        if !m.alive || m.generation != entity.generation || m.archetype_id != 3 {
            return false;
        }
        let row = m.row;
        let c = &mut self.archetype_pos_camera.cameras[row];
        c.yaw = yaw;
        c.pitch = pitch;
        true
    }

    pub fn positions(&self) -> impl Iterator<Item = (Entity, Position)> + '_ {
        self.archetype_pos_only
            .entities
            .iter()
            .zip(self.archetype_pos_only.positions.iter())
            .map(|(&e, &p)| (e, p))
            .chain(
                self.archetype_pos_vel
                    .entities
                    .iter()
                    .zip(self.archetype_pos_vel.positions.iter())
                    .map(|(&e, &p)| (e, p)),
            )
            .chain(
                self.archetype_pos_prefab
                    .entities
                    .iter()
                    .zip(self.archetype_pos_prefab.positions.iter())
                    .map(|(&e, &p)| (e, p)),
            )
            .chain(
                self.archetype_pos_camera
                    .entities
                    .iter()
                    .zip(self.archetype_pos_camera.positions.iter())
                    .map(|(&e, &p)| (e, p)),
            )
    }

    pub fn prefab_views(&self) -> impl Iterator<Item = (Entity, Position, PrefabRef)> + '_ {
        self.archetype_pos_prefab
            .entities
            .iter()
            .zip(self.archetype_pos_prefab.positions.iter())
            .zip(self.archetype_pos_prefab.prefabs.iter())
            .map(|((&e, &p), &pr)| (e, p, pr))
    }

    pub fn camera_views(&self) -> impl Iterator<Item = (Entity, Position, CameraRig)> + '_ {
        self.archetype_pos_camera
            .entities
            .iter()
            .zip(self.archetype_pos_camera.positions.iter())
            .zip(self.archetype_pos_camera.cameras.iter())
            .map(|((&e, &p), &c)| (e, p, c))
    }
}
