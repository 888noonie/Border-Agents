#!/usr/bin/env bash
set -u

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck source=scripts/bb-lib.sh
source "$ROOT/scripts/bb-lib.sh"

# shellcheck source=/dev/null
source "$HOME/.cargo/env" 2>/dev/null || true

bb_source_env "$ROOT"

export BB_TARGET="${BB_TARGET:-firefox}"

bb_init_logging "$ROOT"
bb_begin_session_log
bb_tee_session

bb_banner "Border Buddies frame driver"
bb_log "Workspace: ${ROOT}"
bb_log "Tracking window match: ${BB_TARGET}"

on_exit() {
  local code=$?
  if [[ "${code}" -ne 0 ]]; then
    bb_print_failure_help "${code}"
  else
    bb_log "Frame driver stopped cleanly."
  fi
  exit "${code}"
}

trap on_exit EXIT INT TERM

bb_log "Building frame driver (release)..."
set +e
cargo build --release --manifest-path "${ROOT}/desktop-body/Cargo.toml" --bin bb-frame-driver
build_code=$?
set -e
if [[ "${build_code}" -ne 0 ]]; then
  exit "${build_code}"
fi

bb_log "Starting frame driver..."
exec cargo run --release --manifest-path "${ROOT}/desktop-body/Cargo.toml" --bin bb-frame-driver