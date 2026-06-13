// The real soul runtime — a presence-protocol WebSocket server that runs the ACTUAL
// governance action gate.
//
// This is the piece the dev gateway could never be: `scripts/gateway-dev.mjs` is a
// disposable JS relay that cannot import the TS core, so its `action_request` handler only
// emits a `gateway-stub` `action_result`. This server runs under tsx, imports
// `handleActionRequest` directly, and returns a REAL ActionReceipt over the wire — the same
// deterministic gate (`src/core/actionGate.ts`) the browser body calls in-process.
//
// It is the missing half of roadmap Step 4: the native Rust body stops talking to a stub and
// gets governance parity with the browser. AGENTS.md law 7 holds — the body emits an
// `action_request` (or types `/review` → `said`); the SOUL authorizes; the body only renders
// the `action_result` it gets back.
//
// Run:  npm run soul:dev          (binds ws://127.0.0.1:17387/border-buddies — the body's default)
//       BB_POSTURE=play npm run soul:dev
// Then: BB_BUDDY=owl npm run body:dev   and type `/review` on the body, then `/confirm`.
//
// Posture: BB_POSTURE = work | play | private (default work). Ledger persists to a JSON file
// (BB_SOUL_LEDGER, default <tmp>/bb-soul-ledger.json) so receipts survive restarts and are
// inspectable. There is NO auth on this localhost socket — dev posture only (see Step 4 §10).

import { createServer } from "node:http";
import { readFileSync, writeFileSync, existsSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { WebSocketServer, type WebSocket } from "ws";

import { handleActionRequest, parseActionCommand } from "../src/soulActions";
import { createLiveRepoEditExecutor } from "./liveEffectorExecutors";
import { PRESENCE_PROTOCOL, parsePresenceMessage, presence } from "../src/presenceProtocol";
import { createDefaultBuddySettings, BUDDY_PROFILES, type BuddyProfile } from "../src/buddyProfiles";
import type { UserPosture } from "../src/core";

const PORT = Number(process.env.BB_PRESENCE_PORT ?? 17387);
const PATH = process.env.BB_PRESENCE_PATH ?? "/border-buddies";
const POSTURE = normalizePosture(process.env.BB_POSTURE);
const LEDGER_PATH = process.env.BB_SOUL_LEDGER ?? join(tmpdir(), "bb-soul-ledger.json");

function normalizePosture(value: string | undefined): UserPosture {
  return value === "play" || value === "private" ? value : "work";
}

function log(message: string, extra?: Record<string, unknown>) {
  const stamp = new Date().toISOString();
  console.log(`[soul ${stamp}] ${message}${extra ? " " + JSON.stringify(extra) : ""}`);
}

// A Node-side Storage shim backed by a JSON file. The action ledger (src/receiptLedger.ts)
// defaults to `window.localStorage`, which does not exist in Node — so the soul MUST inject
// its own durable store. This keeps receipts inspectable across restarts (cat the file).
function fileStorage(path: string): Storage {
  const read = (): Record<string, string> => {
    if (!existsSync(path)) return {};
    try {
      return JSON.parse(readFileSync(path, "utf8")) as Record<string, string>;
    } catch {
      return {};
    }
  };
  const write = (data: Record<string, string>) => writeFileSync(path, JSON.stringify(data), "utf8");
  return {
    get length() {
      return Object.keys(read()).length;
    },
    clear: () => write({}),
    getItem: (key) => read()[key] ?? null,
    key: (index) => Object.keys(read())[index] ?? null,
    removeItem: (key) => {
      const data = read();
      delete data[key];
      write(data);
    },
    setItem: (key, value) => {
      const data = read();
      data[key] = String(value);
      write(data);
    },
  };
}

function profileFor(buddy: string): BuddyProfile {
  // Bodies address by persona id (hermes/crab/owl/fox); profiles are keyed the same way.
  return BUDDY_PROFILES[buddy] ?? Object.values(BUDDY_PROFILES)[0];
}

const storage = fileStorage(LEDGER_PATH);
// Live, disk-writing executors rooted at the repo cwd, sandboxed to .border-agents/proofs/.
// The soul-server is a Node context, so unlike the browser body it injects real executors.
const EXECUTORS = { repo_edit: createLiveRepoEditExecutor() };
// Per-buddy pending effector, set when the gate returns needs_confirmation so a later
// `/confirm` knows what it is confirming. Mirrors the browser composer's pendingEffector.
const pending = new Map<string, string>();

/**
 * Run one effector request through the real gate and send the body the resulting cue. Records
 * / clears the pending effector for the confirm round-trip. Returns the receipt for logging.
 */
function authorizeAndReply(
  socket: WebSocket,
  buddy: string,
  effectorId: string,
  confirmed: boolean,
  requestId?: string,
) {
  const { receipt, result, execution } = handleActionRequest({
    buddy,
    effectorId,
    settings: createDefaultBuddySettings(profileFor(buddy)),
    posture: POSTURE,
    history: [],
    // Real disk-writing executors, sandboxed to .border-agents/proofs/. They only run on an
    // `allow` carrying an intent; until the body emits a typed intent over the wire, repo_edit
    // requests authorize but skip execution. No-execute-on-block still holds either way.
    executors: EXECUTORS,
    confirmed,
    requestId,
    storage,
  });
  if (receipt.decision === "needs_confirmation") {
    pending.set(buddy, effectorId);
  } else {
    pending.delete(buddy);
  }
  socket.send(JSON.stringify(result));
  log("authorized", {
    buddy,
    effector: effectorId,
    confirmed,
    decision: receipt.decision,
    receiptId: receipt.receipt_id,
    execution: execution ? `${execution.outcome}${execution.executor_called ? "" : " (skipped)"}` : "none",
  });
  return receipt;
}

const server = createServer((_request, response) => {
  response.writeHead(426, { "content-type": "text/plain" });
  response.end("Upgrade required: this is a presence WebSocket soul.\n");
});

const wss = new WebSocketServer({ server, path: PATH });
const clients = new Set<WebSocket>();

// Frame driver target lifecycle cues — relay to bodies, same as gateway-dev.mjs.
const TO_BODY_RELAY_KINDS = new Set(["target_acquired", "target_moved", "target_lost"]);

function relayPresenceCue(sender: WebSocket, message: Record<string, unknown>) {
  const json = JSON.stringify(message);
  let count = 0;
  for (const client of clients) {
    if (client === sender || client.readyState !== client.OPEN) {
      continue;
    }
    client.send(json);
    count += 1;
  }
  log("relayed presence cue", { kind: message.kind, buddy: message.buddy, clients: count });
}

wss.on("connection", (socket, request) => {
  clients.add(socket);
  const remote = request.socket.remoteAddress ?? "unknown";
  log(`client connected from ${remote}`);

  socket.on("message", (raw) => {
    let parsed: unknown;
    try {
      parsed = JSON.parse(safeString(raw));
    } catch {
      return; // not JSON — ignore, never crash
    }

    // Frame driver (and future platform helpers) emit target_* cues; bodies consume them.
    if (typeof parsed === "object" && parsed !== null) {
      const wire = parsed as Record<string, unknown>;
      if (
        wire.protocol === PRESENCE_PROTOCOL &&
        typeof wire.kind === "string" &&
        TO_BODY_RELAY_KINDS.has(wire.kind)
      ) {
        relayPresenceCue(socket, wire);
        return;
      }
    }

    const message = parsePresenceMessage(parsed);
    // Drop malformed / unknown cues; never crash the soul (mirrors the TS strict parser).
    if (!message) return;
    const buddy = message.buddy;

    switch (message.kind) {
      case "attached": {
        // Handshake: acknowledge with a hydrate snapshot + a greeting so the body settles in.
        socket.send(JSON.stringify(presence.hydrate(buddy, { emotion: "neutral" })));
        socket.send(JSON.stringify(presence.express(buddy, "happy")));
        socket.send(
          JSON.stringify(
            presence.say(buddy, `Soul attached. Type /review to inspect receipts (posture: ${POSTURE}).`),
          ),
        );
        log("attached → hydrate", { buddy });
        return;
      }

      case "said": {
        // The user typed on the body. Action slash-commands route to the gate; everything
        // else is free text the soul would forward to a provider (not wired in this server).
        const command = parseActionCommand(message.text);
        if (!command) {
          log("said (free text — no provider wired in soul-server)", { buddy, text: message.text });
          return;
        }
        if (command.kind === "confirm") {
          const effectorId = pending.get(buddy);
          if (!effectorId) {
            socket.send(JSON.stringify(presence.say(buddy, "Nothing is awaiting confirmation.")));
            return;
          }
          authorizeAndReply(socket, buddy, effectorId, true);
          return;
        }
        authorizeAndReply(socket, buddy, command.effectorId, false);
        return;
      }

      case "action_request": {
        // A body (or future surface) emitted the typed action cue directly.
        authorizeAndReply(socket, buddy, message.effector, message.confirmed === true, message.requestId);
        return;
      }

      default:
        // Presentation/interaction events (clicked/grabbed/dropped/...): nothing to authorize.
        return;
    }
  });

  socket.on("close", () => {
    clients.delete(socket);
    log(`client disconnected from ${remote}`);
  });
  socket.on("error", (error) => log("socket error", { error: String(error) }));
});

function safeString(raw: unknown): string {
  if (typeof raw === "string") return raw;
  if (raw instanceof Buffer) return raw.toString("utf8");
  return String(raw);
}

server.listen(PORT, "127.0.0.1", () => {
  log(`real soul listening on ws://127.0.0.1:${PORT}${PATH}`, { posture: POSTURE, ledger: LEDGER_PATH });
});
