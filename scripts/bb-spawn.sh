#!/usr/bin/env bash
#
# Spawn an ADDITIONAL buddy body onto the screen — "easily create a new buddy" (Slice 0).
#
# Unlike bb-body.sh (the single-instance launcher, which stops any running body first), this
# NEVER kills existing instances: each buddy is its own dockable window keyed by BB_BUDDY, and
# they all attach to the one running soul (the soul accepts many bodies). Tuck each to its own
# edge. Requires the soul to be up already — start it with scripts/bb-start.sh first.
#
# Usage:  bb-spawn [buddy]        e.g.  bb-spawn forge   /   bb-spawn hermes
#         (default buddy: forge — the launcher-bearing buddy)
#
# Bind it to a key in your compositor (COSMIC Settings → Custom Shortcuts → command) to get a
# real "summon a buddy" hotkey without a fragile global-key-grabbing daemon.
set -euo pipefail

BIN="bb-desktop-body"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BUDDY="${1:-${BB_BUDDY:-forge}}"

# shellcheck source=scripts/bb-lib.sh
if [[ -f "${ROOT}/scripts/bb-lib.sh" ]]; then
  source "${ROOT}/scripts/bb-lib.sh"
else
  bb_log() { printf '[BB] %s\n' "$*"; }
fi

# Same per-buddy .env config the other launchers load, so a spawned buddy reads its display
# name / provider from one place.
if declare -f bb_source_env >/dev/null 2>&1; then
  bb_source_env "$ROOT"
fi

PORT="${BB_PRESENCE_PORT:-17387}"
if declare -f bb_port_listening >/dev/null 2>&1; then
  if ! bb_port_listening "$PORT"; then
    bb_log "No soul listening on ws://127.0.0.1:${PORT}/border-buddies."
    bb_log "Start it first:  bash scripts/bb-start.sh   (then re-run bb-spawn)."
    exit 1
  fi
fi

bb_log "Building ${BIN} (release)…"
cargo build --release --manifest-path "${ROOT}/desktop-body/Cargo.toml"

bb_log "Spawning buddy '${BUDDY}' alongside any existing buddies…"
# setsid + background + disown: fully detach so the new buddy outlives this shell and does NOT
# share its controlling terminal (so Ctrl-C here won't take the buddy down).
BB_BUDDY="$BUDDY" setsid "${ROOT}/desktop-body/target/release/${BIN}" >/dev/null 2>&1 </dev/null &
spawned_pid=$!
disown "$spawned_pid" 2>/dev/null || true
bb_log "Spawned '${BUDDY}' (pid ${spawned_pid}). Drag it to any edge to tuck; it has attached to the running soul."
