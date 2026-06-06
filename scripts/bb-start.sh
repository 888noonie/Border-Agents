#!/usr/bin/env bash
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
export BB_LOG_HITBOXES="${BB_LOG_HITBOXES:-0}"

bb_init_logging "$ROOT"
export BB_LOG_EVENTS
bb_begin_session_log
bb_tee_session

bb_banner "Border Buddies start (gateway + desktop)"
bb_log "After a crash/freeze run: npm run report   (or Cursor: Terminal → Run Task → BB report)"
bb_log "Logs folder: ${ROOT}/.bb-logs/"
bb_log "Workspace: ${ROOT}"
bb_log "Node: $(command -v node) ($(node --version 2>/dev/null || echo unknown))"
bb_log "Cargo: $(command -v cargo) ($(cargo --version 2>/dev/null || echo unknown))"

gateway_pid=""
gateway_started_by_us=0
desktop_exit_code=0

cleanup() {
  if [[ "${gateway_started_by_us}" -eq 1 && -n "${gateway_pid}" ]] && kill -0 "${gateway_pid}" 2>/dev/null; then
    bb_log "Stopping dev gateway (pid ${gateway_pid})"
    kill "${gateway_pid}" 2>/dev/null || true
  fi
}

on_exit() {
  local code=$?
  cleanup

  if [[ "${code}" -ne 0 ]]; then
    bb_print_failure_help "${code}"
  else
    bb_log "Border Buddies stopped cleanly."
    if [[ -s "${BB_LOG_EVENTS:-}" ]]; then
      bb_log "Session had event log entries — review: ${BB_LOG_EVENTS}"
      tail -n 15 "$BB_LOG_EVENTS" | sed 's/^/[BB event] /'
    fi
  fi

  exit "${code}"
}

trap on_exit EXIT INT TERM

if bb_port_listening 17387; then
  bb_log "Gateway already listening on ws://127.0.0.1:17387/border-buddies"
else
  bb_log "Starting Hermes dev gateway..."
  npm run gateway:dev >>"$BB_LOG_SESSION" 2>&1 &
  gateway_pid=$!
  gateway_started_by_us=1
  bb_log "Gateway process pid: ${gateway_pid}"

  if ! bb_wait_for_port 17387 40; then
    bb_warn "Gateway is not listening yet; desktop will still start."
    if ! kill -0 "${gateway_pid}" 2>/dev/null; then
      bb_error "Gateway process exited early. Recent gateway output:"
      tail -n 20 "$BB_LOG_SESSION" 2>/dev/null | sed 's/^/[BB gateway] /' || true
    fi
  else
    bb_log "Gateway ready at ws://127.0.0.1:17387/border-buddies"
  fi
fi

bb_log "Starting desktop overlay (npm run desktop:dev)..."
set +e
npm run desktop:dev
desktop_exit_code=$?
set -e

if [[ "${desktop_exit_code}" -ne 0 ]]; then
  bb_error "desktop:dev failed with exit code ${desktop_exit_code}"
  bb_log "Recent session output:"
  tail -n 40 "$BB_LOG_SESSION" 2>/dev/null | sed 's/^/[BB tail] /' || true
fi

exit "${desktop_exit_code}"
