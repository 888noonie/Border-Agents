import { describe, expect, test } from "vitest";
import { handleActionRequest, parseActionCommand } from "../soulActions";
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
    const ledger = readReceiptLedger(storage);
    expect(ledger).toHaveLength(1);
    expect(ledger[0].kind).toBe("action");
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

    expect(readReceiptLedger(storage)).toHaveLength(2);
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
    const ledger = readReceiptLedger(storage);
    expect(ledger).toHaveLength(1);
    expect(ledger[0].buddyId).toBe("veritas");
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
