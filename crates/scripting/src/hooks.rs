//! Lua hooks: optional global `VGE_LUA_SCRIPT`, `_instance_hooks`, `on_tick`, and per-object scripts
//! from level assets (`_entity_scripts` table, `return function(dt, api) ... end`).

use crate::ScriptError;
use ecs::{Entity, Position, Rotation, Scale, Velocity, World};
use glam::Vec3;
use mlua::{Function, Lua, Table, Value};
use scene::Level;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

#[derive(Debug, Default, Clone, Copy)]
pub struct CursorCommands {
    pub center_mouse: bool,
    pub cursor_visible: Option<bool>,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ScriptInput {
    pub mouse_dx: f32,
    pub mouse_dy: f32,
    pub mouse_pos: Option<(f32, f32)>,
    pub key_w: bool,
    pub key_a: bool,
    pub key_s: bool,
    pub key_d: bool,
    pub key_space: bool,
    pub key_shift: bool,
}

pub struct ScriptHost {
    lua: Lua,
    logs: Arc<Mutex<Vec<String>>>,
    cursor_commands: Arc<Mutex<CursorCommands>>,
}

impl ScriptHost {
    pub fn from_file(path: &Path) -> Result<Self, ScriptError> {
        let lua = Lua::new();
        harden_lua_env(&lua);
        let logs = Arc::new(Mutex::new(Vec::new()));
        let cursor_commands = Arc::new(Mutex::new(CursorCommands::default()));
        let src = std::fs::read_to_string(path)?;
        lua.load(src).set_name(path.display().to_string()).exec()?;
        Ok(Self {
            lua,
            logs,
            cursor_commands,
        })
    }

    /// Build VM when `VGE_LUA_SCRIPT` is set and/or the level assigns script assets to objects.
    pub fn from_level(level: &Level) -> Option<Self> {
        Self::from_level_with_base(level, None)
    }

    pub fn from_level_with_base(level: &Level, base_dir: Option<&Path>) -> Option<Self> {
        let env_script = std::env::var("VGE_LUA_SCRIPT").ok();
        let has_entity_scripts = level.objects.iter().any(|o| o.script_asset_id.is_some());
        if env_script.is_none() && !has_entity_scripts {
            return None;
        }

        let lua = Lua::new();
        harden_lua_env(&lua);
        let logs = Arc::new(Mutex::new(Vec::new()));
        let cursor_commands = Arc::new(Mutex::new(CursorCommands::default()));

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
                return Some(Self {
                    lua,
                    logs,
                    cursor_commands,
                });
            }
        };

        for o in &level.objects {
            let Some(aid) = o.script_asset_id.as_deref() else {
                continue;
            };
            let Some(path) = level.resolve_script_asset_path_with_base(aid, base_dir) else {
                tracing::warn!(
                    target = "script",
                    "no script asset {aid} for instance {}",
                    o.instance_id
                );
                continue;
            };
            let src = match std::fs::read_to_string(&path) {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!(target = "script", "read {}: {e}", path.display());
                    continue;
                }
            };
            let f: Function = match lua.load(src).set_name(path.display().to_string()).eval() {
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

        Some(Self {
            lua,
            logs,
            cursor_commands,
        })
    }

    pub fn tick(
        &self,
        world: &mut World,
        entity_by_instance: &HashMap<u64, Entity>,
        dt: f32,
        input: ScriptInput,
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
                let ctx = ApiContext {
                    world_raw: w_raw,
                    map_raw: m_raw,
                    default_instance: instance_id,
                    mouse_dx: input.mouse_dx,
                    mouse_dy: input.mouse_dy,
                    mouse_pos: input.mouse_pos,
                    key_w: input.key_w,
                    key_a: input.key_a,
                    key_s: input.key_s,
                    key_d: input.key_d,
                    key_space: input.key_space,
                    key_shift: input.key_shift,
                    logs: Arc::clone(&self.logs),
                    cursor_commands: Arc::clone(&self.cursor_commands),
                };
                let api = create_api_table(&self.lua, &ctx)?;
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
                let ctx = ApiContext {
                    world_raw: w_raw,
                    map_raw: m_raw,
                    default_instance: instance_id,
                    mouse_dx: input.mouse_dx,
                    mouse_dy: input.mouse_dy,
                    mouse_pos: input.mouse_pos,
                    key_w: input.key_w,
                    key_a: input.key_a,
                    key_s: input.key_s,
                    key_d: input.key_d,
                    key_space: input.key_space,
                    key_shift: input.key_shift,
                    logs: Arc::clone(&self.logs),
                    cursor_commands: Arc::clone(&self.cursor_commands),
                };
                let api = create_api_table(&self.lua, &ctx)?;
                func.call::<()>((dt as f64, api))?;
            }
        }

        let f: Option<Function> = globals.get("on_tick").ok().flatten();
        if let Some(f) = f {
            f.call::<()>(dt as f64)?;
        }

        Ok(())
    }

    pub fn drain_logs(&self) -> Vec<String> {
        match self.logs.lock() {
            Ok(mut logs) => std::mem::take(&mut *logs),
            Err(_) => Vec::new(),
        }
    }

    pub fn push_host_log(&self, line: impl Into<String>) {
        if let Ok(mut logs) = self.logs.lock() {
            logs.push(line.into());
        }
    }

    pub fn drain_cursor_commands(&self) -> CursorCommands {
        match self.cursor_commands.lock() {
            Ok(mut c) => {
                let out = *c;
                *c = CursorCommands::default();
                out
            }
            Err(_) => CursorCommands::default(),
        }
    }
}

/// Apply a minimal, practical sandbox for gameplay scripts.
///
/// We disable libraries and helpers commonly used for filesystem/process/module
/// access so project scripts cannot escape into host-level operations.
fn harden_lua_env(lua: &Lua) {
    let globals = lua.globals();
    for name in [
        "os", "io", "package", "debug", "dofile", "loadfile", "require",
    ] {
        if let Err(e) = globals.set(name, Value::Nil) {
            tracing::warn!(target = "script", "sandbox disable {name}: {e}");
        }
    }
}

struct ApiContext {
    world_raw: usize,
    map_raw: usize,
    default_instance: u64,
    mouse_dx: f32,
    mouse_dy: f32,
    mouse_pos: Option<(f32, f32)>,
    key_w: bool,
    key_a: bool,
    key_s: bool,
    key_d: bool,
    key_space: bool,
    key_shift: bool,
    logs: Arc<Mutex<Vec<String>>>,
    cursor_commands: Arc<Mutex<CursorCommands>>,
}

fn create_api_table(lua: &Lua, ctx: &ApiContext) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    t.set("default_instance", ctx.default_instance as mlua::Integer)?;

    let world_raw = ctx.world_raw;
    let map_raw = ctx.map_raw;
    let mouse_dx = ctx.mouse_dx;
    let mouse_dy = ctx.mouse_dy;
    let mouse_pos = ctx.mouse_pos;
    let key_w = ctx.key_w;
    let key_a = ctx.key_a;
    let key_s = ctx.key_s;
    let key_d = ctx.key_d;
    let key_space = ctx.key_space;
    let key_shift = ctx.key_shift;
    let logs = Arc::clone(&ctx.logs);
    let cursor_commands = Arc::clone(&ctx.cursor_commands);

    t.set(
        "log",
        lua.create_function(move |_, s: String| {
            if let Ok(mut sink) = logs.lock() {
                sink.push(s.clone());
            }
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
        "get_rotation",
        lua.create_function(move |lua, instance_id: u64| {
            let world = unsafe { &mut *(world_raw as *mut World) };
            let map = unsafe { &*(map_raw as *const HashMap<u64, Entity>) };
            let Some(e) = map.get(&instance_id) else {
                return Ok(Value::Nil);
            };
            let Some(r) = world.rotation_of(*e) else {
                return Ok(Value::Nil);
            };
            let out = lua.create_table()?;
            out.set("pitch", r.0.x)?;
            out.set("yaw", r.0.y)?;
            out.set("roll", r.0.z)?;
            Ok(Value::Table(out))
        })?,
    )?;
    t.set(
        "set_rotation",
        lua.create_function(
            move |_, (instance_id, pitch, yaw, roll): (u64, f32, f32, f32)| {
                let world = unsafe { &mut *(world_raw as *mut World) };
                let map = unsafe { &*(map_raw as *const HashMap<u64, Entity>) };
                let Some(e) = map.get(&instance_id) else {
                    return Ok(false);
                };
                Ok(world.set_rotation(*e, Rotation(Vec3::new(pitch, yaw, roll))))
            },
        )?,
    )?;

    t.set(
        "set_position",
        lua.create_function(move |_, (instance_id, x, y, z): (u64, f32, f32, f32)| {
            let world = unsafe { &mut *(world_raw as *mut World) };
            let map = unsafe { &*(map_raw as *const HashMap<u64, Entity>) };
            let Some(e) = map.get(&instance_id) else {
                return Ok(false);
            };
            Ok(world.set_position(*e, Position(Vec3::new(x, y, z))))
        })?,
    )?;

    t.set(
        "get_scale",
        lua.create_function(move |lua, instance_id: u64| {
            let world = unsafe { &mut *(world_raw as *mut World) };
            let map = unsafe { &*(map_raw as *const HashMap<u64, Entity>) };
            let Some(e) = map.get(&instance_id) else {
                return Ok(Value::Nil);
            };
            let Some(s) = world.scale_of(*e) else {
                return Ok(Value::Nil);
            };
            let out = lua.create_table()?;
            out.set("x", s.0.x)?;
            out.set("y", s.0.y)?;
            out.set("z", s.0.z)?;
            Ok(Value::Table(out))
        })?,
    )?;

    t.set(
        "set_scale",
        lua.create_function(move |_, (instance_id, x, y, z): (u64, f32, f32, f32)| {
            let world = unsafe { &mut *(world_raw as *mut World) };
            let map = unsafe { &*(map_raw as *const HashMap<u64, Entity>) };
            let Some(e) = map.get(&instance_id) else {
                return Ok(false);
            };
            let s = Vec3::new(x.max(0.0001), y.max(0.0001), z.max(0.0001));
            Ok(world.set_scale(*e, Scale(s)))
        })?,
    )?;

    t.set(
        "set_velocity",
        lua.create_function(move |_, (instance_id, x, y, z): (u64, f32, f32, f32)| {
            let world = unsafe { &mut *(world_raw as *mut World) };
            let map = unsafe { &*(map_raw as *const HashMap<u64, Entity>) };
            let Some(e) = map.get(&instance_id) else {
                return Ok(false);
            };
            Ok(world.set_velocity(*e, Velocity(Vec3::new(x, y, z))))
        })?,
    )?;

    t.set(
        "mouse_delta",
        lua.create_function(move |_, ()| Ok((mouse_dx as f64, mouse_dy as f64)))?,
    )?;
    t.set(
        "mouse_position",
        lua.create_function(move |lua, ()| {
            if let Some((x, y)) = mouse_pos {
                let out = lua.create_table()?;
                out.set("x", x)?;
                out.set("y", y)?;
                Ok(Value::Table(out))
            } else {
                Ok(Value::Nil)
            }
        })?,
    )?;
    t.set(
        "is_key_down",
        lua.create_function(move |_, key: String| {
            let down = match key.to_ascii_lowercase().as_str() {
                "w" => key_w,
                "a" => key_a,
                "s" => key_s,
                "d" => key_d,
                "space" => key_space,
                "shift" => key_shift,
                _ => false,
            };
            Ok(down)
        })?,
    )?;

    let cursor_commands_center = Arc::clone(&cursor_commands);
    t.set(
        "center_mouse",
        lua.create_function(move |_, ()| {
            if let Ok(mut cmds) = cursor_commands_center.lock() {
                cmds.center_mouse = true;
            }
            Ok(true)
        })?,
    )?;
    let cursor_commands_visible = Arc::clone(&cursor_commands);
    t.set(
        "set_cursor_visible",
        lua.create_function(move |_, visible: bool| {
            if let Ok(mut cmds) = cursor_commands_visible.lock() {
                cmds.cursor_visible = Some(visible);
            }
            Ok(true)
        })?,
    )?;

    t.set(
        "get_camera_angles",
        lua.create_function(move |lua, instance_id: u64| {
            let world = unsafe { &mut *(world_raw as *mut World) };
            let map = unsafe { &*(map_raw as *const HashMap<u64, Entity>) };
            let Some(e) = map.get(&instance_id) else {
                return Ok(Value::Nil);
            };
            let Some((yaw, pitch)) = world.camera_angles_of(*e) else {
                return Ok(Value::Nil);
            };
            let out = lua.create_table()?;
            out.set("yaw", yaw)?;
            out.set("pitch", pitch)?;
            Ok(Value::Table(out))
        })?,
    )?;

    t.set(
        "set_camera_angles",
        lua.create_function(move |_, (instance_id, yaw, pitch): (u64, f32, f32)| {
            let world = unsafe { &mut *(world_raw as *mut World) };
            let map = unsafe { &*(map_raw as *const HashMap<u64, Entity>) };
            let Some(e) = map.get(&instance_id) else {
                return Ok(false);
            };
            Ok(world.set_camera_angles(*e, yaw, pitch))
        })?,
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
    lua.globals().set("_entity_scripts", tbl).expect("globals");
    let host = ScriptHost {
        lua,
        logs: Arc::new(Mutex::new(Vec::new())),
        cursor_commands: Arc::new(Mutex::new(CursorCommands::default())),
    };
    let mut world = World::default();
    let map = HashMap::new();
    host.tick(&mut world, &map, 1.0 / 60.0, ScriptInput::default())
        .expect("tick without error");
}

#[cfg(test)]
#[test]
fn harden_lua_env_disables_dangerous_globals() {
    let lua = Lua::new();
    harden_lua_env(&lua);
    let globals = lua.globals();
    for name in [
        "os", "io", "package", "debug", "dofile", "loadfile", "require",
    ] {
        let v: Value = globals.get(name).expect("global read");
        assert!(matches!(v, Value::Nil), "{name} should be nil");
    }
}
