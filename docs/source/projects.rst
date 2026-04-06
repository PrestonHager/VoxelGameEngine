.. _projects-guide:

Projects (planned)
==================

This page defines the planned **project container** model for VGE.

Goals
-----

* Keep game content in a dedicated **project folder**.
* Store project metadata and asset registry in a single binary **``.vge``** file.
* Ensure asset locations are portable by storing **relative paths** (relative to the project folder).

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

The ``.vge`` file is the project root descriptor. All paths inside the file are interpreted
relative to the folder that contains the ``.vge`` file.

Editor new-project scaffold
---------------------------

Creating a new project now scaffolds:

* ``<Name>/<Name>.vge`` for the binary project descriptor.
* ``<Name>/levels/main.vge.json`` as the default empty level.
* ``<Name>/assets/`` and ``<Name>/scripts/`` directories for content.

``.vge`` file (binary) requirements
-----------------------------------

The ``.vge`` project file should be a binary configuration file containing at least:

* ``format_version``: schema/version for migration.
* ``name``: user-facing project name.
* ``created_at`` / ``updated_at`` (optional but recommended).
* ``default_level`` (optional): relative path to the startup level.
* ``assets``: catalog of project assets.

Asset entries
-------------

Each asset entry should include:

* ``asset_id``: stable unique identifier (UUID recommended).
* ``kind``: asset type (for example: texture, mesh, script, level, material, shader, audio).
* ``path``: **relative path** from the project folder (never absolute).
* ``import_settings`` (optional): type-specific metadata.

Path rules
----------

* Paths in ``.vge`` must be relative and normalized (``/`` separators recommended in-file).
* Reject paths that escape the project root (for example ``..`` traversal outside root).
* Resolve final on-disk paths by joining project root + relative path.

Backward compatibility
----------------------

Current level-only workflows (``*.vge.json``) should keep working during migration.
The editor can open standalone level files, then offer creating or attaching to a ``.vge`` project.

Implementation checklist
------------------------

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
* [~] Update CLI and docs to support project-centric workflows end-to-end (initial docs added; CLI guidance pending).

