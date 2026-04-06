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
    /// 0 = position only, 1 = position + velocity, 2 = position + prefab ref
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

/// World stores entities across simple archetypes (pos-only, pos+vel, pos+prefab).
#[derive(Default)]
pub struct World {
    entities: Vec<EntityMeta>,
    free_list: Vec<u32>,
    next_index: u32,
    archetype_pos_only: Archetype,
    archetype_pos_vel: Archetype,
    archetype_pos_prefab: ArchetypePosPrefab,
}

impl World {
    pub fn clear(&mut self) {
        *self = World::default();
    }

    pub fn spawn_empty(&mut self) -> Entity {
        self.spawn_with(Position::default(), None)
    }

    pub fn spawn_with(&mut self, pos: Position, vel: Option<Velocity>) -> Entity {
        let (index, reused) = if let Some(idx) = self.free_list.pop() {
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
        };

        let meta = &mut self.entities[index];
        if reused {
            meta.generation = meta.generation.wrapping_add(1);
        }
        meta.alive = true;

        let entity = Entity {
            index: index as u32,
            generation: meta.generation,
        };

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

    /// Scene object from the prefab library (static placement; no velocity integration).
    pub fn spawn_prefab(&mut self, pos: Position, prefab: PrefabRef) -> Entity {
        let (index, reused) = if let Some(idx) = self.free_list.pop() {
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
        };

        let meta = &mut self.entities[index];
        if reused {
            meta.generation = meta.generation.wrapping_add(1);
        }
        meta.alive = true;

        let entity = Entity {
            index: index as u32,
            generation: meta.generation,
        };

        let arch = &mut self.archetype_pos_prefab;
        meta.archetype_id = 2;
        meta.row = arch.entities.len();
        arch.entities.push(entity);
        arch.positions.push(pos);
        arch.prefabs.push(prefab);

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

    fn remove_row_prefab(
        arch: &mut ArchetypePosPrefab,
        row: usize,
        all_meta: &mut [EntityMeta],
    ) {
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

    /// Fixed-step integration for entities that have velocity.
    pub fn system_integrate(&mut self, dt: f32) {
        let arch = &mut self.archetype_pos_vel;
        if !arch.has_velocity() {
            return;
        }
        for i in 0..arch.entities.len() {
            arch.positions[i].0 += arch.velocities[i].0 * dt;
        }
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
    }

    /// Iterate scene-placed objects with prefab ids (editor / debug).
    pub fn prefab_views(&self) -> impl Iterator<Item = (Entity, Position, PrefabRef)> + '_ {
        self.archetype_pos_prefab
            .entities
            .iter()
            .zip(self.archetype_pos_prefab.positions.iter())
            .zip(self.archetype_pos_prefab.prefabs.iter())
            .map(|((&e, &p), &pr)| (e, p, pr))
    }
}
