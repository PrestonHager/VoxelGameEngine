# Voxel game engine (Rust + Vulkan)

Workspace layout matches [`agents.md`](agents.md): `ash`-based Vulkan rendering, custom ECS, chunked voxels + meshing, **editor + level JSON (`scene` crate)**, optional editor IPC, Lua scripting hooks, and Rapier3D physics.

## Prerequisites

- **Rust** (stable, recent).
- **Vulkan SDK** and a compatible driver (Windows/Linux). On **macOS**, use **MoltenVK** and point `DYLD_LIBRARY_PATH` / `VK_ICD_FILENAMES` per Apple/MoltenVK docs.

## Build

```bash
cargo build --workspace
cargo run -p editor -- engine-runner
```

## Editor (MVP)

The **editor** (`apps/editor`) is a **single executable**: it hosts the UI and, in **embedded** mode (default), an in-process Vulkan view. For a **separate** engine window, run **`--no-embedded`** or start the same binary with the **`engine-runner`** subcommand (see below). If nothing is listening on the IPC port in external mode, the editor spawns **`editor engine-runner`** next to itself. **Push to engine** reloads the level file over IPC to that host.

**Full walkthrough:** Sphinx [Editor (MVP)](docs/source/editor.rst) (build with `sphinx-build`, see [Docs](#docs) below).

### Quick start

Terminal A — engine (must use the same port as the editor):

```bash
export VGE_IPC_PORT=7878   # Linux/macOS — omit on Windows or use set VAR=...
cargo run -p editor -- engine-runner
```

Terminal B — editor:

```bash
export VGE_IPC_PORT=7878
cargo run -p editor
```

Windows PowerShell:

```powershell
$env:VGE_IPC_PORT = "7878"
cargo run -p editor -- engine-runner
```

```powershell
$env:VGE_IPC_PORT = "7878"
cargo run -p editor
```

**Embedded editor (default — single process):**

```bash
cargo run -p editor
```

**External engine window** (eframe + separate Vulkan host): `cargo run -p editor -- --no-embedded`

### Environment variables

| Variable | Purpose |
|----------|---------|
| `VGE_IPC_PORT` | TCP port for editor ↔ engine IPC (editor defaults to **7878** if unset). |
| `VGE_ENGINE_EXE` | Optional path to the **`editor`** binary (or legacy `engine-runner`) when auto-spawn cannot use `current_exe()`. |
| `VGE_EMBEDDED` | `1`/`true` forces embedded; `0`/`false` forces external mode (see `config.rs`). Default is embedded. |
| `VGE_LUA_SCRIPT` | Path to a `.lua` file loaded by the engine host (embedded or `editor engine-runner`) (see [Scripting](#lua-scripting-ecs-hooks)). |

### Using the UI

- **Left — Library:** prefabs grouped by category; click a name to add an instance at a default position.
- **Right — Scene:** select instances, rename, set **x/y/z**, toggle **visible**, delete. **Terrain (MVP):** `surface_material`, `base_height_voxels` (flat mode only).
- **Center:** level name, **Level file** path, **Save / Load**, **Push to engine** (IPC or in-process when embedded), **Ping engine** (external mode only), log output.

**Push to engine** (external mode) requires the level file to be readable by the **engine host process** (same machine). Use an absolute path in **Level file** if needed.

### Level files

Levels are JSON (`*.vge.json` suggested). Schema is defined by the `scene` crate: `format_version`, `name`, `objects[]` (`instance_id`, `prefab_id`, `name`, `position`, `visible`, optional `camera` for prefab **Camera**), and `terrain` (`mode`, `surface_material`, `base_height_voxels`).

## Lua scripting (ECS hooks)

Lua can drive per-object logic using **level `instance_id`** keys. Set **`VGE_LUA_SCRIPT`** to a file path; see [`docs/source/scripting.rst`](docs/source/scripting.rst) and `scripts/default_game.lua`.

## Built-in prefabs

Stable IDs are stored in level JSON as `prefab_id`. See [`docs/source/prefabs.rst`](docs/source/prefabs.rst) for the full table.

| ID | Name | Category |
|----|------|----------|
| 1 | Cube | Primitive |
| 2 | Sphere (proxy) | Primitive |
| 3 | Spawn point | Gameplay |
| 4 | Waypoint | Gameplay |
| 5 | Light probe | Utility |
| 6 | Tree | Environment |
| 7 | Rock | Environment |
| 8 | Terrain marker | Environment |
| 9 | Camera | Utility |

The **Camera** prefab spawns an ECS **camera rig**; the **first active** camera in the world supplies the view matrix for the engine host / embedded view (see `ecs::CameraRig`).

In Rust, use `scene::ids::CUBE`, `scene::ids::CAMERA`, etc.

**Note:** The renderer still treats instances mostly as positions; per-prefab meshes are a planned follow-up. ECS already stores `PrefabRef` for each placed object.

## Editor IPC smoke test

Terminal A:

```bash
set VGE_IPC_PORT=7878
cargo run -p editor -- engine-runner
```

Terminal B:

```bash
set VGE_IPC_PORT=7878
cargo run -p editor
```

Use **Ping engine** in the editor, then author a level and **Push to engine**.

## Crates

| Path | Role |
|------|------|
| `crates/render-vulkan` | `ash` swapchain, depth, instanced mesh pipeline |
| `crates/ecs` | Minimal archetype ECS (`Position`, `Velocity`, `PrefabRef`) |
| `crates/scene` | Prefab catalog, serializable `Level` / `PlacedObject` / terrain (JSON) |
| `crates/voxel` | Chunks + `svo` octree |
| `crates/meshing` | Greedy block mesh + scalar isosurface (marching cubes) |
| `crates/physics` | Rapier3D world |
| `crates/scripting` | `mlua` + optional `notify` watcher |
| `crates/assets` | `notify` hot-reload helper |
| `shared/protocol` | Versioned bincode IPC |

## Docs

- **Sphinx** sources: [`docs/source`](docs/source). User guides: [Editor](docs/source/editor.rst), [Prefabs](docs/source/prefabs.rst). Build:

  ```bash
  pip install -r docs/requirements.txt
  cd docs/source && sphinx-build -b html . ../_build
  ```

  Includes [Scripting](docs/source/scripting.rst) (Lua + ECS hooks).

- **ReadTheDocs:** add a config when the repo is public.

## CI

GitHub Actions runs `fmt`, `clippy`, and `test` on Windows, Ubuntu, and macOS (see [`.github/workflows/ci.yml`](.github/workflows/ci.yml)).
