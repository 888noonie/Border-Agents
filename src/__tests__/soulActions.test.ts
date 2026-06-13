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

describe("parseActionCommand", () => {
  test("/review defaults to receipt_review", () => {
    expect(parseActionCommand("/review")).toEqual({ kind: "review", effectorId: "receipt_review" });
    expect(parseActionCommand("  /review  ")).toEqual({ kind: "review", effectorId: "receipt_review" });
  });

  test("/review <effector> carries the named effector", () => {
    expect(parseActionCommand("/review terminal")).toEqual({ kind: "review", effectorId: "terminal" });
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
