// Effector executors — the world-facing side of the membrane.
//
// The action gate (src/core/actionGate.ts) decides whether an effect is AUTHORIZED.
// An executor is what actually performs it once authorized, and it produces an
// `ExecutionReceipt` (the nondeterministic, effect-shaped record) — kept separate
// from the deterministic, policy-shaped `ActionReceipt`.
//
// Two hard invariants live here:
//   1. No-execute-on-block: an executor is ONLY ever invoked after the gate returns
//      `allow`. `handleActionRequest` enforces this; an executor never re-checks policy.
//   2. Belt-and-suspenders target guard: even when called, the repo_edit executor
//      refuses any target outside the designated proof directory. The gate already
//      hard-blocks protected targets; this is a second, independent floor.
//
// Executors are injectable: the browser surface has no filesystem and passes none
// (so allowed actions record `executor_called: false`, outcome `skipped` — an honest
// "would run here"); the Node soul-server injects real ones.

import { canonicalizeRepoPath, type ActionIntent, type ActionRoute, type ExecutionReceipt } from "./core";
import type { EffectorId } from "./buddyManifest";

export interface ExecutionContext {
  intent: ActionIntent;
  buddy: string;
  route: ActionRoute;
  /** The ActionReceipt that authorized this execution — pins the audit pair. */
  actionReceiptId: string;
  now: string;
}

export interface ExecutorOutcome {
  outcome: ExecutionReceipt["outcome"];
  detail?: string;
}

export type EffectorExecutor = (ctx: ExecutionContext) => ExecutorOutcome;

export type ExecutorRegistry = Partial<Record<EffectorId, EffectorExecutor>>;

/** The only repo path the repo_edit executor will ever touch — the act-effector proof sandbox. */
export const REPO_EDIT_PROOF_DIR = ".border-agents/proofs/";

/**
 * The default repo_edit executor. It re-asserts the proof-directory guard independently
 * of the gate (defense in depth), using the same `..`-resolving canonicalization the gate
 * uses so a traversal target (`.border-agents/proofs/../../etc`) can never escape the
 * sandbox. This build does not write to disk — it records an honest `ok` for a sandbox
 * target so the audit pair (ActionReceipt + ExecutionReceipt) is complete; the soul-server
 * can inject a disk-writing executor.
 */
export const repoEditProofExecutor: EffectorExecutor = (ctx) => {
  const sandbox = canonicalizeRepoPath(REPO_EDIT_PROOF_DIR).path;
  const { path, escapes } = canonicalizeRepoPath(ctx.intent.target.path);
  const inSandbox = !escapes && (path === sandbox || path.startsWith(`${sandbox}/`));
  if (ctx.intent.target.kind !== "repo_path" || !inSandbox) {
    return {
      outcome: "skipped",
      detail: `executor sandbox: repo_edit only operates under ${REPO_EDIT_PROOF_DIR}; refused "${ctx.intent.target.path}"`,
    };
  }
  return { outcome: "ok", detail: `proof executor authorized ${ctx.intent.operation} on ${path} (no disk write in this build)` };
};

/** The executors wired on a surface with no real I/O backing (e.g. the browser body). */
export const DEFAULT_EXECUTORS: ExecutorRegistry = {
  repo_edit: repoEditProofExecutor,
};

/** Build an ExecutionReceipt from the context and the executor's outcome. */
export function buildExecutionReceipt(
  ctx: ExecutionContext,
  executorCalled: boolean,
  outcome: ExecutorOutcome,
): ExecutionReceipt {
  return {
    receipt_id: `exec:${ctx.buddy}:${ctx.intent.effectorId}:${ctx.now}`,
    action_receipt_id: ctx.actionReceiptId,
    effector: ctx.intent.effectorId,
    buddy: ctx.buddy,
    operation: ctx.intent.operation,
    target: ctx.intent.target,
    route: ctx.route,
    executor_called: executorCalled,
    outcome: outcome.outcome,
    detail: outcome.detail,
    executed_at: ctx.now,
  };
}
