import { invoke } from "@tauri-apps/api/core";

type LogLevel = "info" | "warn" | "error";

const MAX_BUFFER = 80;
const buffer: string[] = [];
let invokeAvailable = true;

function stamp() {
  return new Date().toISOString();
}

function pushBuffer(line: string) {
  buffer.push(line);
  if (buffer.length > MAX_BUFFER) {
    buffer.shift();
  }
}

export function recentBbDiagnostics() {
  return [...buffer];
}

export async function bbLog(level: LogLevel, message: string, detail?: unknown) {
  const detailText =
    detail === undefined
      ? ""
      : typeof detail === "string"
        ? detail
        : JSON.stringify(detail);

  const line = detailText
    ? `[bb-ui ${stamp()}] ${level.toUpperCase()}: ${message} | ${detailText}`
    : `[bb-ui ${stamp()}] ${level.toUpperCase()}: ${message}`;

  pushBuffer(line);

  if (level === "error") {
    console.error(line);
  } else if (level === "warn") {
    console.warn(line);
  } else {
    console.log(line);
  }

  if (!invokeAvailable) {
    return;
  }

  try {
    await invoke("bb_append_log", { line });
  } catch {
    invokeAvailable = false;
  }
}

export function initBbDiagnostics() {
  window.addEventListener("error", (event) => {
    void bbLog("error", "window.error", {
      message: event.message,
      filename: event.filename,
      lineno: event.lineno,
      colno: event.colno,
    });
  });

  window.addEventListener("unhandledrejection", (event) => {
    void bbLog("error", "unhandledrejection", {
      reason: String(event.reason),
    });
  });

  void bbLog("info", "frontend diagnostics ready");
}
