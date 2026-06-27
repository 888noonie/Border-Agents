#!/usr/bin/env bash
# Wizard onboarding stack: real soul (BB_SOUL=wizard) + native host body.
# No browser preview or frame driver — the in-torso onboarding panel is the form surface.
set -u

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck source=scripts/bb-lib.sh
source "$ROOT/scripts/bb-lib.sh"

# shellcheck source=/dev/null
source "$HOME/.cargo/env" 2>/dev/null || true

bb_source_env "$ROOT"

export RUST_BACKTRACE="${RUST_BACKTRACE:-full}"
export CARGO_TERM_COLOR=never
export BB_SOUL=wizard
export BB_BUDDY=host

bb_init_logging "$ROOT"
bb_begin_session_log
bb_tee_session

bb_banner "Wizard onboarding (real soul + host body)"
bb_log "Soul: BB_SOUL=wizard (scripts/soul-server.ts)"
bb_log "Body: BB_BUDDY=host"
bb_log "After a crash: npm run report   (or Run Task → BB report)"

soul_pid=""
soul_started_by_us=0
body_exit_code=0

cleanup() {
  if [[ "${soul_started_by_us}" -eq 1 && -n "${soul_pid}" ]] && kill -0 "${soul_pid}" 2>/dev/null; then
    bb_log "Stopping wizard soul (pid ${soul_pid})"
    kill "${soul_pid}" 2>/dev/null || true
  fi
}

on_exit() {
  local code=$?
  cleanup
  if [[ "${code}" -ne 0 ]]; then
    bb_print_failure_help "${code}"
  else
    bb_log "Wizard onboarding stack stopped cleanly."
  fi
  exit "${code}"
}

trap on_exit EXIT INT TERM

if bb_port_listening 17387; then
  bb_warn "Port 17387 already in use — assuming soul is already running (must be BB_SOUL=wizard)."
else
  bb_log "Starting wizard soul (npm run soul:wizard)..."
  npm run soul:wizard >>"$BB_LOG_SESSION" 2>&1 &
  soul_pid=$!
  soul_started_by_us=1
  if ! bb_wait_for_port 17387 40; then
    bb_error "Wizard soul did not become ready on port 17387."
    exit 1
  fi
  bb_log "Wizard soul ready at ws://127.0.0.1:17387/border-buddies"
fi

bb_log "Starting host body (BB_BUDDY=host npm run body:dev)..."
set +e
BB_BUDDY=host npm run body:dev
body_exit_code=$?
set -e

if [[ "${body_exit_code}" -ne 0 ]]; then
  bb_error "body:dev failed with exit code ${body_exit_code}"
fi

exit "${body_exit_code}"