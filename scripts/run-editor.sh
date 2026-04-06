#!/usr/bin/env bash
# Start the editor GUI. If nothing is listening on VGE_IPC_PORT, the editor spawns
# engine-runner from the same directory as the editor binary.
set -euo pipefail
cd "$(dirname "$0")/.."
export VGE_IPC_PORT="${VGE_IPC_PORT:-7878}"

if [[ "${1:-}" == "--release" ]]; then
  exec cargo run -p editor --release
else
  exec cargo run -p editor
fi
