import { describe, expect, test } from "vitest";
import { runHermesMemoryDemo } from "../demo/hermesMemoryDemo";
import { buildTrace, formatTrace } from "../governanceTrace";

describe("governance trace reporter", () => {
  test("builds a compact trace report for a purpose", () => {
    const agentAction = runHermesMemoryDemo("2026-06-07T12:00:00Z").find(
      (result) => result.purpose === "agent_action",
    );

    expect(agentAction).toBeDefined();

    const report = buildTrace({ frame: agentAction!.frame, prompt: agentAction!.prompt });
    const blockedAction = report.traceLines.find((line) => line.chunkId === "chunk_active_goal");

    expect(report).toMatchObject({
      purpose: "agent_action",
      retrievedCount: 7,
      frame: {
        trusted: 2,
        limited: 0,
        reference_only: 1,
        blocked: 3,
        quarantined: 1,
      },
      prompt: {
        included: 2,
        excluded: 5,
      },
    });
    expect(report.traceLines).toHaveLength(7);
    expect(blockedAction).toMatchObject({
      grade: "blocked",
      promptStatus: "excluded",
      reason: "packet lacks a required permission",
      rules: ["purpose.require_permissions", "grade.blocked.required_permission"],
    });
  });

  test("formats the trace without packet text dumps", () => {
    const externalShare = runHermesMemoryDemo("2026-06-07T12:00:00Z").find(
      (result) => result.purpose === "external_share",
    );

    expect(externalShare).toBeDefined();

    const output = formatTrace(buildTrace({ frame: externalShare!.frame, prompt: externalShare!.prompt }));

    expect(output).toContain("Purpose: external_share");
    expect(output).toContain("Retrieved: 7");
    expect(output).toContain("Frame: trusted=1 limited=0 reference_only=0 blocked=5 quarantined=1");
    expect(output).toContain("Prompt: included=1 excluded=6");
    expect(output).toContain("- chunk_user_profile");
    expect(output).toContain("grade: blocked");
    expect(output).toContain("reason: packet sensitivity is not permitted for this purpose");
    expect(output).toContain("rules: purpose.allow_sensitive, grade.blocked.sensitivity");
    expect(output).not.toContain("text:");
    expect(output).not.toContain("User profile: name alias is Richard");
  });
});
