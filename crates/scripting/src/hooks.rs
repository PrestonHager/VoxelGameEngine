//! Lua hooks: optional global `VGE_LUA_SCRIPT`, `_instance_hooks`, `on_tick`, and per-object scripts
//! from level assets (`_entity_scripts` table, `return function(dt, api) ... end`).

use crate::ScriptError;
use ecs::{Entity, Position, Velocity, World};
use glam::Vec3;
use mlua::{Function, Lua, Table, Value};
use scene::Level;
use std::collections::HashMap;
use std::path::Path;

pub struct ScriptHost {
    lua: Lua,
}

impl ScriptHost {
    pub fn from_file(path: &Path) -> Result<Self, ScriptError> {
        let lua = Lua::new();
        let src = std::fs::read_to_string(path)?;
        lua.load(src)
            .set_name(path.display().to_string())
            .exec()?;
        Ok(Self { lua })
    }

    /// Build VM when `VGE_LUA_SCRIPT` is set and/or the level assigns script assets to objects.
    pub fn from_level(level: &Level) -> Option<Self> {
        let env_script = std::env::var("VGE_LUA_SCRIPT").ok();
        let has_entity_scripts = level
            .objects
            .iter()
            .any(|o| o.script_asset_id.is_some());
        if env_script.is_none() && !has_entity_scripts {
            return None;
        }

        let lua = Lua::new();

        if let Some(p) = env_script.as_ref() {
            match std::fs::read_to_string(p) {
                Ok(body) => {
                    if let Err(e) = lua.load(body).set_name(p.as_str()).exec() {
                        tracing::warn!(target = "script", "VGE_LUA_SCRIPT {p}: {e}");
                    }
                }
                Err(e) => tracing::warn!(target = "script", "read {p}: {e}"),
            }
        }

        let entity_table = match lua.create_table() {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!(target = "script", "lua table: {e}");
                return Some(Self { lua });
            }
        };

        for o in &level.objects {
            let Some(aid) = o.script_asset_id.as_deref() else {
                continue;
            };
            let Some(path) = level.resolve_script_asset_path(aid) else {
                tracing::warn!(target = "script", "no script asset {aid} for instance {}", o.instance_id);
                continue;
            };
            let src = match std::fs::read_to_string(&path) {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!(target = "script", "read {}: {e}", path.display());
                    continue;
                }
            };
            let f: Function = match lua
                .load(src)
                .set_name(path.display().to_string())
                .eval()
            {
                Ok(f) => f,
                Err(e) => {
                    tracing::warn!(target = "script", "{}: {e}", path.display());
                    continue;
                }
            };
            if let Err(e) = entity_table.set(o.instance_id as mlua::Integer, f) {
                tracing::warn!(target = "script", "register hook {}: {e}", o.instance_id);
            }
        }

        if let Err(e) = lua.globals().set("_entity_scripts", entity_table) {
            tracing::warn!(target = "script", "set _entity_scripts: {e}");
        }

        Some(Self { lua })
    }

    pub fn tick(
        &self,
        world: &mut World,
        entity_by_instance: &HashMap<u64, Entity>,
        dt: f32,
    ) -> Result<(), ScriptError> {
        let globals = self.lua.globals();
        let w_raw = world as *mut World as usize;
        let m_raw = entity_by_instance as *const HashMap<u64, Entity> as usize;

        let entity_scripts: Option<Table> = globals.get("_entity_scripts").ok().flatten();
        if let Some(t) = entity_scripts {
            for pair in t.pairs::<mlua::Integer, Function>() {
                let (k, func) = pair?;
                if k <= 0 {
                    continue;
                }
                let instance_id = k as u64;
                let api = create_api_table(&self.lua, w_raw, m_raw, instance_id)?;
                func.call::<()>((dt as f64, api))?;
            }
        }

        let hooks: Option<Table> = globals.get("_instance_hooks").ok().flatten();
        if let Some(hooks) = hooks {
            for pair in hooks.pairs::<Value, Function>() {
                let (k, func) = pair?;
                let instance_id: u64 = match k {
                    Value::Integer(i) if i > 0 => i as u64,
                    Value::Number(n) if n >= 0.0 => n as u64,
                    _ => continue,
                };
                let api = create_api_table(&self.lua, w_raw, m_raw, instance_id)?;
                func.call::<()>((dt as f64, api))?;
            }
        }

        let f: Option<Function> = globals.get("on_tick").ok().flatten();
        if let Some(f) = f {
            f.call::<()>(dt as f64)?;
        }

        Ok(())
    }
}

fn create_api_table(
    lua: &Lua,
    world_raw: usize,
    map_raw: usize,
    default_instance: u64,
) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    t.set("default_instance", default_instance as mlua::Integer)?;

    t.set(
        "log",
        lua.create_function(|_, s: String| {
            tracing::info!(target = "lua", "{s}");
            Ok(())
        })?,
    )?;

    t.set(
        "get_position",
        lua.create_function(move |lua, instance_id: u64| {
            let world = unsafe { &mut *(world_raw as *mut World) };
            let map = unsafe { &*(map_raw as *const HashMap<u64, Entity>) };
            let Some(e) = map.get(&instance_id) else {
                return Ok(Value::Nil);
            };
            let Some(p) = world.position_of(*e) else {
                return Ok(Value::Nil);
            };
            let out = lua.create_table()?;
            out.set("x", p.0.x)?;
            out.set("y", p.0.y)?;
            out.set("z", p.0.z)?;
            Ok(Value::Table(out))
        })?,
    )?;

    t.set(
        "set_position",
        lua.create_function(
            move |_, (instance_id, x, y, z): (u64, f32, f32, f32)| {
                let world = unsafe { &mut *(world_raw as *mut World) };
                let map = unsafe { &*(map_raw as *const HashMap<u64, Entity>) };
                let Some(e) = map.get(&instance_id) else {
                    return Ok(false);
                };
                Ok(world.set_position(*e, Position(Vec3::new(x, y, z))))
            },
        )?,
    )?;

    t.set(
        "set_velocity",
        lua.create_function(
            move |_, (instance_id, x, y, z): (u64, f32, f32, f32)| {
                let world = unsafe { &mut *(world_raw as *mut World) };
                let map = unsafe { &*(map_raw as *const HashMap<u64, Entity>) };
                let Some(e) = map.get(&instance_id) else {
                    return Ok(false);
                };
                Ok(world.set_velocity(*e, Velocity(Vec3::new(x, y, z))))
            },
        )?,
    )?;

    Ok(t)
}

#[cfg(test)]
#[test]
fn tick_runs_in_memory_entity_script() {
    use ecs::World;
    use mlua::{Function, Lua};
    use std::collections::HashMap;

    let lua = Lua::new();
    let f: Function = lua
        .load("return function(dt, api) end")
        .eval()
        .expect("chunk returns function");
    let tbl = lua.create_table().expect("table");
    tbl.set(42i64, f).expect("set hook");
    lua.globals()
        .set("_entity_scripts", tbl)
        .expect("globals");
    let host = ScriptHost { lua };
    let mut world = World::default();
    let map = HashMap::new();
    host.tick(&mut world, &map, 1.0 / 60.0)
        .expect("tick without error");
}
