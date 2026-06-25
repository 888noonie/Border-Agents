// Soul-side action handling — the seam between a presence `action_request` cue and
// the governance action gate. Pure and socket-free so it unit-tests with an in-memory
// Storage; the dev gateway and the browser body both drive it the same way.
//
// Flow: resolve the effector + grant → select the purpose → build the live memory frame
// → run the deterministic gate (src/core/actionGate.ts) → append the ActionReceipt to the
// shared ledger → hand back the receipt and the `action_result` cue to send to the body.
//
// AGENTS.md law 7: authorization happens HERE (the soul), never in the body. The body only
// emits the request and renders the result cue.

import type { BuddySettings } from "./buddyProfiles";
import {
  BUDDY_MANIFEST,
  EFFECTOR_SPECS,
  resolveManifestId,
  type EffectorId,
  type RouteProvider,
} from "./buddyManifest";
import {
  authorizeEffectorAction,
  emptyFrame,
  type ActionIntent,
  type ActionReceipt,
  type ActionRoute,
  type ActionTarget,
  type ExecutionReceipt,
  type SafeContextFrame,
  type UserPosture,
} from "./core";
import {
  buildExecutionReceipt,
  DEFAULT_EXECUTORS,
  type ExecutorRegistry,
} from "./effectorExecutors";
import { buildBuddyGovernanceSnapshot, selectPurpose, type SessionChatLine } from "./liveGovernance";
import {
  presence,
  type PresenceActionIntent,
  type PresenceActionResult,
  type PresenceAlertLevel,
  type PresenceEmotion,
} from "./presenceProtocol";
import { appendActionReceiptToLedger, appendExecutionReceiptToLedger } from "./receiptLedger";

const LOCAL_PROVIDERS = new Set<RouteProvider>(["lm_studio", "ollama"]);

/** The buddy's default (preferred, non-downgraded) route, used when the caller supplies none. */
function defaultRouteFor(manifestId: string): ActionRoute {
  const provider = BUDDY_MANIFEST[manifestId]?.routes.primary[0] ?? "custom";
  return { provider, locality: LOCAL_PROVIDERS.has(provider) ? "local" : "cloud", downgraded: false };
}

function isKnownEffector(id: string): id is EffectorId {
  return Object.prototype.hasOwnProperty.call(EFFECTOR_SPECS, id);
}

/**
 * Parse the on-body / composer slash commands that drive the action gate, so every body
 * (browser composer, native on-body input, future surfaces) interprets them identically.
 * `/review [effector]` requests an effector (default the read-only `receipt_review`);
 * `/confirm` confirms whatever needs_confirmation action is pending for that buddy. Returns
 * null for anything that is not an action command (free text → the provider, not the gate).
 */
export type ActionCommand =
  | { kind: "review"; effectorId: string; target?: string }
  | { kind: "confirm" };

export function parseActionCommand(text: string): ActionCommand | null {
  const trimmed = text.trim();
  if (trimmed === "/confirm") {
    return { kind: "confirm" };
  }
  if (trimmed === "/review" || trimmed.startsWith("/review ")) {
    // `/review` → receipt_review; `/review <effector>` → that effector;
    // `/review <effector> <target...>` → that effector against a typed target.
    const rest = trimmed.slice("/review".length).trim();
    if (rest === "") {
      return { kind: "review", effectorId: "receipt_review" };
    }
    const [effectorId, ...targetParts] = rest.split(/\s+/);
    const target = targetParts.join(" ").trim();
    return target ? { kind: "review", effectorId, target } : { kind: "review", effectorId };
  }
  return null;
}

/**
 * Lift a wire `PresenceActionIntent` (the typed cue a body fills on an `action_request`) into
 * the core `ActionIntent` the gate authorizes. This is the soul half of the wire membrane: only
 * a typed intent with a CONCRETE target may authorize an `act` effector, so an intent whose
 * target is `none` (or absent, or value-less) yields `undefined` — the request degrades to
 * grant-only and any act effector fails closed. The body fills these fields; the soul (here)
 * validates and lifts them — never the body (AGENTS.md law 7). `summary` is synthesized from the
 * operation + target when the body omitted it, so every authorized effect still pins a label.
 */
export function presenceIntentToActionIntent(
  effectorId: EffectorId,
  wire: PresenceActionIntent | undefined,
): ActionIntent | undefined {
  if (!wire) return undefined;
  const kind = wire.target?.kind;
  const value = wire.target?.value;
  // No concrete target → nothing to authorize an effect against. Grant-only request.
  if (!kind || kind === "none" || typeof value !== "string" || value.length === 0) {
    return undefined;
  }
  const target: ActionTarget = { kind, path: value };
  return {
    effectorId,
    operation: wire.operation,
    target,
    ...(wire.payloadDigest ? { payloadDigest: wire.payloadDigest } : {}),
    summary: wire.summary && wire.summary.length > 0 ? wire.summary : `${wire.operation} ${value}`,
  };
}

/**
 * The honest governance face for a gate decision — the soul's authoritative mood for an
 * action outcome (AGENTS.md law 7: mood belongs to the soul, expressed as an `express` cue).
 * Twin of the native body's `Emotion::for_decision` in desktop-body/src/render.rs: `allow`
 * smiles, `needs_confirmation` asks (the questioning curious mouth), and `blocked` — or ANY
 * unknown decision — fails loud as `alert`. Affect must never outrun the outcome: a face that
 * smiled while the gate overrode something would spend instant-trust dishonestly.
 */
export function decisionEmotion(decision: string): PresenceEmotion {
  switch (decision) {
    case "allow":
      return "happy";
    case "needs_confirmation":
      return "curious";
    default:
      return "alert";
  }
}

/**
 * Chrome twin of `decisionEmotion`: maps a gate decision to the passport/ring alert tier the
 * soul sends on `action_result`. Face and chrome derive from the same decision so the body
 * never infers policy state from a facial-expression string (law 7). Garbage fails loud at
 * `critical`, the same "never a reassuring face on bad input" stance as `decisionEmotion`.
 */
export function decisionAlertLevel(decision: string): PresenceAlertLevel {
  switch (decision) {
    case "allow":
      return "ready";
    case "needs_confirmation":
      return "confirm";
    case "blocked":
      return "blocked";
    default:
      return "critical";
  }
}

function summarize(receipt: ActionReceipt, label: string, execution?: ExecutionReceipt): string {
  switch (receipt.decision) {
    case "allow": {
      if (execution && execution.executor_called && execution.outcome === "ok") {
        return `Ran "${label}" via ${execution.route.provider}.`;
      }
      if (execution && execution.outcome === "skipped") {
        return `"${label}" authorized; ${execution.detail ?? "not executed on this surface"}.`;
      }
      return `Running "${label}".`;
    }
    case "needs_confirmation":
      return `"${label}" needs your confirmation before it runs.`;
    case "blocked":
      return `"${label}" is blocked: ${receipt.rules[receipt.rules.length - 1]?.reason ?? "not authorized"}.`;
  }
}

/**
 * Authorize one effector invocation requested by a body and produce both the durable
 * ActionReceipt (appended to the ledger) and the thin `action_result` cue to send back.
 * Never throws on untrusted wire input — an unknown effector id yields a blocked receipt.
 */
export function handleActionRequest(args: {
  buddy: string;
  effectorId: string;
  settings: BuddySettings;
  posture: UserPosture;
  history: SessionChatLine[];
  /** The typed operation + target. When present, the gate authorizes the EFFECT, not just the grant. */
  intent?: ActionIntent;
  /** The provider route the buddy is on. A downgraded route forces confirmation. Defaults to the buddy's preferred route. */
  route?: ActionRoute;
  /** Executors keyed by effector. On `allow`, the matching executor runs and emits an ExecutionReceipt. Browser surfaces pass none. */
  executors?: ExecutorRegistry;
  confirmed?: boolean;
  requestId?: string;
  storage?: Storage;
  now?: string;
}): { receipt: ActionReceipt; result: PresenceActionResult; execution?: ExecutionReceipt } {
  const derivedAt = args.now ?? new Date().toISOString();

  // The body speaks in persona ids (e.g. "owl"); the gate authorizes under the governance
  // id (e.g. "veritas"). Resolve once: grant lookup, the receipt, and the ledger key all use
  // the governance identity (the audit subject), while the result cue is addressed back to
  // the requesting persona so the body recognizes its own outcome.
  const manifestId = resolveManifestId(args.buddy);

  // Unknown effector: synthesize a blocked receipt rather than trusting the wire.
  if (!isKnownEffector(args.effectorId)) {
    const receipt: ActionReceipt = {
      receipt_id: `action:${manifestId}:${args.effectorId}:${derivedAt}`,
      effector: args.effectorId as EffectorId,
      buddy: manifestId,
      decision: "blocked",
      risk: "high",
      posture: args.posture,
      confirmed: args.confirmed === true,
      derived_at: derivedAt,
      rules: [
        {
          field: "effector",
          value: args.effectorId,
          source: "action_request",
          reason: "requested effector is not a known capability",
          policy_rule: "action.blocked.unknown_effector",
        },
      ],
    };
    return finish(args, receipt, args.effectorId, manifestId);
  }

  const spec = EFFECTOR_SPECS[args.effectorId];
  const granted = BUDDY_MANIFEST[manifestId]?.effectors.includes(args.effectorId) ?? false;
  const purpose = selectPurpose(args.settings);

  const snapshot = buildBuddyGovernanceSnapshot({
    buddyId: manifestId,
    history: args.history,
    settings: args.settings,
    now: derivedAt,
  });
  // No memory backing (memory off or nothing retrieved) → empty frame so the gate
  // fails closed: a high-risk / act action with no trusted backing cannot be authorized.
  const frame: SafeContextFrame = snapshot?.frame ?? emptyFrame(purpose);

  const route = args.route ?? defaultRouteFor(manifestId);

  const receipt = authorizeEffectorAction({
    buddy: manifestId,
    effector: spec,
    granted,
    posture: args.posture,
    purpose,
    frame,
    intent: args.intent,
    route,
    confirmed: args.confirmed,
    now: derivedAt,
  });

  // No-execute-on-block: the executor is reachable ONLY on `allow`, and only when this
  // surface wired one. On a `blocked`/`needs_confirmation` receipt no ExecutionReceipt
  // is ever produced — nothing crossed into the world.
  let execution: ExecutionReceipt | undefined;
  if (receipt.decision === "allow" && args.intent) {
    const executor = (args.executors ?? DEFAULT_EXECUTORS)[args.intent.effectorId];
    const ctx = { intent: args.intent, buddy: manifestId, route, actionReceiptId: receipt.receipt_id, now: derivedAt };
    execution = executor
      ? buildExecutionReceipt(ctx, true, executor(ctx))
      : buildExecutionReceipt(ctx, false, { outcome: "skipped", detail: "no executor wired on this surface" });
  }

  return finish(args, receipt, spec.label, manifestId, execution);
}

function finish(
  args: { buddy: string; storage?: Storage; requestId?: string },
  receipt: ActionReceipt,
  label: string,
  ledgerBuddyId: string,
  execution?: ExecutionReceipt,
): { receipt: ActionReceipt; result: PresenceActionResult; execution?: ExecutionReceipt } {
  // Order matters: the authorization receipt lands before the execution receipt, so the
  // ledger reads "authorized X, then executed X" — different borders, in sequence.
  appendActionReceiptToLedger({ buddyId: ledgerBuddyId, receipt, storage: args.storage });
  if (execution) {
    appendExecutionReceiptToLedger({ buddyId: ledgerBuddyId, receipt: execution, storage: args.storage });
  }

  // Structured execution outcome on the wire (not just the human summary): `executed` is
  // the load-bearing bit, and the route is the provider provenance. Present only when an
  // executor was consulted (i.e. on an `allow`). The full ExecutionReceipt stays soul-side.
  const outcome = execution
    ? {
        executed: execution.executor_called && execution.outcome === "ok",
        executionReceiptId: execution.receipt_id,
        route: {
          provider: execution.route.provider,
          locality: execution.route.locality,
          downgraded: execution.route.downgraded,
          ...(execution.route.fallbackOf ? { fallbackOf: execution.route.fallbackOf } : {}),
        },
      }
    : undefined;

  const result = presence.actionResult(args.buddy, {
    effector: receipt.effector,
    decision: receipt.decision,
    receiptId: receipt.receipt_id,
    requestId: args.requestId,
    summary: summarize(receipt, label, execution),
    alertLevel: decisionAlertLevel(receipt.decision),
    ...(outcome ? { outcome } : {}),
  });

  return { receipt, result, execution };
}
