.. _editor-guide:

Editor (MVP)
============

The **editor** authors level JSON, manages prefab instances and project assets, and runs in two modes:

* **Embedded (default):** one ``editor`` process with egui UI plus an in-process engine viewport.
* **External engine:** separate editor UI process and engine host process (``editor engine-runner``) communicating over localhost IPC.

Mode selection uses this precedence:

1. CLI flags: ``--no-embedded`` forces external, ``--embedded`` or ``-e`` forces embedded.
2. ``VGE_EMBEDDED`` env var (``1/true/yes/on`` or ``0/false/no/off``).
3. Saved preferences default (embedded by default unless changed in Preferences).

Launching
---------

**Requirements**

- Same prerequisites as the main engine (Rust, Vulkan where applicable).
- In external mode, the editor and engine host must use the same IPC port.

**Environment**

.. list-table::
   :widths: 30 70
   :header-rows: 1

   * - Variable
     - Meaning
   * - ``VGE_IPC_PORT``
     - TCP port for editor ↔ engine IPC (default in app: **7878** if unset).
   * - ``VGE_ENGINE_EXE``
     - Optional full path to the **editor** binary (or legacy standalone ``engine-runner``) when auto-spawn cannot use ``current_exe()``.
   * - ``VGE_EMBEDDED``
     - Overrides saved preference default. Truthy values force embedded; falsy values force external.
   * - ``VGE_LUA_SCRIPT``
     - Optional ``.lua`` path for ECS hooks (see :doc:`scripting`).

**Commands**

From the repository root:

.. code-block:: bash

   export VGE_IPC_PORT=7878   # Linux/macOS
   cargo run -p editor -- engine-runner

.. code-block:: bash

   export VGE_IPC_PORT=7878
   cargo run -p editor

On Windows (PowerShell):

.. code-block:: powershell

   $env:VGE_IPC_PORT = "7878"
   cargo run -p editor -- engine-runner

.. code-block:: powershell

   $env:VGE_IPC_PORT = "7878"
   cargo run -p editor

If external mode starts without an engine listening on the port, the editor attempts to spawn an engine host automatically. Resolution order is:

1. ``VGE_ENGINE_EXE`` path
2. current executable path, launched with ``engine-runner``
3. legacy sibling ``engine-runner(.exe)``

Startup waits up to 15 seconds for host readiness.

**Embedded launch (explicit override)**

.. code-block:: bash

   export VGE_EMBEDDED=1
   cargo run -p editor

Main window layout
------------------

**Left — Library**

Collapsible groups by category (**Primitive**, **Gameplay**, **Environment**, **Utility**). Click a prefab name to **add** a new placed instance at a default position (see :doc:`prefabs`).

**Right — Scene**

* List of **placed objects**; click to select.
* **Delete selected** removes the instance.
* For the selection: edit **name**, **x / y / z** position, **visible**.
* **Camera** prefab: extra fields **fov**, **yaw/pitch** (degrees), **active** (first active camera drives the engine view).
* **Terrain (MVP)**: ``surface_material`` (voxel material id), ``base_height_voxels`` (flat slab height in world voxel units). Mode is currently **flat** only.

**Center**

* **Level name** and **Level file** path (relative or absolute).
* **Ping engine** / **Retry start engine** — external mode only.
* **Save level** — writes JSON to the level file path.
* **Load level** — reads JSON from that path.
* **Push to engine** — always saves first, then:

  * external mode: sends IPC ``LoadLevelFromPath`` to engine host
  * embedded mode: applies the current level in-process
* **Play / Stop** (embedded mode):

  * entering Play captures engine input in the viewport and hides cursor
  * ``Esc`` or Stop releases capture and exits Play mode

Workflow
--------

1. Start the editor (embedded default), or start in external mode with ``--no-embedded``.
2. If using external mode, start ``editor engine-runner`` with matching ``VGE_IPC_PORT`` (or let the editor spawn it).
3. Add objects from the library; adjust transforms and terrain.
4. **Save level** to e.g. ``demo_level.vge.json``.
5. **Push to engine** to save and apply/reload the level.

In external mode, Push requires the level path to be readable by the engine host process on the same machine.

Level file format
-----------------

Levels are JSON documents produced by the ``scene`` crate (``Level::to_json_pretty``). They include:

* ``format_version`` — schema version (currently **1**).
* ``name`` — display name.
* ``objects`` — array of placed instances (``instance_id``, ``prefab_id``, ``name``, ``position`` ``[x,y,z]``, ``visible``, optional ``camera`` for the Camera prefab).
* ``terrain`` — ``mode`` (``flat``), ``surface_material``, ``base_height_voxels``.

See :doc:`prefabs` for stable ``prefab_id`` values.

Projects and assets
-------------------

Project workflows are implemented and use binary ``.vge`` project files plus project-relative asset paths.
See :doc:`projects` for project scaffold layout, path rules, and current capabilities.

Further reading
---------------

* :doc:`projects` — project workflow, binary ``.vge`` files, path validation, and current limitations.
* :doc:`prefabs` — built-in prefab IDs and categories.
* :doc:`scripting` — Lua hooks, per-object script assets, and sandbox details.
* Repository ``README.md`` — build matrix and crate overview.
* ``agents.md`` — project mission and roadmap phases.
