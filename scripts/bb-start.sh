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

# Which soul process backs the body. Defaults to the real governance soul (soul-server,
# which returns live ActionReceipts). Set BB_SOUL_SCRIPT=gateway:dev for the old provider-chat
# dev gateway (stub governance) as an escape hatch until the unified soul lands (Slice 1).
BB_SOUL_SCRIPT="${BB_SOUL_SCRIPT:-soul:dev}"
export BB_SOUL_SCRIPT

# The on-screen buddy. Defaults to "crab" — the body PERSONA that wears the Forge governance
# identity (resolveManifestId("crab") → "forge"). Using the persona id (not the governance id
# "forge", which has no body profile) gives the proper sprite/colour AND the launcher reach
# grants (open_vscode/open_cursor/open_terminal) the bloom dial opens. Override with
# BB_BUDDY=hermes for the wizard/onboarding persona. The code-level default (desktop-body) stays
# hermes, so a direct `npm run body:dev` is unaffected.
export BB_BUDDY="${BB_BUDDY:-crab}"

bb_banner "Border Buddies start (${BB_SOUL_SCRIPT} + desktop)"
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
    bb_log "Stopping soul (pid ${gateway_pid})"
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
  bb_log "Soul already listening on ws://127.0.0.1:17387/border-buddies"
else
  bb_log "Starting soul (npm run ${BB_SOUL_SCRIPT})..."
  npm run "${BB_SOUL_SCRIPT}" >>"$BB_LOG_SESSION" 2>&1 &
  gateway_pid=$!
  gateway_started_by_us=1
  bb_log "Soul process pid: ${gateway_pid}"

  if ! bb_wait_for_port 17387 40; then
    bb_warn "Soul is not listening yet; desktop will still start."
    if ! kill -0 "${gateway_pid}" 2>/dev/null; then
      bb_error "Soul process exited early. Recent soul output:"
      tail -n 20 "$BB_LOG_SESSION" 2>/dev/null | sed 's/^/[BB soul] /' || true
    fi
  else
    bb_log "Soul ready at ws://127.0.0.1:17387/border-buddies"
  fi
fi

bb_log "Starting native desktop body (npm run body:dev)..."
set +e
npm run body:dev
desktop_exit_code=$?
set -e

if [[ "${desktop_exit_code}" -ne 0 ]]; then
  bb_error "body:dev failed with exit code ${desktop_exit_code}"
  bb_log "Recent session output:"
  tail -n 40 "$BB_LOG_SESSION" 2>/dev/null | sed 's/^/[BB tail] /' || true
fi

exit "${desktop_exit_code}"
