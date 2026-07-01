import { describe, expect, test, beforeAll } from "vitest";
import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { handleActionRequest, decisionAlertLevel } from "../soulActions";
import { readReceiptLedger } from "../receiptLedger";
import type { BuddySettings } from "../buddyProfiles";
import type { SessionChatLine } from "../liveGovernance";

// Slice 4 — the verify. Closes the wire↔ledger↔body loop deterministically:
//   1. drive `handleActionRequest` to produce a real `action_result` envelope with a grade
//      whose `backedBy` ids are the trusted GradeReceipt ids the ledger persisted (law 6);
//   2. pipe that exact envelope through the Rust `bb-parse-action-result` harness — the
//      same `parse_to_body` the live desktop body runs — and read back the `ActionGrade` it
//      reconstructs;
//   3. assert all three agree: envelope.grade.backedBy === harness.backed_by === ledger
//      trusted receipt ids. No layer can drift silently; the body's "Authorized by N graded
//      (M trusted)" claim now traces byte-identically to the ledger.
//
// The harness is built once in `beforeAll`. It is a test-only binary
// (`desktop-body/src/bin/parse_action_result.rs`) that shares `presence.rs` with the main
// body via `#[path]`, so the parser exercised here is byte-identical to the runtime one.

const HERE = dirname(fileURLToPath(import.meta.url));
const DESKTOP_BODY = resolve(HERE, "../../desktop-body");
const HARNESS_BIN = join(DESKTOP_BODY, "target/debug/bb-parse-action-result");

const BASE_SETTINGS: BuddySettings = {
  enabled: true,
  provider: "grok",
  modelLabel: "Grok subscription",
  connectionLabel: "Connected",
  allowAction: false,
  allowExternalShare: false,
  memoryMode: "purpose_graded",
};

// An assistant line grades `trusted` (it may_assert) and, with allowAction on, carries
// may_use_for_action — so it backs a high-risk action. A user line would grade `limited`.
const ACTION_HISTORY: SessionChatLine[] = [{ role: "assistant", text: "Applied the reviewed patch." }];

function memoryStorage(): Storage {
  const map = new Map<string, string>();
  return {
    get length() {
      return map.size;
    },
    clear: () => map.clear(),
    getItem: (k) => (map.has(k) ? map.get(k)! : null),
    key: (i) => Array.from(map.keys())[i] ?? null,
    removeItem: (k) => void map.delete(k),
    setItem: (k, v) => void map.set(k, String(v)),
  };
}

function repoIntent(path: string, operation = "write_patch") {
  return {
    effectorId: "repo_edit" as const,
    operation,
    target: { kind: "repo_path" as const, path },
    payloadDigest: "sha256:patch",
    summary: `${operation} ${path}`,
  };
}

/** Pipe one envelope JSON through the Rust harness and return its parsed JSON output. */
function parseViaRust(envelopeJson: string): {
  ok: boolean;
  grade?: { graded: number; trusted: number; backed_by: string[] } | null;
  alertLevel?: string | null;
} {
  const result = spawnSync(HARNESS_BIN, [], {
    cwd: DESKTOP_BODY,
    input: envelopeJson,
    encoding: "utf8",
    timeout: 10_000,
  });
  if (result.status !== 0) {
    throw new Error(
      `harness exited ${result.status}: stderr=${result.stderr ?? ""} stdout=${result.stdout ?? ""}`,
    );
  }
  return JSON.parse(result.stdout);
}

beforeAll(() => {
  // Always run `cargo build` — let cargo's incremental compilation be the staleness check.
  // Skipping when the binary exists would substitute "file exists = fresh" for cargo's
  // correct mtime check, so editing `presence.rs` in local dev would leave the trace
  // running against a stale parser — the exact "tests green, trust broken" shape this
  // whole arc has been hunting. Warm builds are sub-second; the cost is negligible.
  const build = spawnSync(
    "cargo",
    ["build", "--bin", "bb-parse-action-result", "--quiet"],
    { cwd: DESKTOP_BODY, encoding: "utf8", timeout: 180_000 },
  );
  if (build.status !== 0) {
    throw new Error(`cargo build failed: ${build.stderr ?? ""}`);
  }
  if (!existsSync(HARNESS_BIN)) {
    throw new Error(`harness binary not found at ${HARNESS_BIN} after build`);
  }
}, 180_000);

describe("governance vertical slice 4 — wire↔ledger↔body end-to-end trace", () => {
  const ACTING: BuddySettings = { ...BASE_SETTINGS, allowAction: true };

  test("the body reconstructs the same grade the soul emitted and the ledger persisted", () => {
    const storage = memoryStorage();
    const intent = repoIntent(".border-agents/proofs/slice4.patch");
    // First request → needs_confirmation (high risk), then confirm → allow + execute.
    handleActionRequest({
      buddy: "forge", effectorId: "repo_edit", settings: ACTING, posture: "work",
      history: ACTION_HISTORY, intent, storage, now: "2026-06-13T12:00:00Z",
    });
    const { receipt, result, snapshot } = handleActionRequest({
      buddy: "forge", effectorId: "repo_edit", settings: ACTING, posture: "work",
      history: ACTION_HISTORY, intent, confirmed: true, storage, now: "2026-06-13T12:00:01Z",
    });

    expect(receipt.decision).toBe("allow");
    const grade = result.grade;
    expect(grade).toBeDefined();
    expect(grade!.trusted).toBeGreaterThan(0);

    // (1) The ledger's trusted GradeReceipt ids — law 6's persisted trail.
    const ledger = readReceiptLedger(storage);
    const ledgerTrustedIds = ledger.flatMap((e) =>
      e.kind === "memory" ? e.receipts.filter((r) => r.grade === "trusted").map((r) => r.receipt_id) : [],
    );
    expect(ledgerTrustedIds.length).toBeGreaterThan(0);

    // (2) The exact envelope that goes on the wire, piped through the Rust body parser.
    const wireJson = JSON.stringify(result);
    const parsed = parseViaRust(wireJson);
    expect(parsed.ok).toBe(true);
    expect(parsed.grade).not.toBeNull();
    const bodyBackedBy = parsed.grade!.backed_by;
    expect(parsed.grade!.graded).toBe(grade!.graded);
    expect(parsed.grade!.trusted).toBe(grade!.trusted);

    // (3) Three-way agreement: wire === body === ledger. No layer can drift silently.
    expect(bodyBackedBy).toEqual(grade!.backedBy);
    // Every id the body reconstructed traces to a grade the ledger actually persisted.
    for (const id of bodyBackedBy) expect(ledgerTrustedIds).toContain(id);
    // And the trail the soul projected is a subset of the ledger's trusted grades — the
    // body's "Authorized by N graded (M trusted)" claim is backed by real persisted receipts.
    for (const id of grade!.backedBy) expect(ledgerTrustedIds).toContain(id);

    // The snapshot's frame.receipts is the authoritative source both the wire and the
    // ledger were projected from — close the loop on it too.
    const snapshotTrustedIds = snapshot!.frame.trusted.map(
      (m) => snapshot!.frame.receipts.find((r) => r.chunk_id === m.chunk_id)!.receipt_id,
    );
    expect(bodyBackedBy).toEqual(snapshotTrustedIds);
  });

  test("a no-grade envelope (memory off) parses cleanly with grade: null — backward-compat", () => {
    const storage = memoryStorage();
    const { result } = handleActionRequest({
      buddy: "veritas", effectorId: "receipt_review",
      settings: { ...BASE_SETTINGS, memoryMode: "off" }, posture: "play",
      history: ACTION_HISTORY, storage, now: "2026-06-13T12:00:00Z",
    });
    expect(result.grade).toBeUndefined();
    // The golden action_result shape (no grade field) parses; the body renders no rail it
    // cannot back. This is the back-compat anchor the golden fixture also pins from the Rust side.
    const parsed = parseViaRust(JSON.stringify(result));
    expect(parsed.ok).toBe(true);
    expect(parsed.grade).toBeNull();
  });

  test("a malformed grade on the wire drops the whole cue — strict-parse mirrors isActionGrade", () => {
    // Hand-craft an envelope with a non-integer `trusted`. The TS validator drops it; the
    // Rust parser must drop it too — never render a half-known basis for an authorization.
    const malformed = JSON.stringify({
      protocol: "presence",
      v: 0,
      kind: "action_result",
      buddy: "forge",
      ts: 1,
      effector: "repo_edit",
      decision: "allow",
      receiptId: "r1",
      grade: { graded: 1, trusted: "two", backedBy: [] },
    });
    const parsed = parseViaRust(malformed);
    expect(parsed.ok).toBe(false);
  });

  // R1 — the alertLevel seed of "extend the trace to decision→alertLevel→hue". The soul already
  // derives a glanceable alert tier from every gate decision (`decisionAlertLevel`); R1 threads it
  // to the body side of the wire. This closes the same wire===body loop the grade trace closes,
  // one slice earlier than the ring that will paint the hue: prove the tier is not discarded.
  test("the body reconstructs the same alertLevel the soul derived from each decision", () => {
    const storage = memoryStorage();
    const intent = repoIntent(".border-agents/proofs/slice-r1.patch");
    // needs_confirmation (high risk), then confirm → allow.
    const first = handleActionRequest({
      buddy: "forge", effectorId: "repo_edit", settings: ACTING, posture: "work",
      history: ACTION_HISTORY, intent, storage, now: "2026-06-13T12:00:00Z",
    });
    const second = handleActionRequest({
      buddy: "forge", effectorId: "repo_edit", settings: ACTING, posture: "work",
      history: ACTION_HISTORY, intent, confirmed: true, storage, now: "2026-06-13T12:00:01Z",
    });
    // A protected target (AGENTS.md) hard-blocks regardless of grant/confirmation.
    const blocked = handleActionRequest({
      buddy: "forge", effectorId: "repo_edit", settings: ACTING, posture: "work",
      history: ACTION_HISTORY, intent: repoIntent("AGENTS.md"), storage, now: "2026-06-13T12:00:02Z",
    });

    // The flow exercises three distinct decisions — assert that union, not one arm.
    expect([first, second, blocked].map((r) => r.receipt.decision)).toEqual([
      "needs_confirmation",
      "allow",
      "blocked",
    ]);

    for (const { receipt, result } of [first, second, blocked]) {
      // (soul) the wire carries exactly what decisionAlertLevel maps the decision to.
      const expected = decisionAlertLevel(receipt.decision);
      expect(result.alertLevel).toBe(expected);
      // (body) the same parser the live body runs reconstructs the identical tier — the signal is
      // on the body side of the wire, not discarded. wire === body on the alert tier.
      const parsed = parseViaRust(JSON.stringify(result));
      expect(parsed.ok).toBe(true);
      expect(parsed.alertLevel).toBe(expected);
    }
  });
});
