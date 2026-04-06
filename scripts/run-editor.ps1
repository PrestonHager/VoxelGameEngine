# Start the editor GUI. If nothing is listening on VGE_IPC_PORT, the editor spawns
# engine-runner from the same directory as editor.exe (after `cargo run`, that is target/debug or release).
param(
    [int]$Port = 7878,
    [switch]$Release
)

$ErrorActionPreference = "Stop"
Set-Location (Split-Path -Parent $PSScriptRoot)
$env:VGE_IPC_PORT = "$Port"

if ($Release) {
    cargo run -p editor --release
} else {
    cargo run -p editor
}
