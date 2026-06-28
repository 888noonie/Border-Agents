import { describe, expect, test } from "vitest";
import {
  handleActionRequest,
  actionGradeSummary,
  parseActionCommand,
  presenceIntentToActionIntent,
  decisionEmotion,
  decisionAlertLevel,
  routeHealthFromSoul,
} from "../soulActions";
import { readReceiptLedger } from "../receiptLedger";
import type { BuddySettings } from "../buddyProfiles";
import type { SessionChatLine } from "../liveGovernance";

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

/** Minimal synchronous in-memory Storage so the handler tests stay pure (no jsdom). */
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

describe("handleActionRequest", () => {
  test("blocks an unknown effector without throwing, and appends a receipt", () => {
    const storage = memoryStorage();
    const { receipt, result } = handleActionRequest({
      buddy: "veritas",
      effectorId: "definitely_not_an_effector",
      settings: BASE_SETTINGS,
      posture: "work",
      history: ACTION_HISTORY,
      storage,
      now: "2026-06-13T12:00:00Z",
    });

    expect(receipt.decision).toBe("blocked");
    expect(receipt.rules.some((r) => r.policy_rule === "action.blocked.unknown_effector")).toBe(true);
    expect(result.decision).toBe("blocked");
    expect(result.receiptId).toBe(receipt.receipt_id);
    expect(readReceiptLedger(storage)).toHaveLength(1);
  });

  test("the action_result cue carries alertLevel as the chrome twin of its decision", () => {
    const storage = memoryStorage();
    // blocked path — unknown effector → decision "blocked" → alertLevel "blocked"
    const blocked = handleActionRequest({
      buddy: "veritas",
      effectorId: "definitely_not_an_effector",
      settings: BASE_SETTINGS,
      posture: "work",
      history: ACTION_HISTORY,
      storage,
      now: "2026-06-13T12:00:00Z",
    });
    expect(blocked.result.decision).toBe("blocked");
    expect(blocked.result.alertLevel).toBe("blocked");

    // allow path — low-risk receipt_review under play → decision "allow" → alertLevel "ready"
    const allowed = handleActionRequest({
      buddy: "veritas",
      effectorId: "receipt_review",
      settings: BASE_SETTINGS,
      posture: "play",
      history: [{ role: "user", text: "What changed today?" }],
      storage,
      now: "2026-06-13T12:00:01Z",
    });
    expect(allowed.result.decision).toBe("allow");
    expect(allowed.result.alertLevel).toBe("ready");
  });

  test("low-risk receipt_review under play posture is allowed and recorded", () => {
    const storage = memoryStorage();
    const { receipt, result } = handleActionRequest({
      buddy: "veritas",
      effectorId: "receipt_review",
      settings: BASE_SETTINGS, // allowAction false → summarize_history (low risk)
      posture: "play",
      history: [{ role: "user", text: "What changed today?" }],
      requestId: "req-7",
      storage,
      now: "2026-06-13T12:00:00Z",
    });

    expect(receipt.decision).toBe("allow");
    expect(result.decision).toBe("allow");
    expect(result.requestId).toBe("req-7");
    // The grade that justified the allow is recorded FIRST, then the action receipt:
    // "graded X, then authorized X" (law 6 — every grade produces a receipt).
    const ledger = readReceiptLedger(storage);
    expect(ledger).toHaveLength(2);
    expect(ledger[0].kind).toBe("memory");
    expect(ledger[1].kind).toBe("action");
  });

  test("memory-off, action-enabled request fails closed (high risk, no backing)", () => {
    const storage = memoryStorage();
    const { receipt } = handleActionRequest({
      buddy: "veritas",
      effectorId: "receipt_review",
      settings: { ...BASE_SETTINGS, allowAction: true, memoryMode: "off" },
      posture: "work",
      history: ACTION_HISTORY,
      storage,
      now: "2026-06-13T12:00:00Z",
    });

    expect(receipt.decision).toBe("blocked");
    expect(receipt.rules.some((r) => r.policy_rule === "action.blocked.no_action_grant")).toBe(true);
  });

  test("confirm round-trip records two receipts: needs_confirmation then allow", () => {
    const storage = memoryStorage();
    const settings: BuddySettings = { ...BASE_SETTINGS, allowAction: true }; // agent_action (high)

    const first = handleActionRequest({
      buddy: "veritas",
      effectorId: "receipt_review",
      settings,
      posture: "work",
      history: ACTION_HISTORY,
      requestId: "req-1",
      storage,
      now: "2026-06-13T12:00:00Z",
    });
    expect(first.receipt.decision).toBe("needs_confirmation");

    const second = handleActionRequest({
      buddy: "veritas",
      effectorId: "receipt_review",
      settings,
      posture: "work",
      history: ACTION_HISTORY,
      confirmed: true,
      requestId: "req-1",
      storage,
      now: "2026-06-13T12:00:05Z", // later — distinct receipt id, so both persist
    });
    expect(second.receipt.decision).toBe("allow");
    expect(second.receipt.confirmed).toBe(true);

    // Each gate pass re-grades its backing, so the round-trip records two action receipts
    // (needs_confirmation then allow), each preceded by its own grade entry: 2 memory + 2 action.
    const ledger = readReceiptLedger(storage);
    const actions = ledger.filter((e) => e.kind === "action");
    const memories = ledger.filter((e) => e.kind === "memory");
    expect(actions).toHaveLength(2);
    expect(memories).toHaveLength(2);
    expect(actions.map((e) => (e.kind === "action" ? e.decision : null))).toEqual([
      "needs_confirmation",
      "allow",
    ]);
  });

  // Regression: the dock/body addresses buddies by persona id ("owl"), but grants are keyed
  // by governance id ("veritas"). The request must resolve to the governance identity, not
  // fall through to a blocked/ungranted receipt. The result cue stays addressed to the persona.
  test("a persona-id request resolves to the governance identity (owl → veritas)", () => {
    const storage = memoryStorage();
    const { receipt, result } = handleActionRequest({
      buddy: "owl",
      effectorId: "receipt_review",
      settings: BASE_SETTINGS,
      posture: "work",
      history: [{ role: "user", text: "What changed today?" }],
      storage,
      now: "2026-06-13T12:00:00Z",
    });

    // Authorized (not ungranted) — under work, low-risk reach hits the confirmation floor.
    expect(receipt.decision).toBe("needs_confirmation");
    expect(receipt.rules.some((r) => r.policy_rule === "action.blocked.ungranted")).toBe(false);
    // Receipt/ledger carry the governance identity; the cue is addressed back to the persona.
    expect(receipt.buddy).toBe("veritas");
    expect(result.buddy).toBe("owl");
    // Both the grade entry and the action entry are keyed to the governance identity (the
    // audit subject), not the requesting persona.
    const ledger = readReceiptLedger(storage);
    expect(ledger).toHaveLength(2);
    expect(ledger.every((e) => e.buddyId === "veritas")).toBe(true);
  });

  test("aether can attach local_chat under private posture without an ungranted block", () => {
    const storage = memoryStorage();
    const { receipt } = handleActionRequest({
      buddy: "aether",
      effectorId: "local_chat",
      settings: BASE_SETTINGS,
      posture: "private",
      history: [{ role: "user", text: "hello local model" }],
      intent: {
        effectorId: "local_chat",
        operation: "open_session",
        target: { kind: "url", path: "LM Studio" },
        summary: "open private local chat",
      },
      route: { provider: "lm_studio", locality: "local", downgraded: false },
      storage,
      now: "2026-06-13T12:00:00Z",
    });

    expect(receipt.decision).toBe("needs_confirmation");
    expect(receipt.rules.some((r) => r.policy_rule === "action.blocked.ungranted")).toBe(false);
    expect(receipt.rules.some((r) => r.policy_rule === "action.confirm.risk_floor")).toBe(true);
  });

  test("aether placeholder surfaces can block as known-but-unwired effectors", () => {
    const storage = memoryStorage();
    const { receipt } = handleActionRequest({
      buddy: "aether",
      effectorId: "summarize_long",
      settings: BASE_SETTINGS,
      posture: "work",
      history: [{ role: "user", text: "open the placeholder" }],
      storage,
      now: "2026-06-13T12:00:00Z",
    });

    expect(receipt.decision).toBe("blocked");
    expect(receipt.rules.some((r) => r.policy_rule === "action.blocked.unwired")).toBe(true);
    expect(receipt.rules.some((r) => r.policy_rule === "action.blocked.unknown_effector")).toBe(false);
    expect(receipt.rules.some((r) => r.policy_rule === "action.blocked.ungranted")).toBe(false);
  });
});

// The governance vertical slice: a buddy action grades memory and that grade is now
// persisted as a GradeReceipt trail (law 6), not just consumed as a boolean by the gate.
// The grade lands BEFORE the action it justified — the "graded X, then authorized X" story.
describe("grade-receipt persistence (the governance join)", () => {
  test("an allowed action records the backing grade first, and returns the snapshot", () => {
    const storage = memoryStorage();
    const { receipt, snapshot } = handleActionRequest({
      buddy: "veritas",
      effectorId: "receipt_review",
      settings: BASE_SETTINGS, // summarize_history, low risk
      posture: "play",
      history: [{ role: "user", text: "What changed today?" }],
      storage,
      now: "2026-06-13T12:00:00Z",
    });

    expect(receipt.decision).toBe("allow");
    // The grade that backed the gate is returned to the caller (the wire/body slices project it).
    expect(snapshot?.frame.receipts.length).toBeGreaterThan(0);

    const ledger = readReceiptLedger(storage);
    const memory = ledger.find((e) => e.kind === "memory");
    const action = ledger.find((e) => e.kind === "action");
    expect(memory).toBeDefined();
    expect(action).toBeDefined();
    // Ordering: the grade is persisted ahead of the authorization it justified.
    expect(ledger.indexOf(memory!)).toBeLessThan(ledger.indexOf(action!));
    // The persisted grade carries the actual per-chunk GradeReceipts.
    if (memory && memory.kind === "memory") {
      expect(memory.receipts.length).toBeGreaterThan(0);
      expect(memory.receipts[0].purpose).toBe("summarize_history");
    }
  });

  test("a blocked action still records the grade that justified the block, and nothing executes", () => {
    const storage = memoryStorage();
    const { receipt, execution } = handleActionRequest({
      buddy: "forge",
      effectorId: "repo_edit",
      settings: { ...BASE_SETTINGS, allowAction: true }, // agent_action, memory on
      posture: "work",
      history: ACTION_HISTORY,
      intent: {
        effectorId: "repo_edit",
        operation: "apply_patch",
        target: { kind: "repo_path", path: "AGENTS.md" }, // protected → hard block
        summary: "apply_patch AGENTS.md",
      },
      confirmed: true,
      storage,
      now: "2026-06-13T12:00:00Z",
    });

    expect(receipt.decision).toBe("blocked");
    expect(execution).toBeUndefined();
    const ledger = readReceiptLedger(storage);
    // Law 6 holds on the block path too: the grade is receipted even though the action did not run.
    expect(ledger.some((e) => e.kind === "memory")).toBe(true);
    expect(ledger.some((e) => e.kind === "execution")).toBe(false);
  });

  test("memory off records no grade and returns no snapshot (nothing to receipt, fails closed)", () => {
    const storage = memoryStorage();
    const { receipt, snapshot } = handleActionRequest({
      buddy: "veritas",
      effectorId: "receipt_review",
      settings: { ...BASE_SETTINGS, allowAction: true, memoryMode: "off" },
      posture: "work",
      history: ACTION_HISTORY,
      storage,
      now: "2026-06-13T12:00:00Z",
    });

    expect(receipt.decision).toBe("blocked"); // high risk, no backing
    expect(snapshot).toBeUndefined();
    expect(readReceiptLedger(storage).some((e) => e.kind === "memory")).toBe(false);
  });
});

// Slice 2 of the governance join: the grade that backed the gate is projected onto the
// action_result wire as `grade` so a dumb body can render an honest "authorized by N graded
// memories (M trusted)" rail. backedBy carries the receipt ids (the audit trail), and they
// must be the SAME ids the ledger persisted — the wire cannot be allowed to disagree with law 6.
describe("grade projection onto the action_result wire (Slice 2)", () => {
  const ACTING: BuddySettings = { ...BASE_SETTINGS, allowAction: true };
  function repoIntent(path: string, operation = "write_patch") {
    return {
      effectorId: "repo_edit" as const,
      operation,
      target: { kind: "repo_path" as const, path },
      payloadDigest: "sha256:patch",
      summary: `${operation} ${path}`,
    };
  }

  test("a null snapshot projects no grade — fail-closed, no grade no claim", () => {
    expect(actionGradeSummary(null)).toBeUndefined();
    expect(actionGradeSummary(undefined)).toBeUndefined();
  });

  test("an allowed, trusted-backed action carries a grade whose backedBy ids trace to the ledger", () => {
    const storage = memoryStorage();
    const intent = repoIntent(".border-agents/proofs/slice2.patch");
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
    expect(grade!.graded).toBeGreaterThan(0);
    // trusted equals the traceable trail length — the count can never out-claim backedBy.
    expect(grade!.trusted).toBe(grade!.backedBy.length);
    expect(grade!.trusted).toBeGreaterThan(0);
    // No chunk appears twice in the trail: a duplicate would inflate the trusted count and
    // double-list an audit entry, so the count stays a faithful tally of distinct backing grades.
    expect(new Set(grade!.backedBy).size).toBe(grade!.backedBy.length);

    // The projection used the SAME GradeReceipts the snapshot persisted — byte-identical ids.
    const trustedIds = snapshot!.frame.trusted
      .map((m) => snapshot!.frame.receipts.find((r) => r.chunk_id === m.chunk_id)!.receipt_id);
    expect(grade!.backedBy).toEqual(trustedIds);

    // And every backedBy id is a grade the ledger actually persisted (law 6 trace). The two-pass
    // confirm flow writes one memory entry per pass (distinct derivedAt-keyed ids), so the trail
    // is checked against the union of persisted grade receipts, not just one pass's entry.
    const ledger = readReceiptLedger(storage);
    const ledgerTrustedIds = ledger.flatMap((e) =>
      e.kind === "memory" ? e.receipts.filter((r) => r.grade === "trusted").map((r) => r.receipt_id) : [],
    );
    expect(ledgerTrustedIds.length).toBeGreaterThan(0);
    for (const id of grade!.backedBy) expect(ledgerTrustedIds).toContain(id);
  });

  test("a grade-justified block still carries the grade on the wire (distinct from a no-memory block)", () => {
    const storage = memoryStorage();
    const { receipt, result } = handleActionRequest({
      buddy: "forge", effectorId: "repo_edit", settings: ACTING, posture: "work",
      history: ACTION_HISTORY,
      intent: repoIntent("AGENTS.md", "apply_patch"), // protected → hard block, memory still graded
      confirmed: true, storage, now: "2026-06-13T12:00:00Z",
    });
    expect(receipt.decision).toBe("blocked");
    expect(result.grade).toBeDefined();
    expect(result.grade!.graded).toBeGreaterThan(0);
  });

  test("memory off projects no grade even on an allow (the body shows no rail it cannot back)", () => {
    const storage = memoryStorage();
    const { receipt, result } = handleActionRequest({
      buddy: "veritas", effectorId: "receipt_review",
      settings: { ...BASE_SETTINGS, memoryMode: "off" }, posture: "play", // low risk → allow
      history: ACTION_HISTORY, storage, now: "2026-06-13T12:00:00Z",
    });
    expect(receipt.decision).toBe("allow");
    expect(result.grade).toBeUndefined();
  });
});

// The first true `act` effector through the full seam: body request → resolve → gate
// (intent-level) → executor (only on allow) → ledger. Proves the membrane end-to-end,
// not just the core gate — the layer the owl→veritas seam slipped through.
describe("repo_edit through the execution membrane", () => {
  const ACTING: BuddySettings = { ...BASE_SETTINGS, allowAction: true }; // agent_action (high risk)

  function repoIntent(path: string, operation = "write_patch") {
    return {
      effectorId: "repo_edit" as const,
      operation,
      target: { kind: "repo_path" as const, path },
      payloadDigest: "sha256:patch",
      summary: `${operation} ${path}`,
    };
  }

  // Case A — blocked before confirmation: a safe target but no action-backed memory.
  test("Case A: safe target with memory off is blocked (no_action_grant), executor never runs", () => {
    const storage = memoryStorage();
    const { receipt, execution } = handleActionRequest({
      buddy: "forge",
      effectorId: "repo_edit",
      settings: { ...ACTING, memoryMode: "off" },
      posture: "work",
      history: ACTION_HISTORY,
      intent: repoIntent(".border-agents/proofs/first-act.patch"),
      storage,
      now: "2026-06-13T12:00:00Z",
    });
    expect(receipt.decision).toBe("blocked");
    expect(receipt.rules.some((r) => r.policy_rule === "action.blocked.no_action_grant")).toBe(true);
    expect(execution).toBeUndefined();
    expect(readReceiptLedger(storage).some((e) => e.kind === "execution")).toBe(false);
  });

  // Case B — blocked EVEN AFTER confirmation: a protected target is a hard block.
  test("Case B: protected target (AGENTS.md) is blocked even when backed and confirmed; nothing executes", () => {
    const storage = memoryStorage();
    const { receipt, execution } = handleActionRequest({
      buddy: "forge",
      effectorId: "repo_edit",
      settings: ACTING,
      posture: "work",
      history: ACTION_HISTORY,
      intent: repoIntent("AGENTS.md", "apply_patch"),
      confirmed: true,
      storage,
      now: "2026-06-13T12:00:00Z",
    });
    expect(receipt.decision).toBe("blocked");
    expect(receipt.rules.some((r) => r.policy_rule === "action.blocked.protected_target")).toBe(true);
    expect(execution).toBeUndefined();
    expect(readReceiptLedger(storage).some((e) => e.kind === "execution")).toBe(false);
  });

  // Case C — allowed on a safe target, with backing + confirmation; executor runs once,
  // and the ledger records the ActionReceipt BEFORE the ExecutionReceipt.
  test("Case C: safe target needs confirmation, then on confirm the executor runs and an ExecutionReceipt lands after the ActionReceipt", () => {
    const storage = memoryStorage();
    const intent = repoIntent(".border-agents/proofs/first-act.patch");

    const pending = handleActionRequest({
      buddy: "forge",
      effectorId: "repo_edit",
      settings: ACTING,
      posture: "work",
      history: ACTION_HISTORY,
      intent,
      storage,
      now: "2026-06-13T12:00:00Z",
    });
    expect(pending.receipt.decision).toBe("needs_confirmation");
    expect(pending.execution).toBeUndefined();

    const done = handleActionRequest({
      buddy: "forge",
      effectorId: "repo_edit",
      settings: ACTING,
      posture: "work",
      history: ACTION_HISTORY,
      intent,
      confirmed: true,
      storage,
      now: "2026-06-13T12:00:05Z",
    });
    expect(done.receipt.decision).toBe("allow");
    expect(done.execution?.executor_called).toBe(true);
    expect(done.execution?.outcome).toBe("ok");
    // Route provenance rides on the execution receipt — "buddies persist, providers rotate".
    expect(done.execution?.route.provider).toBe("claude"); // forge's preferred route
    expect(done.execution?.action_receipt_id).toBe(done.receipt.receipt_id);

    const ledger = readReceiptLedger(storage);
    const actionIdx = ledger.findIndex((e) => e.kind === "action" && e.recordedAt === "2026-06-13T12:00:05Z");
    const execIdx = ledger.findIndex((e) => e.kind === "execution");
    expect(actionIdx).toBeGreaterThanOrEqual(0);
    expect(execIdx).toBeGreaterThan(actionIdx); // authorized, THEN executed
  });

  // The executor's own sandbox guard is independent of the gate: even if the gate allowed a
  // non-sandbox target, the default executor refuses to act on it (defense in depth).
  test("executor refuses a non-sandbox target even when allowed (skipped, not ok)", () => {
    const storage = memoryStorage();
    const { receipt, execution } = handleActionRequest({
      buddy: "forge",
      effectorId: "repo_edit",
      settings: ACTING,
      posture: "work",
      history: ACTION_HISTORY,
      intent: repoIntent("src/components/widget.tsx"), // not protected, but outside the sandbox
      confirmed: true,
      storage,
      now: "2026-06-13T12:00:00Z",
    });
    expect(receipt.decision).toBe("allow");
    expect(execution?.executor_called).toBe(true);
    expect(execution?.outcome).toBe("skipped");
  });

  // The act path must also resolve persona ids — the same seam that bit receipt_review.
  test("a persona-id (crab) repo_edit resolves to forge for grant + audit identity", () => {
    const storage = memoryStorage();
    const { receipt, result } = handleActionRequest({
      buddy: "crab",
      effectorId: "repo_edit",
      settings: ACTING,
      posture: "work",
      history: ACTION_HISTORY,
      intent: repoIntent(".border-agents/proofs/first-act.patch"),
      storage,
      now: "2026-06-13T12:00:00Z",
    });
    expect(receipt.rules.some((r) => r.policy_rule === "action.blocked.ungranted")).toBe(false);
    expect(receipt.buddy).toBe("forge");
    expect(result.buddy).toBe("crab");
  });
});

describe("parseActionCommand", () => {
  test("/review defaults to receipt_review", () => {
    expect(parseActionCommand("/review")).toEqual({ kind: "review", effectorId: "receipt_review" });
    expect(parseActionCommand("  /review  ")).toEqual({ kind: "review", effectorId: "receipt_review" });
  });

  test("/review <effector> carries the named effector", () => {
    expect(parseActionCommand("/review terminal")).toEqual({ kind: "review", effectorId: "terminal" });
  });

  test("/review <effector> <target> carries a typed target", () => {
    expect(parseActionCommand("/review repo_edit scratch.md")).toEqual({
      kind: "review",
      effectorId: "repo_edit",
      target: "scratch.md",
    });
    expect(parseActionCommand("/review repo_edit .border-agents/proofs/x.patch")).toEqual({
      kind: "review",
      effectorId: "repo_edit",
      target: ".border-agents/proofs/x.patch",
    });
    // No target → no target field (back-compat with the effector-only form).
    expect(parseActionCommand("/review repo_edit")).toEqual({ kind: "review", effectorId: "repo_edit" });
  });

  test("/confirm is the confirm command", () => {
    expect(parseActionCommand("/confirm")).toEqual({ kind: "confirm" });
  });

  test("free text and near-misses are not action commands", () => {
    expect(parseActionCommand("hello there")).toBeNull();
    expect(parseActionCommand("/reviewer")).toBeNull(); // must be /review or "/review "
    expect(parseActionCommand("/confirming")).toBeNull();
    expect(parseActionCommand("")).toBeNull();
  });
});

describe("presenceIntentToActionIntent", () => {
  test("lifts a concrete repo_path intent into a core ActionIntent", () => {
    const intent = presenceIntentToActionIntent("repo_edit", {
      operation: "write_patch",
      target: { kind: "repo_path", value: ".border-agents/proofs/notes.md" },
      summary: "write notes.md",
    });
    expect(intent).toEqual({
      effectorId: "repo_edit",
      operation: "write_patch",
      target: { kind: "repo_path", path: ".border-agents/proofs/notes.md" },
      summary: "write notes.md",
    });
  });

  test("synthesizes a summary when the body omits one", () => {
    const intent = presenceIntentToActionIntent("repo_edit", {
      operation: "write_patch",
      target: { kind: "repo_path", value: "x.md" },
    });
    expect(intent?.summary).toBe("write_patch x.md");
  });

  test("carries an optional payloadDigest through verbatim", () => {
    const intent = presenceIntentToActionIntent("repo_edit", {
      operation: "apply_patch",
      target: { kind: "repo_path", value: "x.md" },
      payloadDigest: "sha256:abc",
    });
    expect(intent?.payloadDigest).toBe("sha256:abc");
  });

  test("a none / value-less / absent target degrades to undefined (grant-only, fails closed)", () => {
    expect(presenceIntentToActionIntent("repo_edit", undefined)).toBeUndefined();
    expect(
      presenceIntentToActionIntent("repo_edit", { operation: "noop", target: { kind: "none" } }),
    ).toBeUndefined();
    expect(
      presenceIntentToActionIntent("repo_edit", { operation: "write_patch", target: { kind: "repo_path" } }),
    ).toBeUndefined();
    expect(presenceIntentToActionIntent("repo_edit", { operation: "write_patch" })).toBeUndefined();
  });
});

describe("decisionEmotion", () => {
  test("each gate decision maps to a distinct, honest face", () => {
    expect(decisionEmotion("allow")).toBe("happy");
    expect(decisionEmotion("needs_confirmation")).toBe("curious");
    expect(decisionEmotion("blocked")).toBe("alert");
    // distinctness — a glance must tell the three border outcomes apart
    const faces = new Set(["allow", "needs_confirmation", "blocked"].map(decisionEmotion));
    expect(faces.size).toBe(3);
  });

  test("any unknown decision fails loud (alert), never a reassuring face", () => {
    expect(decisionEmotion("garbage")).toBe("alert");
    expect(decisionEmotion("")).toBe("alert");
  });

  test("stays in lockstep with the native body's for_decision twin", () => {
    // These pairings ARE the cross-surface contract (desktop-body render.rs Emotion::for_decision):
    // if either side drifts, the body and soul would disagree on what the gate just did.
    expect(decisionEmotion("allow")).toBe("happy"); //              → Emotion::Happy
    expect(decisionEmotion("needs_confirmation")).toBe("curious"); // → Emotion::Curious
    expect(decisionEmotion("blocked")).toBe("alert"); //            → Emotion::Alert
  });
});

describe("decisionAlertLevel", () => {
  test("each gate decision maps to its passport/ring alert tier", () => {
    expect(decisionAlertLevel("allow")).toBe("ready");
    expect(decisionAlertLevel("needs_confirmation")).toBe("confirm");
    expect(decisionAlertLevel("blocked")).toBe("blocked");
  });

  test("any unknown decision fails loud at the top tier (critical), never quiet", () => {
    expect(decisionAlertLevel("garbage")).toBe("critical");
    expect(decisionAlertLevel("")).toBe("critical");
  });

  test("face and chrome derive from one decision (Law 7 — body never infers policy)", () => {
    // The emotion twin colours the face; the alert twin colours the passport/ring. They are
    // sent together on action_result so the body reads chrome from a cue, not from the face.
    expect([decisionEmotion("allow"), decisionAlertLevel("allow")]).toEqual(["happy", "ready"]);
    expect([decisionEmotion("needs_confirmation"), decisionAlertLevel("needs_confirmation")]).toEqual([
      "curious",
      "confirm",
    ]);
    expect([decisionEmotion("blocked"), decisionAlertLevel("blocked")]).toEqual(["alert", "blocked"]);
  });
});

describe("routeHealthFromSoul", () => {
  test("maps the ready path from real route state", () => {
    expect(
      routeHealthFromSoul({
        hasRoute: true,
        availability: "available",
        downgraded: false,
      }),
    ).toBe("ready");
  });

  test("treats gated as reachable route infrastructure", () => {
    expect(
      routeHealthFromSoul({
        hasRoute: true,
        availability: "gated",
        downgraded: false,
        lastOutcome: "ok",
      }),
    ).toBe("ready");
  });

  test("marks missing or unwired routes unavailable", () => {
    expect(
      routeHealthFromSoul({
        hasRoute: false,
        availability: "available",
        downgraded: false,
      }),
    ).toBe("unavailable");
    expect(
      routeHealthFromSoul({
        hasRoute: true,
        availability: "unwired",
        downgraded: false,
      }),
    ).toBe("unavailable");
  });

  test("marks downgraded routes and degraded outcomes degraded", () => {
    expect(
      routeHealthFromSoul({
        hasRoute: true,
        availability: "available",
        downgraded: true,
      }),
    ).toBe("degraded");
    expect(
      routeHealthFromSoul({
        hasRoute: true,
        availability: "available",
        downgraded: false,
        lastOutcome: "degraded",
      }),
    ).toBe("degraded");
  });

  test("uses most-severe-wins precedence", () => {
    expect(
      routeHealthFromSoul({
        hasRoute: true,
        availability: "unwired",
        downgraded: true,
        lastOutcome: "degraded",
      }),
    ).toBe("unavailable");
    expect(
      routeHealthFromSoul({
        hasRoute: true,
        availability: "available",
        downgraded: true,
        lastOutcome: "failed",
      }),
    ).toBe("unavailable");
  });
});
