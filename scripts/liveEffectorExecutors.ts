// Node-only live executors — the disk-writing counterpart to the browser-safe proof
// executors in src/effectorExecutors.ts. This module imports `node:fs`/`node:path`, so it
// lives in scripts/ (Node land, run via tsx, outside the browser `tsc` graph) and must never
// be pulled into the browser graph (BuddySurface → soulActions → effectorExecutors).
// Node contexts import it: the soul-server and the harm demo / its on-disk test.
//
// This is the first executor that performs a real, irreversible world effect (a file write).
// Two floors keep it safe, and they are INDEPENDENT of each other:
//   1. The gate (src/core/actionGate.ts) hard-blocks protected targets and unbacked actions
//      BEFORE any executor is reachable (no-execute-on-block, enforced by handleActionRequest).
//   2. This executor re-asserts the sandbox guard itself, using the same `..`-resolving
//      canonicalization the gate uses — so even if it were ever invoked for a target outside
//      `.border-agents/proofs/`, it refuses (outcome `skipped`) and writes nothing. A traversal
//      target (`.border-agents/proofs/../../AGENTS.md`) collapses to an escape and is refused.
//
// The intent carries a payloadDigest/summary, not file bytes, so the executor writes a proof
// artifact recording WHAT was authorized (operation, summary, digest, and the authorizing
// ActionReceipt id) — an honest on-disk record that the membrane permitted this write.

import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, join, resolve, sep } from "node:path";

import { canonicalizeRepoPath } from "../src/core";
import { REPO_EDIT_PROOF_DIR, type EffectorExecutor, type ExecutionContext } from "../src/effectorExecutors";

/** The on-disk content the executor writes — a record of the authorized effect, not raw bytes. */
function proofArtifact(ctx: ExecutionContext, canonicalPath: string): string {
  return [
    "# Border Agents — execution proof",
    "",
    "This file was written by the repo_edit act-effector AFTER the action gate authorized it.",
    "",
    `operation:          ${ctx.intent.operation}`,
    `target:             ${canonicalPath}`,
    `payload_digest:     ${ctx.intent.payloadDigest ?? "(none)"}`,
    `summary:            ${ctx.intent.summary ?? "(none)"}`,
    `buddy:              ${ctx.buddy}`,
    `route:              ${ctx.route.provider} (${ctx.route.locality}${ctx.route.downgraded ? ", downgraded" : ""})`,
    `authorized_by:      ${ctx.actionReceiptId}`,
    `executed_at:        ${ctx.now}`,
    "",
  ].join("\n");
}

/**
 * Build a live repo_edit executor rooted at `root` (defaults to the process cwd). All writes
 * land under `root`/`.border-agents/proofs/` and nowhere else; the canonicalized target is
 * re-checked against that sandbox before any filesystem call. Filesystem errors are caught and
 * reported as outcome `error` (the audit pair still records the attempt) rather than thrown.
 */
export function createLiveRepoEditExecutor(opts: { root?: string } = {}): EffectorExecutor {
  const root = opts.root ?? process.cwd();
  const sandboxCanonical = canonicalizeRepoPath(REPO_EDIT_PROOF_DIR).path;
  const sandboxAbsolute = resolve(root, sandboxCanonical);

  return (ctx) => {
    if (ctx.intent.target.kind !== "repo_path") {
      return { outcome: "skipped", detail: `executor sandbox: repo_edit only writes repo paths; refused ${ctx.intent.target.kind} target` };
    }

    const { path: canonicalPath, escapes } = canonicalizeRepoPath(ctx.intent.target.path);
    const inSandbox = !escapes && (canonicalPath === sandboxCanonical || canonicalPath.startsWith(`${sandboxCanonical}/`));
    if (!inSandbox) {
      return {
        outcome: "skipped",
        detail: `executor sandbox: repo_edit only operates under ${REPO_EDIT_PROOF_DIR}; refused "${ctx.intent.target.path}"`,
      };
    }

    // Final defense: resolve to an absolute path and confirm it is genuinely inside the sandbox
    // directory before touching the filesystem (guards against any platform path quirk).
    const absolute = resolve(root, canonicalPath);
    if (absolute !== sandboxAbsolute && !absolute.startsWith(`${sandboxAbsolute}${sep}`)) {
      return { outcome: "skipped", detail: `executor sandbox: resolved path escaped ${REPO_EDIT_PROOF_DIR}; refused "${ctx.intent.target.path}"` };
    }

    try {
      mkdirSync(dirname(absolute), { recursive: true });
      const body = proofArtifact(ctx, canonicalPath);
      writeFileSync(absolute, body, "utf8");
      return { outcome: "ok", detail: `wrote ${canonicalPath} (${Buffer.byteLength(body, "utf8")} bytes) under ${REPO_EDIT_PROOF_DIR}` };
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      return { outcome: "error", detail: `repo_edit write failed for ${canonicalPath}: ${message}` };
    }
  };
}

/** The sandbox directory an executor rooted at `root` will write into — useful for demos/tests. */
export function liveExecutorSandbox(root: string = process.cwd()): string {
  return join(root, canonicalizeRepoPath(REPO_EDIT_PROOF_DIR).path);
}
