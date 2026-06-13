import { describe, expect, test } from "vitest";
import { buildGovernanceSurfaceCopy } from "../governanceSurfaceCopy";
import type { ReceiptLedgerSummary } from "../receiptLedger";

// A memory-only ledger summary — no action has been gated yet.
const MEMORY_ONLY: ReceiptLedgerSummary = {
  entryCount: 2,
  frameCount: 2,
  receiptCount: 5,
  latestBuddyId: "hermes",
  latestPurpose: "summarize_history",
  latestRecordedAt: "2026-06-13T11:00:00Z",
  latestWarnings: 0,
  latestPromptIncluded: 3,
  latestPromptExcluded: 1,
  actionCount: 0,
  latestActionDecision: null,
  executionCount: 0,
  latestExecutionOutcome: null,
};

// A ledger that has seen the action membrane: authorizations + one execution.
const WITH_ACTIONS: ReceiptLedgerSummary = {
  ...MEMORY_ONLY,
  entryCount: 5, // 2 frames + 2 action receipts + 1 execution receipt
  actionCount: 2,
  latestActionDecision: "needs_confirmation",
  executionCount: 1,
  latestExecutionOutcome: "ok",
};

describe("governance surface copy — action membrane in the ledger ticker", () => {
  test("stays quiet about actions on a memory-only session", () => {
    const copy = buildGovernanceSurfaceCopy({ snapshot: null, ledgerSummary: MEMORY_ONLY });
    expect(copy.tickerMessages.some((m) => m.startsWith("Actions:"))).toBe(false);
    // The frame count drives "frames", not the union entryCount.
    expect(copy.tickerMessages.some((m) => m.includes("2 frames and 5 receipts"))).toBe(true);
  });

  test("surfaces gated actions, latest decision, and executions once the membrane has run", () => {
    const copy = buildGovernanceSurfaceCopy({ snapshot: null, ledgerSummary: WITH_ACTIONS });
    const actionLine = copy.tickerMessages.find((m) => m.startsWith("Actions:"));
    expect(actionLine).toBeDefined();
    expect(actionLine).toContain("2 gated");
    expect(actionLine).toContain("latest needs confirmation"); // underscores humanized
    expect(actionLine).toContain("1 executed (ok)");
  });

  test("counts frames honestly when action/execution entries inflate entryCount", () => {
    // entryCount is 5 but only 2 are graded frames — the copy must not call all 5 "frames".
    const copy = buildGovernanceSurfaceCopy({ snapshot: null, ledgerSummary: WITH_ACTIONS });
    expect(copy.tickerMessages.some((m) => m.includes("2 frames"))).toBe(true);
    expect(copy.tickerMessages.some((m) => m.includes("5 frames"))).toBe(false);
    expect(copy.passThroughMessage).toContain("2 graded frames");
  });
});
