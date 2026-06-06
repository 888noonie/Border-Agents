#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

rm -rf dist node_modules/.vite .vite target src-tauri/target

echo "Cleaned Border Buddies build and dev caches in $ROOT"
