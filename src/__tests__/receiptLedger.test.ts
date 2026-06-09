// @vitest-environment jsdom

import { beforeEach, describe, expect, test } from "vitest";
import { buildBuddyGovernanceSnapshot } from "../liveGovernance";
import {
  appendSnapshotToReceiptLedger,
  readReceiptLedger,
  summarizeReceiptLedger,
} from "../receiptLedger";
import type { BuddySettings } from "../buddyProfiles";

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
    expect(latestOnly[0].purpose).toBe("summarize_history");
    expect(latestOnly[0].receipts[0].chunk_id).toBe("chunk:hermes:0");
    expect(latestOnly[0].recordedAt).toBe("2026-06-09T12:01:00Z");
  });
});
