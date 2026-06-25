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
// Then: BB_BUDDY=aether AETHER_NAME=Aether BB_LMSTUDIO_MODEL=<loaded-model> npm run body:dev
// Use the perimeter switcher to request private_local_chat, /confirm it, then type locally.
//
// Posture: BB_POSTURE = work | play | private (default work). Ledger persists to a JSON file
// (BB_SOUL_LEDGER, default <tmp>/bb-soul-ledger.json) so receipts survive restarts and are
// inspectable. There is NO auth on this localhost socket — dev posture only (see Step 4 §10).

import { createServer } from "node:http";
import { readFileSync, writeFileSync, existsSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { WebSocketServer, type WebSocket } from "ws";

import {
  handleActionRequest,
  parseActionCommand,
  presenceIntentToActionIntent,
  decisionEmotion,
  routeHealthFromSoul,
} from "../src/soulActions";
import { buildExecutionReceipt } from "../src/effectorExecutors";
import { createLiveLauncherExecutor, createLiveRepoEditExecutor } from "./liveEffectorExecutors";
import { createLiveLocalChatConnector, type LocalChatMessage } from "./liveLocalChat";
import {
  PRESENCE_PROTOCOL,
  parsePresenceMessage,
  presence,
  type PresenceActionIntent,
  type PresenceSurfaceDescriptor,
} from "../src/presenceProtocol";
import { createDefaultBuddySettings, BUDDY_PROFILES, type BuddyProfile } from "../src/buddyProfiles";
import { EFFECTOR_SPECS, manifestEntry, ROUTE_PROVIDER_LABELS, resolveManifestId, type EffectorId } from "../src/buddyManifest";
import { getSurface, surfaceAvailability, surfaceHydrationList, type SurfaceId } from "../src/surfaceManifest";
import type { ActionIntent, ActionReceipt, ActionRoute, UserPosture } from "../src/core";
import type { SessionChatLine } from "../src/liveGovernance";
import { appendExecutionReceiptToLedger } from "../src/receiptLedger";

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
const EXECUTORS = {
  repo_edit: createLiveRepoEditExecutor(),
  local_chat: () => ({ outcome: "ok" as const, detail: "private local chat session attached" }),
  // Launchers — open a tool the user already has, detached. Reach effectors: the gate
  // authorizes the reach grant + intent, then these only spawn the app (no file write, no
  // command run). BB_TERMINAL overrides the terminal binary (default cosmic-term on COSMIC).
  open_vscode: createLiveLauncherExecutor({ command: "code" }),
  open_cursor: createLiveLauncherExecutor({ command: "cursor" }),
  open_terminal: createLiveLauncherExecutor({ command: process.env.BB_TERMINAL ?? "cosmic-term", isTerminal: true }),
};

// Launcher reach effectors that participate in confirm-once-per-session (see sessionConfirmed).
const LAUNCHER_EFFECTORS: ReadonlySet<EffectorId> = new Set<EffectorId>(["open_vscode", "open_cursor", "open_terminal"]);
const localChat = createLiveLocalChatConnector();
// Per-buddy pending effector, set when the gate returns needs_confirmation so a later
// `/confirm` knows what it is confirming. Mirrors the browser composer's pendingEffector.
const pending = new Map<string, string>();
// Per-buddy pending intent, so the `/confirm` round-trip re-authorizes the SAME typed effect
// (same operation + target) it proposed — confirmation must clear the floor on the exact intent,
// never silently widen to a bare grant.
const pendingIntent = new Map<string, ActionIntent>();
const pendingSurface = new Map<string, SurfaceId>();
const activeSurface = new Map<string, SurfaceId>();
// Per-buddy set of launcher effectors the user has already confirmed THIS session. A launcher is
// low-risk reach, so the gate's confirmation floor (work/private posture confirms low-risk actions)
// asks once; after the user confirms, the soul remembers and passes confirmed:true on later launches
// so opening a tool you already approved doesn't nag. This is the soul remembering the USER's prior
// confirmation (a session grant) — law 7 clean: the body never self-authorizes, the gate is unchanged.
// Cleared when the soul restarts, so a fresh session asks once again.
const sessionConfirmed = new Map<string, Set<EffectorId>>();

function isSessionConfirmed(buddy: string, effectorId: string): boolean {
  return sessionConfirmed.get(buddy)?.has(effectorId as EffectorId) ?? false;
}

function rememberSessionConfirm(buddy: string, effectorId: string): void {
  if (!LAUNCHER_EFFECTORS.has(effectorId as EffectorId)) return;
  let set = sessionConfirmed.get(buddy);
  if (!set) {
    set = new Set<EffectorId>();
    sessionConfirmed.set(buddy, set);
  }
  set.add(effectorId as EffectorId);
}
const attachReceiptId = new Map<string, string>();
const localChatHistory = new Map<string, LocalChatMessage[]>();

// A single action-backed turn so an `act` effector (repo_edit) can reach the allow path on this
// real surface: an assistant assertion grades `trusted` and — with `allowAction` on the settings
// below — carries `may_use_for_action`, the trusted backing the gate requires before it will
// authorize a high-risk effect. Without backing the gate fails closed (empty frame), which is
// correct but means repo_edit could only ever block. This is the "action-backed memory turn".
const ACTION_BACKING: SessionChatLine[] = [{ role: "assistant", text: "Reviewed the patch; it is safe to apply." }];

/** Settings for the gate: the profile's defaults plus `allowAction` so a backed turn can carry
 * may_use_for_action. Per-call (not cached) so a future per-buddy posture stays straightforward. */
function gateSettings(buddy: string, effectorId: string) {
  return { ...createDefaultBuddySettings(profileFor(buddy)), allowAction: effectorId === "repo_edit" };
}

/** Build the typed intent for a `/review <effector> <target>` command. Defaults to a repo_path
 * `write_patch` (repo_edit is the only act effector today); the gate's protected-target policy and
 * the executor sandbox still arbitrate where the write may actually land. */
function commandIntent(effectorId: EffectorId, target: string | undefined): ActionIntent | undefined {
  if (!target) return undefined;
  const wire: PresenceActionIntent = { operation: "write_patch", target: { kind: "repo_path", value: target } };
  return presenceIntentToActionIntent(effectorId, wire);
}

/** The workspace a launcher opens. The SOUL owns this path (law 7) — the body never reports it. */
function launcherWorkspace(): string {
  return process.env.BB_WORKSPACE ?? process.cwd();
}

/** The typed intent for a launcher reach effector. The body emits only the effector id; the soul
 * fills in the workspace target here so the gate authorizes the EFFECT (and the executor opens it). */
function launcherIntent(effectorId: EffectorId): ActionIntent {
  const root = launcherWorkspace();
  const wire: PresenceActionIntent = {
    operation: "open",
    target: { kind: "file_path", value: root },
    summary: `open ${root} in ${EFFECTOR_SPECS[effectorId].label}`,
  };
  // file_path target + non-empty value always lifts to a concrete intent.
  return presenceIntentToActionIntent(effectorId, wire)!;
}

/** The hydrate surface list for `buddy`: the canonical surfaces plus a launcher entry per granted
 * launcher reach effector. Launchers ride the same bloom dial (additive `kind:"launcher"`), so the
 * body opens them via action_request instead of switching the active surface. */
function hydrationSurfacesFor(buddy: string): PresenceSurfaceDescriptor[] {
  const entry = manifestEntry(resolveManifestId(buddy));
  const launchers: PresenceSurfaceDescriptor[] = (entry?.effectors ?? [])
    .filter((id) => LAUNCHER_EFFECTORS.has(id))
    .map((id) => ({
      id,
      label: EFFECTOR_SPECS[id].label,
      availability: "gated" as const,
      kind: "launcher" as const,
      effector: id,
    }));
  return [...surfaceHydrationList(), ...launchers];
}

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
  intent?: ActionIntent,
  posture: UserPosture = POSTURE,
  route?: ActionRoute,
) {
  // The soul projects its own state the instant it begins weighing — the deliberation face.
  // Synchronous here (microseconds), but this is the seam a future provider-backed soul that
  // genuinely thinks for seconds lights up automatically: the body shows `thinking`, then the
  // honest outcome face below. Mood belongs to the soul (law 7), so it announces both.
  socket.send(JSON.stringify(presence.express(buddy, "thinking")));
  // Confirm-once-per-session for launchers: a tool the user already approved this session is
  // auto-confirmed so it opens without a second tap. The gate still sees a confirmed action and
  // records it the same way — this only spares the repeat prompt, never widens authorization.
  const effectiveConfirmed = confirmed || isSessionConfirmed(buddy, effectorId);
  const { receipt, result, execution } = handleActionRequest({
    buddy,
    effectorId,
    settings: gateSettings(buddy, effectorId),
    posture,
    history: ACTION_BACKING,
    // The typed effect to authorize. When present (e.g. a repo_edit targeting a sandbox path),
    // the gate authorizes the EFFECT, not just the grant, and on `allow` the live disk-writing
    // executor (sandboxed to .border-agents/proofs/) runs and emits an ExecutionReceipt. Absent
    // → grant-only: an act effector fails closed. No-execute-on-block holds either way.
    intent,
    executors: EXECUTORS,
    route,
    confirmed: effectiveConfirmed,
    requestId,
    storage,
  });
  if (receipt.decision === "needs_confirmation") {
    pending.set(buddy, effectorId);
    if (intent) pendingIntent.set(buddy, intent);
  } else {
    pending.delete(buddy);
    pendingIntent.delete(buddy);
    pendingSurface.delete(buddy);
    // First time a launcher reaches `allow` it was just confirmed by the user; remember it so the
    // rest of the session opens silently. No-op for non-launcher effectors.
    if (receipt.decision === "allow") rememberSessionConfirm(buddy, effectorId);
  }
  // The honest outcome face, derived from the real receipt — sent BEFORE the result so the
  // soul is the authoritative source of mood. The native body's `Emotion::for_decision` is the
  // designed twin/fallback; they agree, so there is no flicker, never a smile that outruns a block.
  socket.send(JSON.stringify(presence.express(buddy, decisionEmotion(receipt.decision))));
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

function providerLabel(provider: string | undefined): string | undefined {
  if (!provider) return undefined;
  return ROUTE_PROVIDER_LABELS[provider as keyof typeof ROUTE_PROVIDER_LABELS] ?? provider;
}

function localChatRoute(): ActionRoute {
  return { provider: "lm_studio", locality: "local", downgraded: false };
}

/** On-device providers run `local`; everything else is `cloud`. Mirrors the local lane in
 * buddyManifest routes (`lm_studio`, `ollama`). */
const LOCAL_PROVIDERS: ReadonlySet<string> = new Set(["lm_studio", "ollama"]);
function routeLocality(provider: string): "local" | "cloud" {
  return LOCAL_PROVIDERS.has(provider) ? "local" : "cloud";
}

function surfaceIntent(surfaceId: SurfaceId): ActionIntent | undefined {
  const surface = getSurface(surfaceId);
  if (!surface?.effectorId) return undefined;
  return {
    effectorId: surface.effectorId,
    operation: "open_session",
    target: { kind: "url", path: providerLabel(surface.provider) ?? surface.id },
    summary: `open ${surface.label}`,
  };
}

function activateSurface(socket: WebSocket, buddy: string, surfaceId: SurfaceId, receipt?: ActionReceipt) {
  const surface = getSurface(surfaceId);
  if (!surface) return;
  activeSurface.set(buddy, surface.id);
  if (receipt) {
    attachReceiptId.set(buddy, receipt.receipt_id);
  }
  if (surface.id !== "private_local_chat") {
    localChatHistory.delete(buddy);
  }
  const provider = surface.provider;
  const availability = surfaceAvailability(surface.id);
  const route = provider
    ? {
        label: providerLabel(provider) ?? provider,
        locality: routeLocality(provider),
        health: routeHealthFromSoul({
          hasRoute: true,
          availability,
          downgraded: false,
        }),
      }
    : undefined;
  socket.send(
    JSON.stringify(
      presence.surfaceActive(buddy, {
        surface: surface.id,
        posture: surface.posture,
        label: surface.label,
        providerLabel: providerLabel(provider),
        ...(route ? { route } : {}),
      }),
    ),
  );
}

function requestSurface(socket: WebSocket, buddy: string, surfaceName: string) {
  const surface = getSurface(surfaceName);
  if (!surface) {
    socket.send(JSON.stringify(presence.express(buddy, "alert")));
    socket.send(JSON.stringify(presence.say(buddy, "That surface is not known yet.")));
    return;
  }

  if (surface.id === "session") {
    attachReceiptId.delete(buddy);
    activateSurface(socket, buddy, surface.id);
    socket.send(JSON.stringify(presence.say(buddy, "Session surface active.")));
    return;
  }

  if (surface.id === "customize" || !surface.effectorId) {
    socket.send(JSON.stringify(presence.express(buddy, "alert")));
    socket.send(JSON.stringify(presence.say(buddy, "Surface customization is not wired yet.")));
    return;
  }

  const route = surface.provider === "lm_studio" ? localChatRoute() : undefined;
  const receipt = authorizeAndReply(
    socket,
    buddy,
    surface.effectorId,
    false,
    undefined,
    surfaceIntent(surface.id),
    surface.posture,
    route,
  );

  if (receipt.decision === "needs_confirmation") {
    pendingSurface.set(buddy, surface.id);
    socket.send(
      JSON.stringify(
        presence.say(
          buddy,
          surface.id === "private_local_chat"
            ? "Enter private local chat? Your messages stay on this machine — /confirm."
            : `Enter ${surface.label}? Type /confirm to continue.`,
        ),
      ),
    );
    return;
  }

  if (receipt.decision === "allow") {
    activateSurface(socket, buddy, surface.id, receipt);
    socket.send(
      JSON.stringify(
        presence.say(
          buddy,
          surface.id === "private_local_chat"
            ? "Private local chat ready — running locally."
            : `${surface.label} ready.`,
        ),
      ),
    );
    return;
  }

  socket.send(JSON.stringify(presence.say(buddy, "That surface isn't wired yet.")));
}

async function routePrivateLocalChat(socket: WebSocket, buddy: string, text: string) {
  const actionReceiptId = attachReceiptId.get(buddy);
  if (!actionReceiptId) {
    socket.send(JSON.stringify(presence.express(buddy, "alert")));
    socket.send(JSON.stringify(presence.say(buddy, "Private local chat is not authorized yet. Enter the surface first.")));
    return;
  }

  socket.send(JSON.stringify(presence.express(buddy, "thinking")));
  const history = localChatHistory.get(buddy) ?? [
    { role: "system", content: "You are a private local model connected through Border Agents. Reply directly and do not claim external tool access." },
  ];
  const messages: LocalChatMessage[] = [...history, { role: "user", content: text }];
  const reply = await localChat(messages);
  const now = new Date().toISOString();
  const manifestId = resolveManifestId(buddy);
  const intent: ActionIntent = {
    effectorId: "local_chat",
    operation: "chat_completion",
    target: { kind: "url", path: "lm_studio" },
    summary: "send a private local chat turn",
  };
  const unreachable = /^Local model unreachable\b/.test(reply);
  const execution = buildExecutionReceipt(
    { intent, buddy: manifestId, route: localChatRoute(), actionReceiptId, now },
    true,
    { outcome: unreachable ? "error" : "ok", detail: unreachable ? reply : "local chat turn completed" },
  );
  appendExecutionReceiptToLedger({ buddyId: manifestId, receipt: execution, storage });
  localChatHistory.set(buddy, [...messages, { role: "assistant", content: reply }].slice(-24));
  socket.send(JSON.stringify(presence.express(buddy, unreachable ? "alert" : "happy")));
  socket.send(JSON.stringify(presence.say(buddy, reply)));
  log("local chat turn", { buddy, receiptId: execution.receipt_id, outcome: execution.outcome });
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

  socket.on("message", async (raw) => {
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
        socket.send(
          JSON.stringify(presence.hydrate(buddy, { emotion: "neutral", surfaces: hydrationSurfacesFor(buddy) })),
        );
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
          if (activeSurface.get(buddy) === "private_local_chat") {
            await routePrivateLocalChat(socket, buddy, message.text);
            return;
          }
          // Free text. This is the GOVERNANCE soul — it gates actions, it does not forward chat
          // to a provider (that is the dev gateway / a provider-backed soul's job). Silently
          // dropping it left the body hung on a "Reply pending — waiting for <buddy>…" promise it
          // could never keep. Answer honestly instead, so the body clears that false pending: a
          // pending state must never outrun what the soul can actually deliver.
          socket.send(
            JSON.stringify(
              presence.say(
                buddy,
                "I'm the governance soul — I gate actions, I don't relay chat. Try /review or /confirm. (Free conversation needs a provider-backed soul.)",
              ),
            ),
          );
          log("said (free text — governance soul declined honestly)", { buddy, text: message.text });
          return;
        }
        if (command.kind === "confirm") {
          const effectorId = pending.get(buddy);
          if (!effectorId) {
            socket.send(JSON.stringify(presence.say(buddy, "Nothing is awaiting confirmation.")));
            return;
          }
          // Re-authorize the SAME typed effect that was proposed — confirmation clears the floor
          // on the exact intent, it does not fall back to a bare grant.
          const surfaceId = pendingSurface.get(buddy);
          const surface = surfaceId ? getSurface(surfaceId) : undefined;
          const receipt = authorizeAndReply(
            socket,
            buddy,
            effectorId,
            true,
            undefined,
            pendingIntent.get(buddy),
            surface?.posture ?? POSTURE,
            surface?.provider === "lm_studio" ? localChatRoute() : undefined,
          );
          if (receipt.decision === "allow" && surface) {
            activateSurface(socket, buddy, surface.id, receipt);
            socket.send(
              JSON.stringify(
                presence.say(
                  buddy,
                  surface.id === "private_local_chat"
                    ? "Private local chat ready — running locally."
                    : `${surface.label} ready.`,
                ),
              ),
            );
          }
          return;
        }
        // `/review <effector> <target>` carries a typed effect (e.g. `/review repo_edit
        // .border-agents/proofs/notes.md`); bare `/review` is grant-only. The soul builds the
        // intent here — the body only reported the text (law 7).
        const intent = commandIntent(command.effectorId as EffectorId, command.target);
        authorizeAndReply(socket, buddy, command.effectorId, false, undefined, intent);
        return;
      }

      case "action_request": {
        // A body (or future surface) emitted the typed action cue directly. Lift the wire intent
        // into a core ActionIntent; on a confirm follow-up that omits it, fall back to the pending
        // intent so the executor still runs against the originally-proposed effect.
        const lifted = presenceIntentToActionIntent(message.effector as EffectorId, message.intent);
        let intent = lifted ?? (message.confirmed ? pendingIntent.get(buddy) : undefined);
        // Launchers arrive as a bare action_request (the body names only the effector). The soul
        // owns the workspace target, so synthesise the typed intent here — without it the executor
        // would not run on `allow` (grant-only). Same target on the confirm round-trip, so confirm
        // re-authorizes the exact effect.
        if (!intent && LAUNCHER_EFFECTORS.has(message.effector as EffectorId)) {
          intent = launcherIntent(message.effector as EffectorId);
        }
        authorizeAndReply(socket, buddy, message.effector, message.confirmed === true, message.requestId, intent);
        return;
      }

      case "surface_request": {
        requestSurface(socket, buddy, message.surface);
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
