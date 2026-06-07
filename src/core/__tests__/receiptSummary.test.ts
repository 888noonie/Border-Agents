import { describe, expect, test } from "vitest";
import { runHermesMemoryDemo } from "../demo/hermesMemoryDemo";
import { summarizeFrameReceipts } from "../receiptSummary";

describe("receipt summary view model", () => {
  test("groups receipt summaries by grade with counts and final reasons", () => {
    const agentAction = runHermesMemoryDemo("2026-06-07T12:00:00Z").find(
      (result) => result.purpose === "agent_action",
    );

    expect(agentAction).toBeDefined();

    const summary = summarizeFrameReceipts(agentAction!.frame, agentAction!.prompt);

    expect(summary).toMatchObject({
      purpose: "agent_action",
      counts: {
        trusted: 2,
        limited: 0,
        reference_only: 1,
        blocked: 3,
        quarantined: 1,
      },
      total_retrieved: 7,
      total_receipts: 7,
      prompt_included: 2,
      prompt_excluded: 5,
    });
    expect(summary.by_grade.blocked).toHaveLength(3);
    expect(summary.by_grade.quarantined[0]).toMatchObject({
      chunk_id: "chunk_review_required",
      final_reason: "packet is held for review",
      policy_rules: ["packet.review", "grade.quarantined.review_required"],
      prompt_status: "excluded",
    });
  });

  test("marks trusted prompt entries as included and constrained entries as excluded in strict mode", () => {
    const externalShare = runHermesMemoryDemo("2026-06-07T12:00:00Z").find(
      (result) => result.purpose === "external_share",
    );

    expect(externalShare).toBeDefined();

    const summary = summarizeFrameReceipts(externalShare!.frame, externalShare!.prompt);
    const trusted = summary.items.find((item) => item.chunk_id === "chunk_external_note");
    const blocked = summary.items.find((item) => item.chunk_id === "chunk_user_profile");

    expect(trusted).toMatchObject({
      grade: "trusted",
      final_reason: "packet is authorized for the active purpose",
      prompt_status: "included",
      prompt_reason: null,
      source_type: "chat_session",
      source_id: "hermes/external_note",
    });
    expect(blocked).toMatchObject({
      grade: "blocked",
      final_reason: "packet sensitivity is not permitted for this purpose",
      prompt_status: "excluded",
      prompt_reason: "blocked content is excluded by strict mode",
    });
  });

  test("uses unknown prompt status when no prompt render result is provided", () => {
    const summarizeHistory = runHermesMemoryDemo("2026-06-07T12:00:00Z")[0];
    const summary = summarizeFrameReceipts(summarizeHistory.frame);

    expect(summary.prompt_included).toBe(0);
    expect(summary.prompt_excluded).toBe(0);
    expect(summary.items.every((item) => item.prompt_status === "unknown")).toBe(true);
  });
});
