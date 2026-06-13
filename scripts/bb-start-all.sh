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
export BB_TARGET="${BB_TARGET:-firefox}"

# Which soul owns ws://127.0.0.1:17387/border-buddies — only one can bind the port.
#   governance (default) — real action gate (npm run soul:dev)
#   gateway            — dev gateway with provider chat (npm run gateway:dev)
#   wizard             — onboarding persona (npm run gateway:wizard)
export BB_START_SOUL="${BB_START_SOUL:-governance}"
# Browser buddies UI harness (Vite on :1420). Set BB_START_BROWSER=0 to skip.
export BB_START_BROWSER="${BB_START_BROWSER:-1}"

bb_init_logging "$ROOT"
export BB_LOG_EVENTS
bb_begin_session_log
bb_tee_session

bb_banner "Border Buddies start ALL (soul + browser + frame + body)"
bb_log "Soul mode: ${BB_START_SOUL}"
bb_log "Browser preview: ${BB_START_BROWSER}"
bb_log "Frame target: ${BB_TARGET}"
bb_log "After a crash/freeze run: npm run report   (or Run Task → Report)"
bb_log "Logs folder: ${ROOT}/.bb-logs/"
bb_log "Workspace: ${ROOT}"
bb_log "Node: $(command -v node) ($(node --version 2>/dev/null || echo unknown))"
bb_log "Cargo: $(command -v cargo) ($(cargo --version 2>/dev/null || echo unknown))"

soul_pid=""
frame_pid=""
browser_pid=""
soul_started_by_us=0
frame_started_by_us=0
browser_started_by_us=0
body_exit_code=0

soul_start_cmd() {
  case "${BB_START_SOUL}" in
    wizard)
      echo "gateway:wizard"
      ;;
    gateway | echo | dev)
      echo "gateway:dev"
      ;;
    governance | soul | *)
      echo "soul:dev"
      ;;
  esac
}

cleanup() {
  if [[ "${browser_started_by_us}" -eq 1 && -n "${browser_pid}" ]] && kill -0 "${browser_pid}" 2>/dev/null; then
    bb_log "Stopping browser preview (pid ${browser_pid})"
    kill "${browser_pid}" 2>/dev/null || true
  fi
  if [[ "${frame_started_by_us}" -eq 1 && -n "${frame_pid}" ]] && kill -0 "${frame_pid}" 2>/dev/null; then
    bb_log "Stopping frame driver (pid ${frame_pid})"
    kill "${frame_pid}" 2>/dev/null || true
  fi
  if [[ "${soul_started_by_us}" -eq 1 && -n "${soul_pid}" ]] && kill -0 "${soul_pid}" 2>/dev/null; then
    bb_log "Stopping soul process (pid ${soul_pid})"
    kill "${soul_pid}" 2>/dev/null || true
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
  bb_warn "Port 17387 already in use — assuming soul/gateway is already running."
else
  local_cmd="$(soul_start_cmd)"
  bb_log "Starting soul (${local_cmd})..."
  npm run "${local_cmd}" >>"$BB_LOG_SESSION" 2>&1 &
  soul_pid=$!
  soul_started_by_us=1
  bb_log "Soul process pid: ${soul_pid}"

  if ! bb_wait_for_port 17387 40; then
    bb_error "Soul/gateway did not become ready on port 17387."
    if ! kill -0 "${soul_pid}" 2>/dev/null; then
      bb_error "Soul process exited early. Recent output:"
      tail -n 20 "$BB_LOG_SESSION" 2>/dev/null | sed 's/^/[BB soul] /' || true
    fi
    exit 1
  fi
  bb_log "Soul ready at ws://127.0.0.1:17387/border-buddies"
fi

if [[ "${BB_START_BROWSER}" == "0" ]]; then
  bb_log "Browser preview disabled (BB_START_BROWSER=0)."
elif bb_port_listening 1420; then
  bb_log "Browser preview already listening on http://127.0.0.1:1420"
else
  bb_log "Starting browser buddies (npm run dev)..."
  npm run dev >>"$BB_LOG_SESSION" 2>&1 &
  browser_pid=$!
  browser_started_by_us=1
  bb_log "Browser preview pid: ${browser_pid}"

  if bb_wait_for_port 1420 40; then
    bb_log "Browser preview ready at http://127.0.0.1:1420"
  else
    bb_warn "Browser preview is not listening yet; native body will still start."
    if ! kill -0 "${browser_pid}" 2>/dev/null; then
      bb_warn "Browser preview process exited early."
      browser_started_by_us=0
      browser_pid=""
    fi
  fi
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
  bb_log "Frame driver pid: ${frame_pid}"
  sleep 0.5
  if ! kill -0 "${frame_pid}" 2>/dev/null; then
    bb_warn "Frame driver exited early (COSMIC/Wayland may be unavailable). Body will still start."
    frame_started_by_us=0
    frame_pid=""
  fi
fi

bb_log "Starting native desktop body (npm run body:dev)..."
set +e
npm run body:dev
body_exit_code=$?
set -e

if [[ "${body_exit_code}" -ne 0 ]]; then
  bb_error "body:dev failed with exit code ${body_exit_code}"
  bb_log "Recent session output:"
  tail -n 40 "$BB_LOG_SESSION" 2>/dev/null | sed 's/^/[BB tail] /' || true
fi

exit "${body_exit_code}"