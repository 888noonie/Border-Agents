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

bb_banner "Border Buddies soul (governance)"
bb_log "Workspace: ${ROOT}"
bb_log "Real action gate — ws://127.0.0.1:${BB_PRESENCE_PORT:-17387}${BB_PRESENCE_PATH:-/border-buddies}"

on_exit() {
  local code=$?
  if [[ "${code}" -ne 0 ]]; then
    bb_print_failure_help "${code}"
  else
    bb_log "Soul stopped cleanly."
  fi
  exit "${code}"
}

trap on_exit EXIT INT TERM

bb_log "Starting soul (npm run soul:dev)..."
set +e
npm run soul:dev
exit $?