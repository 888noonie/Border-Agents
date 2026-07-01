#!/usr/bin/env bash
#
# Single-instance launcher for the native desktop presence body.
#
# Starting the body twice leaves two buddies on screen (each `cargo run` spawns a
# fresh process, and the old one keeps running). This script guarantees exactly one:
# it stops any existing `bb-desktop-body` first, waits for it to actually exit, then
# builds and runs a fresh one. Use it instead of a bare `cargo run`.
#
# Env passthrough: BB_BUDDY, BB_MARGIN_LEFT/TOP, BB_OUTPUT_INDEX, BB_PRESENCE_URL,
# WAYLAND_DISPLAY, etc. are all honoured by the body as usual.
set -euo pipefail

BIN="bb-desktop-body"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# shellcheck source=scripts/bb-lib.sh
if [[ -f "${ROOT}/scripts/bb-lib.sh" ]]; then
  source "${ROOT}/scripts/bb-lib.sh"
else
  bb_log() { printf '[BB] %s\n' "$*"; }
fi

# Load .env (HERMES_NAME / HERMES_PROVIDER / HERMES_MODEL, etc.) like the other launchers
# (bb-start, bb-soul, …) do — so the canonical single-instance body launcher sees the same
# per-buddy config and the native head reads its display name + provider from one place.
if declare -f bb_source_env >/dev/null 2>&1; then
  bb_source_env "$ROOT"
fi

# --- stop any running instance(s) so we never stack buddies ---
stop_existing() {
  # -x: exact process-name match, so we never hit the cargo/bash wrappers.
  if ! pgrep -x "$BIN" >/dev/null 2>&1; then
    return 0
  fi

  bb_log "Stopping existing ${BIN} instance(s): $(pgrep -x "$BIN" | tr '\n' ' ')"
  pkill -x "$BIN" 2>/dev/null || true

  # Wait up to ~2s for a graceful exit before escalating to SIGKILL.
  for _ in $(seq 1 20); do
    pgrep -x "$BIN" >/dev/null 2>&1 || return 0
    sleep 0.1
  done

  bb_log "Instance(s) still alive — sending SIGKILL"
  pkill -9 -x "$BIN" 2>/dev/null || true
  sleep 0.2
}

stop_existing

bb_log "Building ${BIN} (release)…"
cargo build --release --manifest-path "${ROOT}/desktop-body/Cargo.toml"

# The laminal path is the one we live in: default to the ring skin unless the caller pins a
# skin explicitly. `BB_SKIN=clay bb-body.sh` still restores the frozen figure. (Keeping the
# figure the default would let BB_SKIN=ring silently rot — see docs/laminal-ring-pivot.md.)
export BB_SKIN="${BB_SKIN:-ring}"

bb_log "Starting a single ${BIN} (buddy=${BB_BUDDY:-hermes}, skin=${BB_SKIN})"
# exec so signals (Ctrl-C, task stop) reach the body directly.
exec "${ROOT}/desktop-body/target/release/${BIN}" "$@"
