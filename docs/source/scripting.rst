.. _scripting:

Lua scripting (ECS hooks)
=========================

The ``scripting`` crate hosts **Lua 5.4** via ``mlua``. The engine calls into Lua on each **fixed physics tick** (60 Hz) when a script file is configured.

Loading
-------

Set the environment variable **``VGE_LUA_SCRIPT``** to a ``.lua`` file path before starting **``editor engine-runner``** (external host) or the **embedded editor** (``VGE_EMBEDDED=1``). The file is executed once at startup; there is no hot reload in this MVP (the existing ``ScriptHotWatch`` helper remains available for future wiring).

Global hooks
------------

**``on_tick(dt)``** (optional)

* Called every frame after per-instance hooks.
* ``dt`` is the fixed timestep in seconds (same as the engine integrator).

**``_instance_hooks``** (optional table)

* Keys: **level instance ids** (the ``instance_id`` field saved in JSON for each ``PlacedObject``).
* Values: functions ``function(dt, api) ... end``.

The **``api``** table (per hook invocation) exposes:

.. list-table::
   :widths: 28 72
   :header-rows: 1

   * - Method
     - Behavior
   * - ``api.log(msg)``
     - Logs at ``info`` level with target ``lua``.
   * - ``api.get_position(instance_id)``
     - Returns ``{ x, y, z }`` or ``nil`` if the id is unknown or the entity has no position.
   * - ``api.set_position(instance_id, x, y, z)``
     - Returns ``true`` if a position was written for a live entity.
   * - ``api.set_velocity(instance_id, x, y, z)``
     - Returns ``true`` only for entities in the **velocity** archetype (movable bodies).

**``api.default_instance``** duplicates the table key (the instance id passed as the hook’s table entry) for convenience.

Safety
------

Hooks run **synchronously** during ``EngineState::tick``. The Rust side passes raw pointers to the live ``World`` only for the duration of Lua calls: **do not** call back into engine APIs from other threads, and avoid re-entrancy.

Example
-------

See ``scripts/default_game.lua`` in the repository for a commented template.

Rust API surface
----------------

* ``scripting::ScriptHost::from_file`` / ``try_from_env``
* ``ScriptHost::tick(&self, world: &mut World, entity_by_instance: &HashMap<u64, Entity>, dt)``
* Legacy helpers: ``LuaBackend``, ``run_lua_file``, ``ScriptHotWatch`` (``scripting`` crate root).
