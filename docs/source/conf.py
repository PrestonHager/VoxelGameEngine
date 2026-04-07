# Sphinx configuration — extend when API docs are generated from Rust.

project = "VoxelGameEngine"
copyright = "VoxelGameEngine contributors"
author = "VoxelGameEngine contributors"
extensions: list[str] = []
exclude_patterns: list[str] = ["_build", "Thumbs.db", ".DS_Store"]
html_theme = "alabaster"
html_baseurl = "https://vge.prestonhager.com"
html_extra_path = ["CNAME"]
