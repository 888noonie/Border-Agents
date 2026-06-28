#!/usr/bin/env bash
# Forge commandeer stack: governance soul + COSMIC frame driver + forge body.
set -u

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck source=scripts/bb-lib.sh
source "$ROOT/scripts/bb-lib.sh"

# shellcheck source=/dev/null
source "$HOME/.cargo/env" 2>/dev/null || true

bb_source_env "$ROOT"

export RUST_BACKTRACE="${RUST_BACKTRACE:-full}"
export RUST_LOG="${RUST_LOG:-border_agents_lib=debug,border_agents=debug,warn}"
export CARGO_TERM_COLOR=never
export BB_TARGET="${BB_TARGET:-firefox}"
export BB_BUDDY=forge

bb_init_logging "$ROOT"
bb_begin_session_log
bb_tee_session

bb_banner "Forge stack (soul + frame driver + body)"
bb_log "Soul: governance gate (npm run soul:dev)"
bb_log "Frame target: ${BB_TARGET}"
bb_log "Body: BB_BUDDY=forge"

soul_pid=""
frame_pid=""
soul_started_by_us=0
frame_started_by_us=0
body_exit_code=0

cleanup() {
  if [[ "${frame_started_by_us}" -eq 1 && -n "${frame_pid}" ]] && kill -0 "${frame_pid}" 2>/dev/null; then
    bb_log "Stopping frame driver (pid ${frame_pid})"
    kill "${frame_pid}" 2>/dev/null || true
  fi
  if [[ "${soul_started_by_us}" -eq 1 && -n "${soul_pid}" ]] && kill -0 "${soul_pid}" 2>/dev/null; then
    bb_log "Stopping soul (pid ${soul_pid})"
    kill "${soul_pid}" 2>/dev/null || true
  fi
}

on_exit() {
  local code=$?
  cleanup
  if [[ "${code}" -ne 0 ]]; then
    bb_print_failure_help "${code}"
  else
    bb_log "Forge stack stopped cleanly."
  fi
  exit "${code}"
}

trap on_exit EXIT INT TERM

if bb_port_listening 17387; then
  bb_warn "Port 17387 already in use — assuming soul is already running."
else
  bb_log "Starting soul (npm run soul:dev)..."
  npm run soul:dev >>"$BB_LOG_SESSION" 2>&1 &
  soul_pid=$!
  soul_started_by_us=1
  if ! bb_wait_for_port 17387 40; then
    bb_error "Soul did not become ready on port 17387."
    exit 1
  fi
  bb_log "Soul ready at ws://127.0.0.1:17387/border-buddies"
fi

if pgrep -x bb-frame-driver >/dev/null 2>&1; then
  bb_log "Frame driver already running — skipping start."
else
  bb_log "Building frame driver (release)..."
  if ! cargo build --release --manifest-path "${ROOT}/desktop-body/Cargo.toml" --bin bb-frame-driver >>"$BB_LOG_SESSION" 2>&1; then
    bb_error "Frame driver build failed."
    exit 1
  fi
  bb_log "Starting frame driver (BB_TARGET=${BB_TARGET})..."
  BB_TARGET="${BB_TARGET}" \
    cargo run --release --manifest-path "${ROOT}/desktop-body/Cargo.toml" --bin bb-frame-driver \
    >>"$BB_LOG_SESSION" 2>&1 &
  frame_pid=$!
  frame_started_by_us=1
  sleep 0.5
  if ! kill -0 "${frame_pid}" 2>/dev/null; then
    bb_warn "Frame driver exited early (COSMIC/Wayland may be unavailable). Body will still start."
    frame_started_by_us=0
    frame_pid=""
  fi
fi

bb_log "Starting forge body (BB_BUDDY=forge npm run body:dev)..."
set +e
BB_BUDDY=forge npm run body:dev
body_exit_code=$?
set -e

if [[ "${body_exit_code}" -ne 0 ]]; then
  bb_error "body:dev failed with exit code ${body_exit_code}"
fi

exit "${body_exit_code}"