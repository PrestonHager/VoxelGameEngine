Agents.md — Open-Source Voxel Engine (Rust + Vulkan)

Mission

Develop an open-source, industry-standard voxel-based game engine in
Rust with: - Vulkan-based rendering - Component-driven architecture
(custom ECS) - Infinite, editable voxel worlds - Integrated GUI editor
(Godot-like workflow) - Cross-language scripting with hot-reloading -
Strong documentation via Sphinx + ReadTheDocs

Core Technical Decisions

Rendering

-   Backend: Vulkan
-   Rust abstraction: ash (low-level)
-   Cross-platform support is mandatory (Windows, Linux, macOS via
    MoltenVK)

Voxel Representation

Models (Static / Prefabs)

-   Sparse Voxel Octree (SVO)

Terrain (Dynamic / Infinite)

-   Chunked grid system
-   Dual Contouring mesh extraction

Workspace Layout (Cargo)

/crates engine-core ecs scene render-vulkan voxel meshing physics assets
scripting api platform logging

/apps editor engine-runner

/shared protocol formats

/docs source

Documentation (human guides)

-   README.md — build, editor quick start, prefab ID table, level JSON overview
-   docs/source/editor.rst — editor panels, workflow, env vars, Push to engine
-   docs/source/prefabs.rst — built-in prefab catalog and stable IDs
-   docs/source/scripting.rst — Lua hooks, VGE_LUA_SCRIPT, ECS instance API

Architecture

Editor and engine run as separate processes communicating via IPC.

Key Systems

-   Infinite world streaming
-   Asset hot reloading
-   Script hot reloading

CI/CD

-   GitHub Actions
-   clippy + rustfmt

Roadmap

Phase 1: ECS + Renderer
Phase 2: Voxel + Meshing
Phase 3: Editor + Assets
Phase 4: Physics + Optimization
Phase 5: Plugins + Networking (planned)

Definition of Done

-   Cross-platform build
-   CI passing
-   Documented
