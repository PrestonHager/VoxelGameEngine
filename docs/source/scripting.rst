.. _scripting:

Lua scripting (ECS hooks)
=========================

The ``scripting`` crate hosts **Lua 5.4** via ``mlua``. Script hooks run on the engine fixed tick
(``1/60`` seconds) and can be sourced from a global script file, per-object script assets, or both.

Loading
-------

Scripting initializes when at least one of the following is present:

* environment variable ``VGE_LUA_SCRIPT`` pointing to a global script file
* one or more placed level objects with ``script_asset_id`` mapped to script assets

The active scripts are loaded when a level is applied/loaded. There is no always-on hot reload loop in the editor runtime path.

Global hooks
------------

**``on_tick(dt)``** (optional)

* Called every frame after per-instance hooks.
* ``dt`` is the fixed timestep in seconds (same as the engine integrator).

**``_instance_hooks``** (optional table)

* Keys: **level instance ids** (the ``instance_id`` field saved in JSON for each ``PlacedObject``).
* Values: functions ``function(dt, api) ... end``.

**``_entity_scripts``** (optional table)

Per-object script assets can return a function and are exposed by instance id. This supports attaching
scripts from project assets to individual objects without hard-coding IDs in one monolithic script.

Lua API
-------

The ``api`` table (per hook invocation) exposes:

.. list-table::
   :widths: 30 70
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
   * - ``api.get_rotation(instance_id)``
     - Returns ``{ pitch, yaw, roll }`` or ``nil``.
   * - ``api.set_rotation(instance_id, pitch, yaw, roll)``
     - Sets entity rotation and returns ``true`` on success.
   * - ``api.mouse_delta()``
     - Returns per-frame mouse movement as ``{ x, y }``.
   * - ``api.mouse_position()``
     - Returns current mouse position as ``{ x, y }``.
   * - ``api.center_mouse()``
     - Requests mouse re-center in the host window on this tick.
   * - ``api.set_cursor_visible(visible)``
     - Requests cursor visibility change.
   * - ``api.get_camera_angles(instance_id)``
     - Returns current camera angles as ``{ yaw, pitch }``.
   * - ``api.set_camera_angles(instance_id, yaw, pitch)``
     - Updates camera angles and returns ``true`` on success.

**``api.default_instance``** duplicates the table key (the instance id passed as the hook’s table entry) for convenience.

Safety
------

Hooks run **synchronously** during ``EngineState::tick``. The Rust side passes raw pointers to the live ``World`` only for the duration of Lua calls: **do not** call back into engine APIs from other threads, and avoid re-entrancy.

Sandbox and restrictions
------------------------

The runtime disables dangerous globals and module loading helpers in script environments, including:
``os``, ``io``, ``package``, ``debug``, ``dofile``, ``loadfile``, and ``require``.
Plan scripts around engine-exposed APIs rather than filesystem/process access.

Example
-------

See ``scripts/default_game.lua`` in the repository for a commented template.

Rust API surface
----------------

* ``scripting::ScriptHost::from_file`` / ``try_from_env``
* ``ScriptHost::tick(&self, world: &mut World, entity_by_instance: &HashMap<u64, Entity>, dt)``
* Legacy helpers: ``LuaBackend``, ``run_lua_file``, ``ScriptHotWatch`` (``scripting`` crate root).
