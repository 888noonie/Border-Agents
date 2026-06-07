import { describe, expect, test } from "vitest";
import { hermesRetrievedMemories, runHermesMemoryDemo } from "../demo/hermesMemoryDemo";
import { formatHermesMemoryDemo } from "../demo/hermesMemoryDemoFormatter";

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

  test("CLI formatter shows all purposes, grades, and receipt rules", () => {
    const output = formatHermesMemoryDemo(runHermesMemoryDemo("2026-06-07T12:00:00Z"));

    expect(output).toContain("Border Agents Hermes Memory Demo");
    expect(output).toContain("Purpose: summarize_history");
    expect(output).toContain("Purpose: answer_current_policy");
    expect(output).toContain("Purpose: agent_action");
    expect(output).toContain("Purpose: external_share");
    expect(output).toContain("trusted:");
    expect(output).toContain("limited:");
    expect(output).toContain("reference_only:");
    expect(output).toContain("blocked:");
    expect(output).toContain("quarantined:");
    expect(output).toContain("receipts:");
    expect(output).toContain("grade.reference_only.expired");
    expect(output).toContain("grade.blocked.required_permission");
  });
});
