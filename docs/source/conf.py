# Sphinx configuration — extend when API docs are generated from Rust.

project = "VoxelGameEngine"
copyright = "VoxelGameEngine contributors"
author = "VoxelGameEngine contributors"
extensions: list[str] = []
exclude_patterns: list[str] = ["_build", "Thumbs.db", ".DS_Store"]
html_theme = "alabaster"
