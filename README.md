# Voxel game engine (Rust + Vulkan)

Workspace layout matches [`agents.md`](agents.md): `ash`-based Vulkan rendering, custom ECS, chunked voxels + meshing, **editor + level JSON (`scene` crate)**, optional editor IPC, Lua scripting hooks, and Rapier3D physics.

## Installation and Documentation

Install via the latest [GitHub Release](https://github.com/PrestonHager/VoxelGameEngine/releases) for your platform, or build from source using the instructions below.

Documentation can be found at [vge.prestonhager.com](https://vge.prestonhager.com) and is built from this repository.

## Prerequisites

- **Rust** (stable, recent).
- **Vulkan SDK** and a compatible driver (Windows/Linux). On **macOS**, use **MoltenVK** and point `DYLD_LIBRARY_PATH` / `VK_ICD_FILENAMES` per Apple/MoltenVK docs.

## Build

```bash
cargo build --workspace
cargo run -p editor -- engine-runner
```

## Nix (overlay and flake)

The repo ships a Nix **overlay** that exposes **`pkgs.vge-editor`** (the editor binary as **`vge-editor`**, plus a `.desktop` entry). Builds are **Linux-only** (`x86_64-linux`, `aarch64-linux`). **`flake.lock`** pins **`nixpkgs`**; run **`nix flake update`** when you want to move to a newer nixpkgs.

### This repository as a flake

From a checkout of this repo (flake inputs must be **git-tracked**):

| Command | Purpose |
|---------|---------|
| `nix build` / `nix build .#vge-editor` | Build the editor package |
| `nix run` | Run **`vge-editor`** |
| `nix develop` | Dev shell with the same build inputs as the package plus Rust tools |

### Use the overlay from another flake

Add this repo as an input and apply **`overlays.default`**, then install **`vge-editor`**:

```nix
{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    vge.url = "github:PrestonHager/VoxelGameEngine"; # or git+file:… for a local checkout
  };

  outputs = { self, nixpkgs, vge, ... }:
    let
      system = "x86_64-linux";
      pkgs = import nixpkgs {
        inherit system;
        overlays = [ vge.overlays.default ];
      };
    in
    {
      packages.${system}.default = pkgs.vge-editor;
    };
}
```

On **NixOS** or **Home Manager**, use the same overlay list under **`nixpkgs.overlays`** and add **`vge-editor`** to **`environment.systemPackages`** or **`home.packages`**.

### NixOS (configuration without flakes)

Point the module system at **`nix/overlay.nix`** (absolute path or a path relative to your config entry):

```nix
{ pkgs, ... }: let
  version = "v0.1.3"; # replace with latest version number
  voxelSrc = pkgs.fetchFromGitHub {
    owner = "PrestonHager";
    repo = "VoxelGameEngine";
    rev = "v${version}"; # or use 'main' for latest unstable version
    # Use `nix-shell -p nix-prefetch-github --run "nix-prefetch-github PrestonHager VoxelGameEngine --rev main"`
    # to get the hash and replace it below
    sha256 = "sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";
  };
in {
  nixpkgs.overlays = [
    (import "${voxelSrc}/nix/overlay.nix")
  ];
  environment.systemPackages = [ pkgs.vge-editor ];
}
```

Rebuild and switch as usual (**`nixos-rebuild switch`**).

### `nix-shell` / `nix shell` (no project flake)

From the **repository root**:

```bash
# Prefetch the hash and replace it in the fetch statement below
nix-shell -p nix-prefetch-github --run "nix-prefetch-github PrestonHager VoxelGameEngine --rev main"
# Run a nix shell with the package installed
nix-shell -E '
with import <nixpkgs> {
  overlays = [
    (import "${
      builtins.fetchTarball {
        url = "https://github.com/PrestonHager/VoxelGameEngine/archive/main.tar.gz";
        sha256 = "sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";
      }
    }/nix/overlay.nix")
  ];
};
mkShell {
  packages = [ vge-editor ];
}
'
# Then open the editor window by running
vge-editor
```

See **`nix/overlay.nix`** for additional copy-paste examples.

## Editor (MVP)

The **editor** (`apps/editor`) is a **single executable**: it hosts the UI and, in **embedded** mode (default), an in-process Vulkan view. For a **separate** engine window, run **`--no-embedded`** or start the same binary with the **`engine-runner`** subcommand (see below). If nothing is listening on the IPC port in external mode, the editor spawns **`editor engine-runner`** next to itself. **Push to engine** always saves first, then reloads the level file (IPC in external mode, in-process apply in embedded mode).

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

Use `--embedded` / `-e` to force embedded mode. You can also set `VGE_EMBEDDED=1|0` to override the saved default preference.

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

Lua can drive per-object logic using **level `instance_id`** keys. Script hooks can come from:

- global script file via `VGE_LUA_SCRIPT`
- per-object script assets attached in the level/project

See [`docs/source/scripting.rst`](docs/source/scripting.rst) and `scripts/default_game.lua`.

## Projects workflow

Projects are supported via binary `.vge` files plus project-relative content paths.

- New project scaffolds `<Name>.vge`, `levels/main.vge.json`, `assets/`, and `scripts/`
- Project files enforce relative in-root paths (no absolute or traversal paths)
- Project metadata includes `default_level`, assets, and project-level `vsync_enabled`

See [`docs/source/projects.rst`](docs/source/projects.rst) for details.

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

- **Sphinx** sources: [`docs/source`](docs/source). User guides: [Editor](docs/source/editor.rst), [Projects](docs/source/projects.rst), [Prefabs](docs/source/prefabs.rst), [Scripting](docs/source/scripting.rst). Build:

  ```bash
  pip install -r docs/requirements.txt
  cd docs/source && sphinx-build -b html . ../_build
  ```

- **ReadTheDocs:** add a config when the repo is public.

## CI

GitHub Actions workflows:

- [`.github/workflows/pull-request.yml`](.github/workflows/pull-request.yml): `fmt`, `clippy`, and `test` on pull requests
- [`.github/workflows/main-build-release.yml`](.github/workflows/main-build-release.yml): main-branch multi-OS builds and release gating
- [`.github/workflows/docs-pages.yml`](.github/workflows/docs-pages.yml): Sphinx build and GitHub Pages deployment
