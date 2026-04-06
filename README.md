# Voxel game engine (Rust + Vulkan)

Workspace layout matches [`agents.md`](agents.md): `ash`-based Vulkan rendering, custom ECS, chunked voxels + meshing, **editor + level JSON (`scene` crate)**, optional editor IPC, Lua scripting hooks, and Rapier3D physics.

## Prerequisites

- **Rust** (stable, recent).
- **Vulkan SDK** and a compatible driver (Windows/Linux). On **macOS**, use **MoltenVK** and point `DYLD_LIBRARY_PATH` / `VK_ICD_FILENAMES` per Apple/MoltenVK docs.

## Build

```bash
cargo build --workspace
cargo run -p engine-runner
```

## Editor (MVP)

The **editor** (`apps/editor`) is an egui shell for **placing prefabs** (including a **Camera** that drives the engine view), editing **terrain** (flat slab + material/height), and saving **levels** as JSON. By default it can **start `engine-runner`** if nothing is listening on the IPC port, and **Push to engine** reloads the level file over IPC. With **`VGE_EMBEDDED=1`**, the same binary runs **OpenGL egui + a Vulkan child window** (parented on Windows/X11 when supported) and **Push** applies the level **in-process** (no separate engine).

**Full walkthrough:** Sphinx [Editor (MVP)](docs/source/editor.rst) (build with `sphinx-build`, see [Docs](#docs) below).

### Quick start

Terminal A — engine (must use the same port as the editor):

```bash
export VGE_IPC_PORT=7878   # Linux/macOS — omit on Windows or use set VAR=...
cargo run -p engine-runner
```

Terminal B — editor:

```bash
export VGE_IPC_PORT=7878
cargo run -p editor
```

Windows PowerShell:

```powershell
$env:VGE_IPC_PORT = "7878"
cargo run -p engine-runner
```

```powershell
$env:VGE_IPC_PORT = "7878"
cargo run -p editor
```

Or from the repo root: `.\scripts\run-editor.ps1` (optional `-Port`, `-Release`).

**Embedded editor (single process):**

```bash
export VGE_EMBEDDED=1
cargo run -p editor
```

### Environment variables

| Variable | Purpose |
|----------|---------|
| `VGE_IPC_PORT` | TCP port for editor ↔ engine IPC (editor defaults to **7878** if unset). |
| `VGE_ENGINE_EXE` | Optional path to `engine-runner` when it is not next to `editor` (e.g. after `cargo run`, both live under `target/debug`). |
| `VGE_EMBEDDED` | Set to `1` to run the editor with an in-process Vulkan **Engine view** window instead of spawning `engine-runner`. |
| `VGE_LUA_SCRIPT` | Path to a `.lua` file loaded by **engine-runner** and the embedded editor (see [Scripting](#lua-scripting-ecs-hooks)). |

### Using the UI

- **Left — Library:** prefabs grouped by category; click a name to add an instance at a default position.
- **Right — Scene:** select instances, rename, set **x/y/z**, toggle **visible**, delete. **Terrain (MVP):** `surface_material`, `base_height_voxels` (flat mode only).
- **Center:** level name, **Level file** path, **Save / Load**, **Push to engine** (IPC or in-process when embedded), **Ping engine** (external mode only), log output.

**Push to engine** (external mode) requires the level file to be readable by `engine-runner` (same machine). Use an absolute path in **Level file** if needed.

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

The **Camera** prefab spawns an ECS **camera rig**; the **first active** camera in the world supplies the view matrix for `engine-runner` / embedded view (see `ecs::CameraRig`).

In Rust, use `scene::ids::CUBE`, `scene::ids::CAMERA`, etc.

**Note:** The renderer still treats instances mostly as positions; per-prefab meshes are a planned follow-up. ECS already stores `PrefabRef` for each placed object.

## Editor IPC smoke test

Terminal A:

```bash
set VGE_IPC_PORT=7878
cargo run -p engine-runner
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
