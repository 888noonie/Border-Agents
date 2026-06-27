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
  type PresenceClicked,
  type PresenceSurfaceDescriptor,
} from "../src/presenceProtocol";
import { createDefaultBuddySettings, BUDDY_PROFILES, type BuddyProfile } from "../src/buddyProfiles";
import {
  INITIAL_ONBOARDING_STATE,
  currentAct,
  entryMode,
  type OnboardingCue,
  type OnboardingEvent,
  type OnboardingState,
} from "../src/wizardOnboarding";
import { actCues, actPanel, onHostEvent } from "../src/wizardOnboardingHost";
import {
  applyPanelChoices,
  createWizardHostDraft,
  receiptDetailForOnboardingAct,
  type WizardHostDraft,
} from "../src/wizardHostDraft";
import {
  lifecycleReceiptKinds,
  readLifecycleReceipts,
  recordLifecycleReceipt,
} from "../src/lifecycleReceipts";
import { EFFECTOR_SPECS, LAUNCHER_REACH_EFFECTORS, manifestEntry, ROUTE_PROVIDER_LABELS, resolveManifestId, type EffectorId } from "../src/buddyManifest";
import { getSurface, surfaceAvailability, surfaceHydrationList, type SurfaceId } from "../src/surfaceManifest";
import type { ActionIntent, ActionReceipt, ActionRoute, UserPosture } from "../src/core";
import type { SessionChatLine } from "../src/liveGovernance";
import { appendExecutionReceiptToLedger } from "../src/receiptLedger";

const PORT = Number(process.env.BB_PRESENCE_PORT ?? 17387);
const PATH = process.env.BB_PRESENCE_PATH ?? "/border-buddies";
const POSTURE = normalizePosture(process.env.BB_POSTURE);
const LEDGER_PATH = process.env.BB_SOUL_LEDGER ?? join(tmpdir(), "bb-soul-ledger.json");
// Which persona this soul runs. "echo" (default) is the governance gate; "wizard" runs the
// scripted onboarding Host (Acts 0–5) instead. One soul per port — the Host owns the socket.
const IS_WIZARD = process.env.BB_SOUL?.trim() === "wizard";

function normalizePosture(value: string | undefined): UserPosture {
  return value === "play" || value === "private" ? value : "work";
}

function sessionPosture(): UserPosture {
  return IS_WIZARD ? wizardSessionPosture : POSTURE;
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
  // CLI coding agents — spawned bare at the workspace (no path arg). BB_CLAUDE_CODE / BB_AGENT_ZERO
  // override the command. `claude` may be absent on this machine; the executor then returns a clear
  // "install the command" receipt rather than failing silently.
  open_claude_code: createLiveLauncherExecutor({ command: process.env.BB_CLAUDE_CODE ?? "claude", bareCommand: true }),
  open_agent_zero: createLiveLauncherExecutor({ command: process.env.BB_AGENT_ZERO ?? "a0", bareCommand: true }),
  // Commandeer — an `act` effector whose world-effect (raise/type a native window) happens
  // OUT of this process, in the frame driver. The gate authorizes it here; on `allow` the soul
  // dispatches a `commandeer` cue to the driver (see dispatchCommandeer). The executor only
  // records that the cue was sent — the driver is fire-and-forget (a future driver→soul ack
  // would tighten this into a real success/failure outcome).
  commandeer: () => ({ outcome: "ok" as const, detail: "commandeer cue dispatched to the frame driver" }),
};

// Control mode types this benign marker (NO trailing newline, so nothing executes) as the
// proof-of-drive. The soul owns what is typed (law 7); free-text control is a later increment.
const COMMANDEER_CONTROL_TEXT = "border-agents control ok";

// Launcher reach effectors that participate in confirm-once-per-session (see sessionConfirmed).
// Single source: the manifest's LAUNCHER_REACH_EFFECTORS.
const LAUNCHER_EFFECTORS: ReadonlySet<EffectorId> = LAUNCHER_REACH_EFFECTORS;
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
  // `act` effectors need trusted action-backing in the frame before the gate will allow them.
  // repo_edit and commandeer are the wired act effectors today.
  const allowAction = effectorId === "repo_edit" || effectorId === "commandeer";
  return { ...createDefaultBuddySettings(profileFor(buddy)), allowAction };
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
  const canonical = surfaceHydrationList();
  // A launcher reach effector already exposed as a canonical surface (e.g. claude_code →
  // open_claude_code) must not be appended a second time as a free-standing launcher entry.
  const canonicalLauncherEffectors = new Set(canonical.map((s) => s.effector).filter(Boolean));
  const launchers: PresenceSurfaceDescriptor[] = (entry?.effectors ?? [])
    .filter((id) => LAUNCHER_EFFECTORS.has(id) && !canonicalLauncherEffectors.has(id))
    .map((id) => ({
      id,
      label: EFFECTOR_SPECS[id].label,
      availability: "gated" as const,
      kind: "launcher" as const,
      effector: id,
    }));
  return [...canonical, ...launchers];
}

// --- Wizard onboarding Host -----------------------------------------------------
//
// Under BB_SOUL=wizard this soul runs the scripted Host (src/wizardOnboardingHost.ts)
// instead of the governance gate: it greets the body at Act 0, advances the flow on the
// raw events the body/panel report, records a durable lifecycle receipt for each act that
// earns one, and hands off to the companion (hermes) when onboarding completes. Law 7
// holds — the Host (soul) decides the acts and receipts; the body only presents cues.
//
// The opaque settings-panel WINDOW is deferred (its own spike): until it ships, the form
// acts advance on injected `panel:*` clicks (an additive optional `clicked.panel` field).

const HOST_BUDDY = "host";
const HERMES_BUDDY = "hermes";
// find_me (Act 4) has no form; it self-advances on a timeout the Host fires. Mirrors the
// panel's IDLE_AUTO_ADVANCE_MS (src/onboardingPanelModel.ts) so both halves linger alike.
const WIZARD_TIMEOUT_MS = 6000;
// A state parked on the final act — what a re-entering (already-onboarded) body seeds with,
// so the linear script never replays in hub mode.
const COMPLETED_ONBOARDING_STATE: OnboardingState = { actIndex: 5, completed: true };

const onboarding = new Map<string, OnboardingState>();
const wizardDrafts = new Map<string, WizardHostDraft>();
const wizardTimers = new Map<string, ReturnType<typeof setTimeout>>();
/** Posture chosen during wizard onboarding; applied to gate calls after handoff. */
let wizardSessionPosture: UserPosture = POSTURE;

/** Translate one onboarding cue into a presence envelope and send it to the body. */
function emitCues(socket: WebSocket, buddy: string, cues: readonly OnboardingCue[]): void {
  for (const cue of cues) {
    switch (cue.kind) {
      case "move_to":
        socket.send(JSON.stringify(presence.moveTo(buddy, cue.position)));
        break;
      case "express":
        socket.send(
          JSON.stringify(
            presence.express(buddy, cue.emotion, cue.intensity !== undefined ? { intensity: cue.intensity } : {}),
          ),
        );
        break;
      case "say":
        socket.send(JSON.stringify(presence.say(buddy, cue.text)));
        break;
      case "attention":
        socket.send(JSON.stringify(presence.attention(buddy, cue.focus)));
        break;
    }
  }
}

/**
 * Push the current act's onboarding form section to the body as a `panel` cue (Build C). Always
 * emitted alongside the act's cues so a no-form act (`section: "none"`) closes any open panel —
 * the body and the script never drift about what's shown. Summary row statuses read the live
 * lifecycle ledger so the receipts the body shows match what's actually recorded.
 */
function emitPanel(socket: WebSocket, buddy: string, state: OnboardingState): void {
  const receiptKinds = lifecycleReceiptKinds(readLifecycleReceipts(storage));
  const draft = wizardDrafts.get(buddy) ?? createWizardHostDraft();
  socket.send(JSON.stringify(presence.panel(buddy, actPanel(state, receiptKinds, draft))));
}

function clearWizardTimer(buddy: string): void {
  const timer = wizardTimers.get(buddy);
  if (timer) {
    clearTimeout(timer);
    wizardTimers.delete(buddy);
  }
}

/** Arm the self-advance timer if the act we just entered advances on a timeout (find_me). */
function scheduleWizardTimeout(socket: WebSocket, buddy: string, state: OnboardingState): void {
  if (!currentAct(state).advanceOn.includes("timeout")) return;
  const timer = setTimeout(() => {
    wizardTimers.delete(buddy);
    advanceWizard(socket, buddy, "timeout");
  }, WIZARD_TIMEOUT_MS);
  wizardTimers.set(buddy, timer);
}

/** Onboarding complete: park the Host and bring in the companion (hydrate hermes). The true
 * window-dismiss / hermes-body spawn lands with the deferred panel spike — here we park the
 * Host tucked and hydrate the hermes buddy so a connected hermes body settles in. */
function handoffToHermes(socket: WebSocket, hostBuddy: string): void {
  log("wizard complete → handoff to hermes", { host: hostBuddy });
  // The summary panel was open when `done` fired; close it before parking the Host tucked.
  socket.send(JSON.stringify(presence.panel(hostBuddy, { section: "none", title: "" })));
  socket.send(
    JSON.stringify(presence.moveTo(hostBuddy, { mode: "tucked", edge: "right", offset: { x: 0, y: 48 } })),
  );
  socket.send(
    JSON.stringify(
      presence.hydrate(HERMES_BUDDY, {
        emotion: "happy",
        speech: "Hi — I'm Hermes, your companion. What shall we do first?",
        surfaces: hydrationSurfacesFor(HERMES_BUDDY),
      }),
    ),
  );
}

/** Fold one to-soul event into the onboarding flow, presenting the next act's cues and
 * recording any earned lifecycle receipt. A non-advancing event is a silent no-op. */
function advanceWizard(socket: WebSocket, buddy: string, event: OnboardingEvent): void {
  const state = onboarding.get(buddy) ?? INITIAL_ONBOARDING_STATE;
  const act = currentAct(state);
  const draft = wizardDrafts.get(buddy) ?? createWizardHostDraft();
  const result = onHostEvent(state, event);
  if (result.next === state) return; // event didn't satisfy the current act — nothing to do
  clearWizardTimer(buddy);
  if (result.receipt) {
    const detail = receiptDetailForOnboardingAct(act, draft);
    recordLifecycleReceipt({ kind: result.receipt, detail, storage });
    log("wizard receipt recorded", { buddy, kind: result.receipt, detail });
  }
  onboarding.set(buddy, result.next);
  if (result.completedNow) {
    handoffToHermes(socket, buddy);
    return;
  }
  emitCues(socket, buddy, actCues(result.next));
  emitPanel(socket, buddy, result.next);
  scheduleWizardTimeout(socket, buddy, result.next);
}

/** Top-level Host handler under BB_SOUL=wizard. Seeds per-buddy state on attach (linear on
 * first run, parked-complete on re-entry), greets at Act 0, and routes the advancing events
 * (`clicked` — optionally carrying a `panel:*` discriminator — and `dropped`) into the flow. */
function handleWizardHost(socket: WebSocket, message: PresenceClicked | { kind: string; buddy: string }): void {
  const buddy = message.buddy || HOST_BUDDY;
  switch (message.kind) {
    case "attached": {
      const mode = entryMode(lifecycleReceiptKinds(readLifecycleReceipts(storage)));
      const state = mode === "linear" ? INITIAL_ONBOARDING_STATE : COMPLETED_ONBOARDING_STATE;
      onboarding.set(buddy, state);
      wizardDrafts.set(buddy, createWizardHostDraft());
      wizardSessionPosture = POSTURE;
      clearWizardTimer(buddy);
      socket.send(JSON.stringify(presence.hydrate(buddy, { emotion: "neutral" })));
      if (mode === "linear") {
        emitCues(socket, buddy, actCues(state));
        emitPanel(socket, buddy, state);
        scheduleWizardTimeout(socket, buddy, state);
      } else {
        socket.send(JSON.stringify(presence.express(buddy, "happy")));
        socket.send(
          JSON.stringify(presence.say(buddy, "You're already set up — tug me out whenever you need me.")),
        );
      }
      log("wizard attached", { buddy, mode });
      return;
    }
    case "clicked": {
      const clicked = message as PresenceClicked;
      if (clicked.panel) {
        const draft = wizardDrafts.get(buddy) ?? createWizardHostDraft();
        const nextDraft = applyPanelChoices(draft, clicked.panel, clicked.panelChoices);
        wizardDrafts.set(buddy, nextDraft);
        if (clicked.panel === "posture_set") {
          wizardSessionPosture = nextDraft.posture;
        }
        advanceWizard(socket, buddy, `panel:${clicked.panel}` as OnboardingEvent);
      } else {
        advanceWizard(socket, buddy, "clicked");
      }
      return;
    }
    case "dropped": {
      advanceWizard(socket, buddy, "dropped");
      return;
    }
    case "said": {
      // Onboarding is pre-connection — the Host doesn't route chat to a provider yet.
      socket.send(JSON.stringify(presence.express(buddy, "happy")));
      socket.send(JSON.stringify(presence.say(buddy, "Let's finish getting you set up first.")));
      return;
    }
    default:
      return;
  }
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
  posture: UserPosture = sessionPosture(),
  route?: ActionRoute,
) {
  // The soul projects its own state the instant it begins weighing — the deliberation face.
  // Synchronous here (microseconds), but this is the seam a future provider-backed soul that
  // genuinely thinks for seconds lights up automatically: the body shows `thinking`, then the
  // honest outcome face below. Mood belongs to the soul (law 7), so it announces both.
  socket.send(JSON.stringify(presence.express(buddy, "thinking")));
  // Launchers self-confirm: tapping a specific launcher (Claude Code, Agent Zero, VS Code, …)
  // IS the explicit user action — it only opens a tool the user already has and applies no
  // effect, so a second "/confirm" tap is redundant friction for the safest category. The gate
  // still authorizes the grant + intent, runs every hard block, and records a confirmed receipt
  // the same way; this only sets the confirmation CADENCE for launchers to zero (interaction
  // layer, never a widening of authorization). `act`/high-risk effectors are untouched and still
  // confirm. `isSessionConfirmed` remains for any other reach effector confirmed earlier.
  const isLauncher = LAUNCHER_EFFECTORS.has(effectorId as EffectorId);
  const effectiveConfirmed = confirmed || isLauncher || isSessionConfirmed(buddy, effectorId);
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
  // On an authorized commandeer, the world-effect lives in the frame driver: dispatch the
  // gated cue to it now. This is the only place a commandeer reaches the driver, so it covers
  // both the direct action_request allow and the /confirm round-trip (both pass through here).
  if (effectorId === "commandeer" && receipt.decision === "allow" && intent) {
    dispatchCommandeer(socket, buddy, intent);
  }
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

/**
 * Send the gated `commandeer` cue to the frame driver. The gate has already authorized this
 * effect (allow). The driver is another client, so we relay to everyone-but-the-requesting-body;
 * the driver picks it up and carries out the mechanism (activate / type). The intent carries the
 * window in `target.path` and the mode in `operation` (pin | monitor | control). AGENTS.md law 7:
 * the soul decides + dispatches; the driver is only the mechanism, with no OS consent of its own.
 */
function dispatchCommandeer(socket: WebSocket, buddy: string, intent: ActionIntent) {
  const targetId = intent.target.path;
  const mode = intent.operation;
  const cue: Record<string, unknown> = {
    protocol: PRESENCE_PROTOCOL,
    v: 0,
    kind: "commandeer",
    buddy,
    ts: Date.now(),
    targetId,
    mode,
  };
  if (mode === "control") {
    cue.text = COMMANDEER_CONTROL_TEXT;
  }
  relayPresenceCue(socket, cue);
  log("commandeer dispatched to driver", { buddy, targetId, mode });
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
// `targets_available` is the enumerated window list the body's pin-picker renders.
const TO_BODY_RELAY_KINDS = new Set([
  "target_acquired",
  "target_moved",
  "target_lost",
  "targets_available",
]);

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

    // Wizard persona owns the whole socket: it runs the onboarding script, not the gate.
    if (IS_WIZARD) {
      handleWizardHost(socket, message);
      return;
    }

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
