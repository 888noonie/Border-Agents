import { describe, expect, test } from "vitest";
import { buildBuddyGovernanceSnapshot, buildGovernanceBuddyMessages } from "../liveGovernance";
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

describe("liveGovernance", () => {
  test("builds a summarize-history frame from live session history", () => {
    const snapshot = buildBuddyGovernanceSnapshot({
      buddyId: "hermes",
      history: [
        { role: "user", text: "Summarize the last changes." },
        { role: "assistant", text: "You updated the governance report and cleaned the markdown." },
      ],
      settings: BASE_SETTINGS,
      now: "2026-06-09T12:00:00Z",
    });

    expect(snapshot?.purpose).toBe("summarize_history");
    expect(snapshot?.retrieved).toHaveLength(2);
    expect(snapshot?.frame.trusted).toHaveLength(1);
    expect(snapshot?.frame.limited).toHaveLength(1);
    expect(snapshot?.prompt.included).toHaveLength(2);
  });

  test("promotes action-enabled sessions to agent_action purpose", () => {
    const snapshot = buildBuddyGovernanceSnapshot({
      buddyId: "hermes",
      history: [{ role: "assistant", text: "Apply the patch after review." }],
      settings: {
        ...BASE_SETTINGS,
        allowAction: true,
      },
      now: "2026-06-09T12:00:00Z",
    });

    expect(snapshot?.purpose).toBe("agent_action");
    expect(snapshot?.frame.trusted).toHaveLength(1);
    expect(snapshot?.prompt.included).toHaveLength(1);
  });

  test("promotes external-share sessions to external_share purpose and excludes internal user text", () => {
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

    expect(snapshot?.purpose).toBe("external_share");
    expect(snapshot?.frame.trusted).toHaveLength(1);
    expect(snapshot?.frame.quarantined).toHaveLength(0);
    expect(snapshot?.frame.blocked).toHaveLength(1);
    expect(snapshot?.prompt.included).toEqual([
      expect.objectContaining({
        grade: "trusted",
      }),
    ]);
    expect(snapshot?.prompt.context).toContain("Border Agents is a deterministic governance layer for AI work.");
    expect(snapshot?.prompt.context).not.toContain("Share the draft with the partner.");
  });

  test("returns null when memory mode is off", () => {
    const snapshot = buildBuddyGovernanceSnapshot({
      buddyId: "hermes",
      history: [{ role: "assistant", text: "This should not be graded." }],
      settings: {
        ...BASE_SETTINGS,
        memoryMode: "off",
      },
    });

    expect(snapshot).toBeNull();
  });

  test("derives dock buddy status copy from a live governance snapshot", () => {
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

    const messages = buildGovernanceBuddyMessages(snapshot!);

    expect(messages.crab).toBe("1 trusted · 0 limited · 1 held back.");
    expect(messages.owl).toBe("1 receipt warnings need review.");
    expect(messages.fox).toBe("Share context: 1 in · 1 out.");
  });
});
