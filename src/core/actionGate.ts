// Action gate — the action-side mirror of the memory grader.
//
// `gradeMemories` decides whether a *memory* may enter a prompt and emits a
// `GradeReceipt` per chunk. `authorizeEffectorAction` decides whether a *buddy
// action* (an effector invocation) may run and emits a single `ActionReceipt`.
// Both share the same `DerivationStep` trail so the ledger and Trust Workbench
// render them identically.
//
// Trust-critical laws baked in here (AGENTS.md):
//   - Deterministic: same inputs + `now` → identical receipt. No I/O, no clock
//     unless `now` is omitted (the caller passes it for reproducibility).
//   - Fail-closed: every unhandled path lands on a tighter decision, never a
//     looser one.
//   - Confirmation tightens, never widens: `confirmed: true` can only turn a
//     `needs_confirmation` into `allow`. It can NEVER turn a `blocked` into
//     `allow` — the three hard blocks below run first and ignore confirmation.
//
// This module is the SINGLE source of authorization truth. The dev gateway
// (scripts/gateway-dev.mjs) is a disposable stand-in and must never reimplement
// this logic — it only relays the request and renders the result.

import type { EffectorId, EffectorKind, EffectorSpec } from "../buddyManifest";
import { getPurposePolicy, type BuiltInPurpose } from "./policies";
import type { DerivationStep, PurposePolicy, SafeContextFrame } from "./types";
import { requiresConfirmation, resolvePosturePolicy, type UserPosture } from "./userPosture";

export type ActionDecision = "allow" | "needs_confirmation" | "blocked";

export interface ActionReceipt {
  receipt_id: string; // `action:{buddy}:{effector}:{derived_at}`
  effector: EffectorId;
  buddy: string;
  decision: ActionDecision;
  /** Effective risk the gate evaluated (act effectors are floored to `medium`). */
  risk: PurposePolicy["risk"];
  posture: UserPosture;
  /** True when this receipt is the post-confirmation pass for the same action. */
  confirmed: boolean;
  derived_at: string;
  /** Same DerivationStep shape as GradeReceipt; includes posture clamps. */
  rules: DerivationStep[];
}

const RISK_RANK: Record<PurposePolicy["risk"], number> = { low: 0, medium: 1, high: 2 };

// `act` effectors perform an action in place of the real tool, so they always
// carry at least medium risk — enough to hit the confirmation floor — even when
// the selected purpose is low-risk. `reach` effectors inherit the purpose risk.
function effectiveRisk(kind: EffectorKind, purposeRisk: PurposePolicy["risk"]): PurposePolicy["risk"] {
  if (kind === "act") {
    return RISK_RANK[purposeRisk] >= RISK_RANK.medium ? purposeRisk : "medium";
  }
  return purposeRisk;
}

function rule(field: string, value: unknown, source: string, reason: string, policyRule: string): DerivationStep {
  return { field, value, source, reason, policy_rule: policyRule };
}

/**
 * Authorize a single effector invocation. Pure and deterministic.
 *
 * The caller resolves `granted` from the buddy manifest and `purpose` from the
 * buddy settings (`selectPurpose`), and passes the live `SafeContextFrame` so
 * the gate can require trusted memory backing for `act`/high-risk actions
 * without re-grading anything.
 */
export function authorizeEffectorAction(args: {
  buddy: string;
  effector: EffectorSpec;
  granted: boolean;
  posture: UserPosture;
  purpose: BuiltInPurpose;
  frame: SafeContextFrame;
  confirmed?: boolean;
  now?: string;
}): ActionReceipt {
  const derivedAt = args.now ?? new Date().toISOString();
  const confirmed = args.confirmed === true;
  const { effector } = args;

  // Posture-tightening is part of the audit trail — seed `rules` with it, the
  // same way grading records what the posture clamped.
  const resolution = resolvePosturePolicy(args.posture, args.purpose);
  const rules: DerivationStep[] = [...resolution.clamps];
  const policy = resolution.policy;

  const receipt = (decision: ActionDecision, risk: PurposePolicy["risk"]): ActionReceipt => ({
    receipt_id: `action:${args.buddy}:${effector.id}:${derivedAt}`,
    effector: effector.id,
    buddy: args.buddy,
    decision,
    risk,
    posture: args.posture,
    confirmed,
    derived_at: derivedAt,
    rules,
  });

  // --- Hard blocks. These run first and IGNORE `confirmed` — confirmation can
  // never widen authorization, only waive the confirmation floor below. ---

  if (!args.granted) {
    rules.push(rule("granted", false, "buddy_manifest", "effector is not granted to this buddy", "action.blocked.ungranted"));
    return receipt("blocked", policy.risk);
  }

  if (!effector.wired) {
    rules.push(rule("wired", false, `effector:${effector.id}`, "effector is not wired live behind the gate", "action.blocked.unwired"));
    return receipt("blocked", policy.risk);
  }

  const risk = effectiveRisk(effector.kind, policy.risk);

  // Action backing: an `act` effector or any high-risk action must be backed by
  // at least one trusted memory that permits `may_use_for_action`. Mirrors the
  // grader's action-permission block (grader.ts: grade.blocked.action_permission).
  if (effector.kind === "act" || risk === "high") {
    const actionBacked = args.frame.trusted.some((memory) => memory.packet.permissions.may_use_for_action);
    if (!actionBacked) {
      rules.push(
        rule(
          "may_use_for_action",
          false,
          "safe_context_frame.trusted",
          "no trusted memory permits this action",
          "action.blocked.no_action_grant",
        ),
      );
      return receipt("blocked", risk);
    }
  }

  // --- Confirmation floor. High/medium risk always confirms, in every posture;
  // low risk defers to the interaction posture. Confirmation can clear ONLY this
  // gate, never the hard blocks above. ---

  if (requiresConfirmation(args.posture, risk)) {
    if (confirmed) {
      rules.push(
        rule(
          "confirmed",
          true,
          `user_posture:${args.posture}`,
          "user explicitly confirmed the action; confirmation override recorded",
          "action.allow.confirmed",
        ),
      );
      return receipt("allow", risk);
    }
    rules.push(
      rule(
        "risk",
        risk,
        `user_posture:${args.posture}`,
        "action risk requires explicit confirmation before it may run",
        "action.confirm.risk_floor",
      ),
    );
    return receipt("needs_confirmation", risk);
  }

  rules.push(rule("risk", risk, `user_posture:${args.posture}`, "action is authorized to run", "action.allow"));
  return receipt("allow", risk);
}
