.. _editor-guide:

Editor (MVP)
============

The **editor** authors **levels** (JSON), manages a **built-in prefab library**, and can run in two modes:

* **External engine (default):** ``eframe`` + optional auto-launch of the same binary with the ``engine-runner`` subcommand; **Push** uses ``shared/protocol`` (``LoadLevelFromPath``).
* **Embedded:** set ``VGE_EMBEDDED=1`` — same binary runs **egui** (OpenGL via glutin) and a second **Vulkan** window, parented to the editor on **Windows** and **X11** when the platform allows. **Push** applies the level **in-process** (no IPC).

Launching
---------

**Requirements**

- Same prerequisites as the main engine (Rust, Vulkan where applicable).
- For **Push to engine**, an **engine host** (``editor engine-runner``) must be listening on the same IPC port as the editor.

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
     - Set to ``1`` for in-process Vulkan **Engine view** (child window) + egui.
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

If the editor starts **without** an engine on the port (non-embedded), it tries to spawn **``editor engine-runner``** using the same executable (typical after ``cargo run``: ``target/debug/editor``). Set ``VGE_ENGINE_EXE`` if the host binary is not discoverable from ``current_exe()``.

**Embedded launch**

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
* **Push to engine** — external: save + IPC ``LoadLevelFromPath``; embedded: save (optional) + apply level to the in-process engine state.

Workflow
--------

1. Start **``editor engine-runner``** with ``VGE_IPC_PORT`` set (or let the editor spawn it).
2. Start **editor** with the **same** port.
3. Add objects from the library; adjust transforms and terrain.
4. **Save level** to e.g. ``demo_level.vge.json``.
5. **Push to engine** to hot-reload that file in the running engine.

**Note:** Push requires the file to exist on disk and be readable by the **engine host** (same machine; path is canonicalized on the editor side). Network shares or different working directories may need an explicit absolute path in **Level file**.

Level file format
-----------------

Levels are JSON documents produced by the ``scene`` crate (``Level::to_json_pretty``). They include:

* ``format_version`` — schema version (currently **1**).
* ``name`` — display name.
* ``objects`` — array of placed instances (``instance_id``, ``prefab_id``, ``name``, ``position`` ``[x,y,z]``, ``visible``, optional ``camera`` for the Camera prefab).
* ``terrain`` — ``mode`` (``flat``), ``surface_material``, ``base_height_voxels``.

See :doc:`prefabs` for stable ``prefab_id`` values.

Further reading
---------------

* :doc:`projects` — planned ``.vge`` project folder + binary config model.
* :doc:`prefabs` — built-in prefab IDs and categories.
* :doc:`scripting` — Lua hooks and ``VGE_LUA_SCRIPT``.
* Repository ``README.md`` — build matrix and crate overview.
* ``agents.md`` — project mission and roadmap phases.
