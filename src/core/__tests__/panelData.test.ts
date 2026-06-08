import { describe, expect, test } from "vitest";
import { runHermesMemoryDemo } from "../demo/hermesMemoryDemo";
import { buildNexusPanelData, buildVeritasPanelData } from "../panelData";

describe("Trust Workbench panel data", () => {
  test("builds NexusPanelData from frame and prompt summaries", () => {
    const agentAction = runHermesMemoryDemo("2026-06-07T12:00:00Z").find(
      (result) => result.purpose === "agent_action",
    );

    expect(agentAction).toBeDefined();

    const nexus = buildNexusPanelData({ frame: agentAction!.frame, prompt: agentAction!.prompt });

    expect(nexus).toMatchObject({
      purpose: "agent_action",
      retrievedCount: 7,
      frameBuckets: {
        trusted: 2,
        limited: 0,
        reference_only: 1,
        blocked: 3,
        quarantined: 1,
      },
      promptSummary: {
        included: 2,
        excluded: 5,
      },
      trustBadgeState: "quarantined",
    });
    expect(nexus.topSources).toContainEqual({
      sourceId: "hermes/review_required",
      sourceType: "chat_session",
      count: 1,
      highestGrade: "quarantined",
    });
  });

  test("builds VeritasPanelData warnings and evidence-ready items", () => {
    const externalShare = runHermesMemoryDemo("2026-06-07T12:00:00Z").find(
      (result) => result.purpose === "external_share",
    );

    expect(externalShare).toBeDefined();

    const veritas = buildVeritasPanelData({ frame: externalShare!.frame, prompt: externalShare!.prompt });

    expect(veritas.purpose).toBe("external_share");
    expect(veritas.receiptGroups.trusted).toHaveLength(1);
    expect(veritas.receiptGroups.blocked).toHaveLength(5);
    expect(veritas.receiptGroups.quarantined).toHaveLength(1);
    expect(veritas.warnings).toHaveLength(6);
    expect(veritas.warnings[0]).toMatchObject({
      chunkId: "chunk_user_profile",
      grade: "blocked",
      promptStatus: "excluded",
      finalReason: "packet sensitivity is not permitted for this purpose",
      policyRules: ["purpose.allow_sensitive", "grade.blocked.sensitivity"],
      warningType: "blocked",
    });
    expect(veritas.evidenceReady).toHaveLength(1);
    expect(veritas.evidenceReady[0]).toMatchObject({
        chunkId: "chunk_external_note",
        packetId: "mem_pkt_external_note",
        grade: "trusted",
        promptStatus: "included",
        promptReason: null,
        finalReason: "packet is authorized for the active purpose",
        policyRules: ["grade.trusted.permissions", "grade.trusted"],
        sourceId: "hermes/external_note",
        sourceType: "chat_session",
      });
    expect(veritas.evidenceReady[0].receiptId).toBe(
      "grade:external_share:chunk_external_note:2026-06-07T12:00:00Z",
    );
    expect(veritas.evidenceReady[0].ruleDetails).toHaveLength(2);
  });

  test("marks constrained prompt entries as evidence ready for annotated policy display", () => {
    const currentPolicy = runHermesMemoryDemo("2026-06-07T12:00:00Z").find(
      (result) => result.purpose === "answer_current_policy",
    );

    expect(currentPolicy).toBeDefined();

    const veritas = buildVeritasPanelData({ frame: currentPolicy!.frame, prompt: currentPolicy!.prompt });
    const constrained = veritas.evidenceReady.find((item) => item.chunkId === "chunk_unverified_preference");

    expect(constrained).toMatchObject({
      grade: "limited",
      promptStatus: "included",
      finalReason: "packet is relevant but constrained",
      policyRules: ["permissions.requires_verification_before_assertion", "grade.limited.constraints"],
    });
  });
});
