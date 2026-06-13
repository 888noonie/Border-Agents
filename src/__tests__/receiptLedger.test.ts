// @vitest-environment jsdom

import { beforeEach, describe, expect, test } from "vitest";
import { buildBuddyGovernanceSnapshot } from "../liveGovernance";
import {
  appendActionReceiptToLedger,
  appendSnapshotToReceiptLedger,
  isActionEntry,
  isMemoryEntry,
  readReceiptLedger,
  RECEIPT_LEDGER_STORAGE_KEY,
  summarizeReceiptLedger,
} from "../receiptLedger";
import type { ActionReceipt } from "../core";
import type { BuddySettings } from "../buddyProfiles";

const ACTION_RECEIPT: ActionReceipt = {
  receipt_id: "action:veritas:receipt_review:2026-06-13T12:00:00Z",
  effector: "receipt_review",
  buddy: "veritas",
  decision: "allow",
  risk: "low",
  posture: "play",
  confirmed: false,
  derived_at: "2026-06-13T12:00:00Z",
  rules: [{ field: "risk", value: "low", source: "user_posture:play", reason: "ok", policy_rule: "action.allow" }],
};

const BASE_SETTINGS: BuddySettings = {
  enabled: true,
  provider: "grok",
  modelLabel: "Grok subscription",
  connectionLabel: "Connected",
  allowAction: false,
  allowExternalShare: false,
  memoryMode: "purpose_graded",
};

describe("receiptLedger", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  test("persists a live snapshot and summarizes the latest receipt frame", () => {
    const snapshot = buildBuddyGovernanceSnapshot({
      buddyId: "hermes",
      history: [
        { role: "user", text: "Share the draft with the partner." },
        { role: "assistant", text: "Border Agents is a deterministic governance layer for AI work." },
      ],
      settings: {
        ...BASE_SETTINGS,
        allowExternalShare: true,
      },
      now: "2026-06-09T12:00:00Z",
    });

    expect(snapshot).not.toBeNull();

    const entries = appendSnapshotToReceiptLedger({
      buddyId: "hermes",
      snapshot: snapshot!,
      storage: localStorage,
    });

    expect(entries).toHaveLength(1);
    expect(readReceiptLedger(localStorage)).toHaveLength(1);

    const summary = summarizeReceiptLedger(entries);

    expect(summary.entryCount).toBe(1);
    expect(summary.receiptCount).toBe(2);
    expect(summary.latestBuddyId).toBe("hermes");
    expect(summary.latestPurpose).toBe("external_share");
    expect(summary.latestWarnings).toBe(1);
    expect(summary.latestPromptIncluded).toBe(1);
    expect(summary.latestPromptExcluded).toBe(1);
  });

  test("dedupes identical snapshot writes and caps the ledger length", () => {
    const first = buildBuddyGovernanceSnapshot({
      buddyId: "hermes",
      history: [{ role: "assistant", text: "First frame." }],
      settings: BASE_SETTINGS,
      now: "2026-06-09T12:00:00Z",
    });
    const second = buildBuddyGovernanceSnapshot({
      buddyId: "hermes",
      history: [{ role: "assistant", text: "Second frame." }],
      settings: BASE_SETTINGS,
      now: "2026-06-09T12:01:00Z",
    });

    appendSnapshotToReceiptLedger({
      buddyId: "hermes",
      snapshot: first!,
      storage: localStorage,
      maxEntries: 1,
    });
    appendSnapshotToReceiptLedger({
      buddyId: "hermes",
      snapshot: first!,
      storage: localStorage,
      maxEntries: 1,
    });
    const latestOnly = appendSnapshotToReceiptLedger({
      buddyId: "hermes",
      snapshot: second!,
      storage: localStorage,
      maxEntries: 1,
    });

    expect(latestOnly).toHaveLength(1);
    const entry = latestOnly[0];
    expect(isMemoryEntry(entry)).toBe(true);
    if (isMemoryEntry(entry)) {
      expect(entry.purpose).toBe("summarize_history");
      expect(entry.receipts[0].chunk_id).toBe("chunk:hermes:0");
    }
    expect(entry.recordedAt).toBe("2026-06-09T12:01:00Z");
  });

  test("stores action receipts alongside memory entries in one ledger", () => {
    const memorySnapshot = buildBuddyGovernanceSnapshot({
      buddyId: "hermes",
      history: [{ role: "assistant", text: "A memory frame." }],
      settings: BASE_SETTINGS,
      now: "2026-06-13T11:00:00Z",
    });
    appendSnapshotToReceiptLedger({ buddyId: "hermes", snapshot: memorySnapshot!, storage: localStorage });
    const entries = appendActionReceiptToLedger({ buddyId: "veritas", receipt: ACTION_RECEIPT, storage: localStorage });

    expect(entries).toHaveLength(2);
    const action = entries.find(isActionEntry)!;
    expect(action.effector).toBe("receipt_review");
    expect(action.decision).toBe("allow");

    const summary = summarizeReceiptLedger(entries);
    expect(summary.entryCount).toBe(2);
    expect(summary.frameCount).toBe(1); // only the memory entry is a graded frame
    expect(summary.receiptCount).toBe(2); // 1 memory receipt + 1 action receipt
    expect(summary.actionCount).toBe(1);
    expect(summary.latestActionDecision).toBe("allow");
    expect(summary.latestPurpose).toBeNull(); // latest entry is an action, not a memory frame
  });

  test("dedupes identical action receipts by entry id", () => {
    appendActionReceiptToLedger({ buddyId: "veritas", receipt: ACTION_RECEIPT, storage: localStorage });
    const again = appendActionReceiptToLedger({ buddyId: "veritas", receipt: ACTION_RECEIPT, storage: localStorage });
    expect(again).toHaveLength(1);
  });

  test("reads a legacy entry without a kind field as a memory entry", () => {
    // A pre-action-receipt ledger entry — no `kind` discriminator.
    const legacy = {
      entryId: "hermes:summarize_history:grade:summarize_history:chunk:hermes:0:t",
      buddyId: "hermes",
      purpose: "summarize_history",
      recordedAt: "2026-06-01T00:00:00Z",
      frameCounts: { trusted: 1, limited: 0, reference_only: 0, blocked: 0, quarantined: 0 },
      promptIncluded: 1,
      promptExcluded: 0,
      receipts: [],
    };
    localStorage.setItem(RECEIPT_LEDGER_STORAGE_KEY, JSON.stringify([legacy]));

    const entries = readReceiptLedger(localStorage);
    expect(entries).toHaveLength(1);
    expect(isMemoryEntry(entries[0])).toBe(true);
    expect(isActionEntry(entries[0])).toBe(false);
    expect(summarizeReceiptLedger(entries).latestPurpose).toBe("summarize_history");
  });
});
