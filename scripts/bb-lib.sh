#!/usr/bin/env bash

bb_ts() {
  date '+%Y-%m-%d %H:%M:%S'
}

bb_log() {
  printf '[BB %s] %s\n' "$(bb_ts)" "$*"
}

bb_warn() {
  printf '[BB %s] WARNING: %s\n' "$(bb_ts)" "$*" >&2
}

bb_error() {
  printf '[BB %s] ERROR: %s\n' "$(bb_ts)" "$*" >&2
}

bb_banner() {
  printf '\n══════════════════════════════════════════════════════════════\n'
  printf '[BB %s] %s\n' "$(bb_ts)" "$*"
  printf '══════════════════════════════════════════════════════════════\n'
}

bb_init_logging() {
  local root="$1"
  BB_LOG_DIR="${root}/.bb-logs"
  mkdir -p "$BB_LOG_DIR"
  BB_LOG_SESSION="${BB_LOG_DIR}/bb-start-$(date +%Y%m%d-%H%M%S).log"
  BB_LOG_LATEST="${BB_LOG_DIR}/bb-latest.log"
  BB_LOG_EVENTS="${BB_LOG_DIR}/bb-events.log"
  export BB_LOG_DIR BB_LOG_SESSION BB_LOG_LATEST BB_LOG_EVENTS
}

bb_begin_session_log() {
  : >"$BB_LOG_SESSION"
  : >"$BB_LOG_EVENTS"
  ln -sf "$BB_LOG_SESSION" "$BB_LOG_LATEST"
  bb_log "Session log: $BB_LOG_SESSION"
  bb_log "Latest log:  $BB_LOG_LATEST"
  bb_log "Events log:  $BB_LOG_EVENTS (Rust panics, hitbox errors, UI errors)"
}

bb_tee_session() {
  exec > >(tee -a "$BB_LOG_SESSION") 2>&1
}

bb_port_listening() {
  local port="$1"
  ss -ltn 2>/dev/null | awk -v pattern=":${port}$" '$4 ~ pattern { found = 1 } END { exit(found ? 0 : 1) }'
}

bb_wait_for_port() {
  local port="$1"
  local tries="${2:-30}"
  local attempt

  for ((attempt = 1; attempt <= tries; attempt++)); do
    if bb_port_listening "$port"; then
      bb_log "Port ${port} is listening (attempt ${attempt}/${tries})"
      return 0
    fi
    sleep 0.2
  done

  bb_error "Port ${port} did not become ready after ${tries} attempts"
  return 1
}

bb_source_env() {
  local root="$1"
  local env_file=""
  local line key value

  if [[ -f "${root}/.env" ]]; then
    env_file="${root}/.env"
  elif [[ -f "${root}/components/buddy/.env" ]]; then
    env_file="${root}/components/buddy/.env"
  fi

  if [[ -z "$env_file" ]]; then
    return 0
  fi

  while IFS= read -r line || [[ -n "$line" ]]; do
    line="${line%$'\r'}"
    line="${line#"${line%%[![:space:]]*}"}"
    line="${line%"${line##*[![:space:]]}"}"

    if [[ -z "$line" || "$line" == \#* || "$line" != *=* ]]; then
      continue
    fi

    key="${line%%=*}"
    value="${line#*=}"
    key="${key%"${key##*[![:space:]]}"}"
    value="${value#"${value%%[![:space:]]*}"}"

    if [[ ! "$key" =~ ^[A-Za-z_][A-Za-z0-9_]*$ ]]; then
      bb_warn "Skipping invalid environment key in ${env_file}: ${key}"
      continue
    fi

    if [[ "$value" == \"*\" && "$value" == *\" && "${#value}" -ge 2 ]]; then
      value="${value:1:${#value}-2}"
    elif [[ "$value" == \'*\' && "$value" == *\' && "${#value}" -ge 2 ]]; then
      value="${value:1:${#value}-2}"
    fi

    printf -v "$key" '%s' "$value"
    export "$key"
  done <"$env_file"

  bb_log "Loaded Hermes environment from ${env_file}"
}

bb_print_failure_help() {
  local exit_code="$1"
  bb_error "Border Buddies exited with code ${exit_code}"
  bb_log "Copy the lines below (or attach the log files) when reporting issues."
  if [[ -n "${BB_LOG_SESSION:-}" ]]; then
    bb_log "Session log: ${BB_LOG_SESSION}"
    bb_log "Latest log:  ${BB_LOG_LATEST}"
  fi
  if [[ -n "${BB_LOG_EVENTS:-}" && -s "${BB_LOG_EVENTS}" ]]; then
    bb_log "Recent events from ${BB_LOG_EVENTS}:"
    tail -n 30 "$BB_LOG_EVENTS" | sed 's/^/[BB event] /'
  else
    bb_warn "No bb-events.log entries yet (native/UI errors appear here)."
  fi
  bb_log "Debug env: RUST_BACKTRACE=${RUST_BACKTRACE:-unset} RUST_LOG=${RUST_LOG:-unset}"
  bb_log "Run: bash scripts/bb-report.sh"
}

bb_report_logs() {
  local root="${1:-.}"
  local log_dir="${root}/.bb-logs"
  bb_banner "Border Buddies diagnostic report"
  if [[ ! -d "$log_dir" ]]; then
    bb_warn "No .bb-logs directory at ${log_dir}"
    return 1
  fi
  for file in "$log_dir"/bb-events.log "$log_dir"/bb-latest.log; do
    if [[ -f "$file" ]]; then
      bb_log "---- ${file} (last 40 lines) ----"
      tail -n 40 "$file" | sed 's/\x1b\[[0-9;]*m//g'
    fi
  done
}
