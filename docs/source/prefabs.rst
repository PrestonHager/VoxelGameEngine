.. _prefabs:

Built-in prefabs
================

Prefabs live in the Rust crate ``crates/scene`` (``PrefabLibrary::builtin``). Each has a **stable numeric ID** stored in level JSON as ``prefab_id``. Do not renumber IDs without bumping ``format_version`` and migrating levels.

Rendering note (MVP)
--------------------

The engine currently draws **instances** from positions; distinct meshes per prefab type are **not** implemented yet. All prefabs still appear in the ECS as ``Position`` + ``PrefabRef`` so gameplay and future rendering can branch on ``prefab_id``.

Catalog
-------

.. list-table::
   :widths: 8 25 18 49
   :header-rows: 1

   * - ID
     - Editor name
     - Category
     - Intended use
   * - **1**
     - Cube
     - Primitive
     - Placeholder solid / blocking volume.
   * - **2**
     - Sphere (proxy)
     - Primitive
     - Stand-in for round props until mesh variants exist.
   * - **3**
     - Spawn point
     - Gameplay
     - Player / pawn spawn marker.
   * - **4**
     - Waypoint
     - Gameplay
     - Path, AI, or script anchor.
   * - **5**
     - Light probe
     - Utility
     - Future lighting sample location.
   * - **6**
     - Tree
     - Environment
     - Vegetation placeholder.
   * - **7**
     - Rock
     - Environment
     - Scatter / obstacle placeholder.
   * - **8**
     - Terrain marker
     - Environment
     - Annotates terrain regions; full terrain painting is separate from this marker.

Constants in code
-----------------

Rust code can use ``scene::ids::CUBE``, ``scene::ids::SPAWN_POINT``, etc., from the ``scene`` crate (see ``crates/scene/src/prefabs.rs``).

Adding prefabs
--------------

1. Add a new ``PrefabInfo`` entry in ``PrefabLibrary`` with a **new unused ID**.
2. Document it here and in the repository ``README.md`` prefab table.
3. When per-prefab rendering exists, hook ``PrefabRef`` in the renderer.
