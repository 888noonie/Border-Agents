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
import { BUDDY_MANIFEST, EFFECTOR_SPECS, resolveManifestId, type EffectorId } from "./buddyManifest";
import {
  authorizeEffectorAction,
  emptyFrame,
  type ActionReceipt,
  type SafeContextFrame,
  type UserPosture,
} from "./core";
import { buildBuddyGovernanceSnapshot, selectPurpose, type SessionChatLine } from "./liveGovernance";
import { presence, type PresenceActionResult } from "./presenceProtocol";
import { appendActionReceiptToLedger } from "./receiptLedger";

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
  | { kind: "review"; effectorId: string }
  | { kind: "confirm" };

export function parseActionCommand(text: string): ActionCommand | null {
  const trimmed = text.trim();
  if (trimmed === "/confirm") {
    return { kind: "confirm" };
  }
  if (trimmed === "/review" || trimmed.startsWith("/review ")) {
    const effectorId = trimmed.slice("/review".length).trim() || "receipt_review";
    return { kind: "review", effectorId };
  }
  return null;
}

function summarize(receipt: ActionReceipt, label: string): string {
  switch (receipt.decision) {
    case "allow":
      return `Running "${label}".`;
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
  confirmed?: boolean;
  requestId?: string;
  storage?: Storage;
  now?: string;
}): { receipt: ActionReceipt; result: PresenceActionResult } {
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

  const receipt = authorizeEffectorAction({
    buddy: manifestId,
    effector: spec,
    granted,
    posture: args.posture,
    purpose,
    frame,
    confirmed: args.confirmed,
    now: derivedAt,
  });

  return finish(args, receipt, spec.label, manifestId);
}

function finish(
  args: { buddy: string; storage?: Storage; requestId?: string },
  receipt: ActionReceipt,
  label: string,
  ledgerBuddyId: string,
): { receipt: ActionReceipt; result: PresenceActionResult } {
  appendActionReceiptToLedger({ buddyId: ledgerBuddyId, receipt, storage: args.storage });

  const result = presence.actionResult(args.buddy, {
    effector: receipt.effector,
    decision: receipt.decision,
    receiptId: receipt.receipt_id,
    requestId: args.requestId,
    summary: summarize(receipt, label),
  });

  return { receipt, result };
}
