#!/usr/bin/env bash
set -u

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck source=scripts/bb-lib.sh
source "$ROOT/scripts/bb-lib.sh"

bb_init_logging "$ROOT"
bb_report_logs "$ROOT"

if pgrep -af 'target/debug/border-agents' >/dev/null 2>&1; then
  bb_log "border-agents process is still running:"
  pgrep -af 'target/debug/border-agents' | sed 's/^/[BB proc] /'
else
  bb_warn "border-agents process is not running."
fi

if bb_port_listening 17387; then
  bb_log "Gateway is listening on :17387"
else
  bb_warn "Gateway is not listening on :17387"
fi

if bb_port_listening 1420; then
  bb_log "Vite dev server is listening on :1420"
else
  bb_warn "Vite dev server is not listening on :1420"
fi
