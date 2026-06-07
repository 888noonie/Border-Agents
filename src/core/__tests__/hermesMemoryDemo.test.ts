import { describe, expect, test } from "vitest";
import { hermesRetrievedMemories, runHermesMemoryDemo } from "../demo/hermesMemoryDemo";

describe("Hermes memory demo", () => {
  test("the same retrieved chunks produce different frames for different purposes", () => {
    const results = runHermesMemoryDemo("2026-06-07T12:00:00Z");
    const chunkSets = results.map((result) =>
      result.frame.receipts
        .map((receipt) => receipt.chunk_id)
        .sort()
        .join(","),
    );

    expect(new Set(chunkSets).size).toBe(1);

    const summaries = results.map((result) => ({
      purpose: result.purpose,
      trusted: result.frame.trusted.length,
      limited: result.frame.limited.length,
      referenceOnly: result.frame.reference_only.length,
      blocked: result.frame.blocked.length,
      quarantined: result.frame.quarantined.length,
    }));

    expect(new Set(summaries.map((summary) => JSON.stringify(summary))).size).toBeGreaterThan(1);
    expect(results.every((result) => result.frame.receipts.length === hermesRetrievedMemories.length)).toBe(true);
  });
});
