.. _projects-guide:

Projects
========

The project workflow is implemented in the editor and scene crate.
VGE projects use a binary ``.vge`` file plus a project folder containing levels, scripts, and other assets.

Goals
-----

* Keep game content in a dedicated project folder.
* Store project metadata and asset registry in a binary ``.vge`` file.
* Keep paths portable with project-relative references.

Project folder layout
---------------------

Suggested layout:

.. code-block:: text

   my-game/
   ├─ my-game.vge
   ├─ levels/
   │  └─ main.vge.json
   ├─ scripts/
   │  └─ gameplay.lua
   └─ assets/
      ├─ textures/
      └─ meshes/

The ``.vge`` file is the project root descriptor. Paths stored in the file are interpreted
relative to the directory containing that ``.vge`` file.

Editor new-project scaffold
---------------------------

Creating a new project scaffolds:

* ``<Name>/<Name>.vge`` binary project descriptor.
* ``<Name>/levels/main.vge.json`` default level.
* ``<Name>/assets/`` and ``<Name>/scripts/`` content directories.

``.vge`` file format
--------------------

The scene crate stores project data in a versioned binary file with magic header ``VGEPRJ\\0\\n``.
Current fields include:

* ``format_version``: schema version.
* ``name``: project display name.
* ``default_level``: optional project-relative level path.
* ``assets``: asset list including IDs, kind, and project-relative path.
* ``vsync_enabled``: project-level embedded viewport VSync toggle.

Asset entries
-------------

Each asset entry includes:

* ``id``: stable unique identifier.
* ``name``: display name.
* ``kind``: asset type (for example ``Script``, ``VoxModel``, ``DataJson``).
* ``path``: relative path from project root.

Path rules
----------

* Paths must be relative and normalized.
* Absolute paths are rejected.
* Traversal outside project root (``..``) is rejected.
* Runtime path resolution joins project root + relative path.

Backward compatibility
----------------------

Standalone level workflows (``*.vge.json``) still work. The editor can open and save levels even when no
project file is loaded.

Editor workflow status
----------------------

* [x] Add a new ``Project`` data model module with binary serialize/deserialize support.
* [x] Define ``.vge`` binary schema v1 (header/magic + versioned payload).
* [x] Add path validation utilities to enforce relative, in-root asset paths.
* [x] Add editor UX baseline: **New Project**, **Open Project**, **Save Project**, **Save Project As**.
* [x] Add project persistence APIs (load/save project file atomically).
* [x] Update scene/level save/load flow to support project-relative paths rooted at the project folder.
* [x] Update scripting/asset loading to resolve from project root instead of process CWD.
* [ ] Add migration helpers for existing level-first users (create project from current level file).
* [ ] Extend IPC/protocol messages if engine host needs explicit project context.
* [~] Add tests:
    * [x] binary round-trip for ``.vge``;
    * [x] path traversal rejection;
    * [x] relative-path resolution;
    * [ ] editor integration flow (open/save/push from project context).
* [~] Update CLI and docs to support project-centric workflows end-to-end.

Current behavior notes
----------------------

* New/Open/Save/Save As project actions are available from the editor.
* Default level path is project-relative and can be loaded automatically from project metadata.
* Asset imports write project-relative paths when a project is open.
* In external mode, level push still uses level file paths over IPC; explicit project context IPC extension remains future work.

