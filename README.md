# Voxel game engine (Rust + Vulkan)

Workspace layout matches [`agents.md`](agents.md): `ash`-based Vulkan rendering, custom ECS, chunked voxels + meshing, optional editor IPC, Lua scripting hooks, and Rapier3D physics.

## Prerequisites

- **Rust** (stable, recent).
- **Vulkan SDK** and a compatible driver (Windows/Linux). On **macOS**, use **MoltenVK** and point `DYLD_LIBRARY_PATH` / `VK_ICD_FILENAMES` per Apple/MoltenVK docs.

## Build

```bash
cargo build --workspace
cargo run -p engine-runner
```

### Editor IPC smoke test

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

## Crates

| Path | Role |
|------|------|
| `crates/render-vulkan` | `ash` swapchain, depth, instanced mesh pipeline |
| `crates/ecs` | Minimal archetype ECS |
| `crates/voxel` | Chunks + `svo` octree |
| `crates/meshing` | Greedy block mesh + scalar isosurface (marching cubes) |
| `crates/physics` | Rapier3D world |
| `crates/scripting` | `mlua` + optional `notify` watcher |
| `crates/assets` | `notify` hot-reload helper |
| `shared/protocol` | Versioned bincode IPC |

## Docs

Sphinx sources live under [`docs/source`](docs/source). A ReadTheDocs config can be added when the repo is public.

## CI

GitHub Actions runs `fmt`, `clippy`, and `test` on Windows, Ubuntu, and macOS (see [`.github/workflows/ci.yml`](.github/workflows/ci.yml)).
