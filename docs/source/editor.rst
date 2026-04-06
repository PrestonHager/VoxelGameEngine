.. _editor-guide:

Editor (MVP)
============

The **editor** is an ``eframe``/``egui`` shell that authors **levels** (JSON), manages a **built-in prefab library**, and talks to **engine-runner** over TCP using ``shared/protocol``. The engine keeps its **own Vulkan window**; the editor does not embed the 3D view yet.

Launching
---------

**Requirements**

- Same prerequisites as the main engine (Rust, Vulkan where applicable).
- For **Push to engine**, ``engine-runner`` must be listening on the same IPC port as the editor.

**Environment**

.. list-table::
   :widths: 30 70
   :header-rows: 1

   * - Variable
     - Meaning
   * - ``VGE_IPC_PORT``
     - TCP port for editor ↔ engine IPC (default in app: **7878** if unset).
   * - ``VGE_ENGINE_EXE``
     - Optional full path to ``engine-runner`` if it is not next to the editor binary.

**Commands**

From the repository root:

.. code-block:: bash

   export VGE_IPC_PORT=7878   # Linux/macOS
   cargo run -p engine-runner

.. code-block:: bash

   export VGE_IPC_PORT=7878
   cargo run -p editor

On Windows (PowerShell):

.. code-block:: powershell

   $env:VGE_IPC_PORT = "7878"
   cargo run -p engine-runner

.. code-block:: powershell

   $env:VGE_IPC_PORT = "7878"
   cargo run -p editor

Helper scripts (from repo root):

* ``scripts/run-editor.ps1`` — sets ``VGE_IPC_PORT`` (default 7878) and runs ``cargo run -p editor`` (optional ``-Release``).
* ``scripts/run-editor.sh`` — same for Unix shells; optional first argument ``--release``.

If the editor starts **without** an engine on the port, it tries to spawn ``engine-runner`` from the same directory as ``editor`` (typical after ``cargo run``: ``target/debug``). Ensure ``engine-runner`` is built and appears beside ``editor``, or set ``VGE_ENGINE_EXE``.

Main window layout
------------------

**Left — Library**

Collapsible groups by category (**Primitive**, **Gameplay**, **Environment**, **Utility**). Click a prefab name to **add** a new placed instance at a default position (see :doc:`prefabs`).

**Right — Scene**

* List of **placed objects**; click to select.
* **Delete selected** removes the instance.
* For the selection: edit **name**, **x / y / z** position, **visible**.
* **Terrain (MVP)**: ``surface_material`` (voxel material id), ``base_height_voxels`` (flat slab height in world voxel units). Mode is currently **flat** only.

**Center**

* **Level name** and **Level file** path (relative or absolute).
* **Ping engine** — connectivity check.
* **Retry start engine** — respawn helper if auto-launch failed.
* **Save level** — writes JSON to the level file path.
* **Load level** — reads JSON from that path.
* **Push to engine** — saves the level, resolves an **absolute** path, then sends ``LoadLevelFromPath`` over IPC so the running engine reloads the file.

Workflow
--------

1. Start **engine-runner** with ``VGE_IPC_PORT`` set (or let the editor spawn it).
2. Start **editor** with the **same** port.
3. Add objects from the library; adjust transforms and terrain.
4. **Save level** to e.g. ``demo_level.vge.json``.
5. **Push to engine** to hot-reload that file in the running engine.

**Note:** Push requires the file to exist on disk and be readable by **engine-runner** (same machine; path is canonicalized on the editor side). Network shares or different working directories may need an explicit absolute path in **Level file**.

Level file format
-----------------

Levels are JSON documents produced by the ``scene`` crate (``Level::to_json_pretty``). They include:

* ``format_version`` — schema version (currently **1**).
* ``name`` — display name.
* ``objects`` — array of placed instances (``instance_id``, ``prefab_id``, ``name``, ``position`` ``[x,y,z]``, ``visible``).
* ``terrain`` — ``mode`` (``flat``), ``surface_material``, ``base_height_voxels``.

See :doc:`prefabs` for stable ``prefab_id`` values.

Further reading
---------------

* :doc:`prefabs` — built-in prefab IDs and categories.
* Repository ``README.md`` — build matrix and crate overview.
* ``agents.md`` — project mission and roadmap phases.
