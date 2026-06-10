#!/usr/bin/env bash
set -euo pipefail

port_pids="$(
  ss -ltnp 2>/dev/null \
    | awk '/:1420 / { print $NF }' \
    | sed -n 's/.*pid=\([0-9][0-9]*\).*/\1/p' \
    | sort -u
)"

gateway_pids="$(
  ss -ltnp 2>/dev/null \
    | awk '/:17387 / { print $NF }' \
    | sed -n 's/.*pid=\([0-9][0-9]*\).*/\1/p' \
    | sort -u
)"

process_pids="$(
  pgrep -f 'target/debug/border-agents|tauri dev|border-agents|gateway-dev|vite --host 127.0.0.1 --port 1420' 2>/dev/null \
    | sort -u || true
)"

# Native desktop presence body — matched by exact name so we don't catch wrappers.
body_pids="$(pgrep -x 'bb-desktop-body' 2>/dev/null | sort -u || true)"

pids="$(printf '%s\n%s\n%s\n%s\n' "$port_pids" "$gateway_pids" "$process_pids" "$body_pids" | awk 'NF' | sort -u)"

if [ -z "$pids" ]; then
  echo "No Border Buddies desktop, gateway, or dev-server processes found."
  exit 0
fi

echo "Stopping Border Buddies processes: $pids"
kill $pids 2>/dev/null || true
sleep 1
kill -9 $pids 2>/dev/null || true
echo "Stopped Border Buddies desktop/dev/gateway processes."
echo "Diagnostic report: cd Border-Agents && npm run report"
