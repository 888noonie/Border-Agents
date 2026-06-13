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

import type { EffectorId, EffectorKind, EffectorSpec, RouteProvider } from "../buddyManifest";
import { getPurposePolicy, type BuiltInPurpose } from "./policies";
import type { DerivationStep, PurposePolicy, SafeContextFrame } from "./types";
import { requiresConfirmation, resolvePosturePolicy, type UserPosture } from "./userPosture";

export type ActionDecision = "allow" | "needs_confirmation" | "blocked";

// --- The execution membrane (GPT roundtable, 2026-06-13) ----------------------
//
// An effector grant ("Forge may edit the repo") is not enough to authorize an
// effect ("write THIS patch to AGENTS.md"). The gate therefore evaluates a typed
// `ActionIntent` — the specific operation + target the body wants — not just the
// effector id. This is what lets a single `repo_edit` effector be allowed for a
// scratch file yet hard-blocked for `AGENTS.md`. Authorization is intent-level.

export interface ActionTarget {
  /** What kind of thing the action touches; drives which protection policy applies. */
  kind: "repo_path" | "file_path" | "url" | "command";
  /** The concrete target — a repo-relative path, URL, or command string. */
  path: string;
}

export interface ActionIntent {
  effectorId: EffectorId;
  /** Effector-specific verb, e.g. "write_patch" | "apply_patch". */
  operation: string;
  target: ActionTarget;
  /** Hash of the payload (diff/command/body) so the receipt pins WHAT was authorized. */
  payloadDigest?: string;
  /** One-line, user-facing description of the intended effect. */
  summary: string;
}

// The provider route the buddy is currently running on. Carried separately from the
// policy decision because a route change is itself a trust-boundary crossing: a
// buddy that silently falls back from a local/low-retention route to a cloud route
// has a different trust posture even though the user did nothing. `downgraded` makes
// that crossing explicit so the gate can refuse to let it happen silently.
export interface ActionRoute {
  provider: RouteProvider;
  /** The preferred provider this route fell back from, if any. */
  fallbackOf?: RouteProvider;
  locality: "local" | "cloud";
  /** True when this route is lower-trust than the buddy's preferred route. */
  downgraded: boolean;
}

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

// The world-facing outcome of running an authorized action. Kept SEPARATE from
// `ActionReceipt`: authorization is deterministic and policy-shaped; execution is
// nondeterministic and effect-shaped. They are different borders. An ExecutionReceipt
// is only ever produced when the gate returned `allow` AND the executor ran — and it
// records the route that actually carried the effect (provider provenance).
export interface ExecutionReceipt {
  receipt_id: string; // `exec:{buddy}:{effector}:{executed_at}`
  /** Links back to the authorizing ActionReceipt — the pair is the full audit story. */
  action_receipt_id: string;
  effector: EffectorId;
  buddy: string;
  operation: string;
  target: ActionTarget;
  /** Which provider route carried this effect — the "buddies persist, providers rotate" trail. */
  route: ActionRoute;
  /** Always false unless the gate allowed the action. The no-execute-on-block invariant. */
  executor_called: boolean;
  outcome: "ok" | "error" | "skipped";
  detail?: string;
  executed_at: string;
}

// Repo targets the gate will NEVER write to, regardless of grant, backing, or
// confirmation. These are the trust-critical surfaces (the laws, the gate itself,
// the dependency manifest, version control). Matched by exact path or path prefix.
export const PROTECTED_REPO_TARGETS: readonly string[] = [
  "AGENTS.md",
  "src/core/",
  ".git/",
  "package.json",
  "package-lock.json",
];

/** Normalize a repo-relative path for protection matching (strip `./`, leading `/`). */
function normalizeRepoPath(path: string): string {
  return path.replace(/^\.\//, "").replace(/^\/+/, "");
}

/**
 * Is this intent aimed at a protected target? Only repo-path targets are guarded here;
 * other target kinds get their own policy as effectors graduate. Pure and exported so
 * surfaces and tests can pre-flight a target without invoking the full gate.
 */
export function isProtectedTarget(intent: ActionIntent): boolean {
  if (intent.target.kind !== "repo_path") {
    return false;
  }
  const path = normalizeRepoPath(intent.target.path);
  return PROTECTED_REPO_TARGETS.some((p) => path === p || path.startsWith(p));
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
  /** The specific operation + target. When present, the gate authorizes the EFFECT, not just the effector. */
  intent?: ActionIntent;
  /** The provider route the buddy is running on. A downgraded route forces confirmation. */
  route?: ActionRoute;
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

  // Intent-level hard block: a protected target may never be written, regardless of
  // grant, backing, or confirmation. This is the membrane — the gate authorizes the
  // EFFECT, not just the effector. Runs before backing/confirmation so it can never be
  // waived. Mirrors the manifest's "reachable, not replace" stance at the target level.
  if (args.intent && isProtectedTarget(args.intent)) {
    rules.push(
      rule(
        "target",
        args.intent.target.path,
        "action_intent.target",
        "target is a protected surface and may never be written by an effector",
        "action.blocked.protected_target",
      ),
    );
    return receipt("blocked", effectiveRisk(effector.kind, policy.risk));
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
  // low risk defers to the interaction posture. A DOWNGRADED route also forces the
  // floor: degradation to a lower-trust provider is never silent. Confirmation can
  // clear ONLY this floor, never the hard blocks above. ---

  const riskFloor = requiresConfirmation(args.posture, risk);
  const routeDowngraded = args.route?.downgraded === true;

  if (riskFloor || routeDowngraded) {
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
    // Record the stronger reason first: the risk floor if it applies, else the route
    // downgrade. Either way the action holds until the user confirms.
    if (riskFloor) {
      rules.push(
        rule(
          "risk",
          risk,
          `user_posture:${args.posture}`,
          "action risk requires explicit confirmation before it may run",
          "action.confirm.risk_floor",
        ),
      );
    } else {
      rules.push(
        rule(
          "route",
          args.route?.provider ?? "unknown",
          args.route?.fallbackOf ? `route_fallback:${args.route.fallbackOf}` : "route_fallback",
          "buddy fell back to a lower-trust route; confirm before continuing on it",
          "action.confirm.route_downgrade",
        ),
      );
    }
    return receipt("needs_confirmation", risk);
  }

  rules.push(rule("risk", risk, `user_posture:${args.posture}`, "action is authorized to run", "action.allow"));
  return receipt("allow", risk);
}
