#!/usr/bin/env bash
set -u

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck source=scripts/bb-lib.sh
source "$ROOT/scripts/bb-lib.sh"

bb_source_env "$ROOT"

bb_init_logging "$ROOT"
bb_begin_session_log
bb_tee_session

bb_banner "Border Buddies browser preview"
bb_log "Workspace: ${ROOT}"
bb_log "UI harness at http://127.0.0.1:1420 — connects to soul/gateway on ws://127.0.0.1:17387"

on_exit() {
  local code=$?
  if [[ "${code}" -ne 0 ]]; then
    bb_print_failure_help "${code}"
  else
    bb_log "Browser preview stopped cleanly."
  fi
  exit "${code}"
}

trap on_exit EXIT INT TERM

bb_log "Starting browser buddies (npm run dev)..."
set +e
npm run dev
exit $?