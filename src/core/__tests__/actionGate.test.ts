import { describe, expect, test } from "vitest";
import { authorizeEffectorAction } from "../actionGate";
import { emptyFrame } from "../grader";
import type { MemoryPacket, RetrievedMemory, SafeContextFrame } from "../types";
import { EFFECTOR_SPECS, type EffectorSpec } from "../../buddyManifest";
import { USER_POSTURES } from "../userPosture";

const NOW = "2026-06-13T12:00:00Z";

// receipt_review is the live, read-only `reach` effector.
const REVIEW = EFFECTOR_SPECS.receipt_review;
// terminal is a real `act` spec but ships unwired; fabricate a wired copy so we can
// exercise the act-kind path of the gate without flipping the manifest.
const WIRED_ACT: EffectorSpec = { ...EFFECTOR_SPECS.terminal, wired: true };
const UNWIRED_ACT = EFFECTOR_SPECS.terminal; // wired: false

function backedFrame(): SafeContextFrame {
  const frame = emptyFrame("agent_action");
  frame.trusted.push(actionMemory());
  return frame;
}

describe("authorizeEffectorAction", () => {
  test("ungranted effector is blocked, and confirmation cannot override it", () => {
    const blocked = authorizeEffectorAction({
      buddy: "veritas",
      effector: REVIEW,
      granted: false,
      posture: "play",
      purpose: "summarize_history",
      frame: backedFrame(),
      now: NOW,
    });
    expect(blocked.decision).toBe("blocked");
    expect(blocked.rules.some((r) => r.policy_rule === "action.blocked.ungranted")).toBe(true);

    const stillBlocked = authorizeEffectorAction({
      buddy: "veritas",
      effector: REVIEW,
      granted: false,
      posture: "play",
      purpose: "summarize_history",
      frame: backedFrame(),
      confirmed: true,
      now: NOW,
    });
    expect(stillBlocked.decision).toBe("blocked");
  });

  test("unwired effector is blocked", () => {
    const receipt = authorizeEffectorAction({
      buddy: "forge",
      effector: UNWIRED_ACT,
      granted: true,
      posture: "work",
      purpose: "agent_action",
      frame: backedFrame(),
      now: NOW,
    });
    expect(receipt.decision).toBe("blocked");
    expect(receipt.rules.some((r) => r.policy_rule === "action.blocked.unwired")).toBe(true);
  });

  test("act effector without trusted action-backed memory is blocked", () => {
    const receipt = authorizeEffectorAction({
      buddy: "forge",
      effector: WIRED_ACT,
      granted: true,
      posture: "work",
      purpose: "summarize_history", // low-risk purpose; act kind still demands backing
      frame: emptyFrame("summarize_history"),
      now: NOW,
    });
    expect(receipt.decision).toBe("blocked");
    expect(receipt.rules.some((r) => r.policy_rule === "action.blocked.no_action_grant")).toBe(true);
  });

  test("high-risk purpose without action-backed memory is blocked even for a reach effector", () => {
    const receipt = authorizeEffectorAction({
      buddy: "veritas",
      effector: REVIEW,
      granted: true,
      posture: "work",
      purpose: "agent_action", // high risk
      frame: emptyFrame("agent_action"),
      now: NOW,
    });
    expect(receipt.decision).toBe("blocked");
    expect(receipt.rules.some((r) => r.policy_rule === "action.blocked.no_action_grant")).toBe(true);
  });

  test("medium-risk action needs confirmation in every posture", () => {
    for (const posture of USER_POSTURES) {
      const receipt = authorizeEffectorAction({
        buddy: "veritas",
        effector: REVIEW,
        granted: true,
        posture,
        purpose: "answer_current_policy", // medium risk
        frame: emptyFrame("answer_current_policy"),
        now: NOW,
      });
      expect(receipt.decision, `medium risk must confirm under ${posture}`).toBe("needs_confirmation");
      expect(receipt.rules.some((r) => r.policy_rule === "action.confirm.risk_floor")).toBe(true);
    }
  });

  test("confirmation turns needs_confirmation into an allow with an override step", () => {
    const receipt = authorizeEffectorAction({
      buddy: "veritas",
      effector: REVIEW,
      granted: true,
      posture: "work",
      purpose: "answer_current_policy",
      frame: emptyFrame("answer_current_policy"),
      confirmed: true,
      now: NOW,
    });
    expect(receipt.decision).toBe("allow");
    expect(receipt.confirmed).toBe(true);
    expect(receipt.rules.some((r) => r.policy_rule === "action.allow.confirmed")).toBe(true);
  });

  test("low-risk reach: confirmed under work/private, waived under play", () => {
    const base = {
      buddy: "veritas",
      effector: REVIEW,
      granted: true,
      purpose: "summarize_history" as const, // low risk
      frame: emptyFrame("summarize_history"),
      now: NOW,
    };
    expect(authorizeEffectorAction({ ...base, posture: "play" }).decision).toBe("allow");
    expect(authorizeEffectorAction({ ...base, posture: "work" }).decision).toBe("needs_confirmation");
    expect(authorizeEffectorAction({ ...base, posture: "private" }).decision).toBe("needs_confirmation");
  });

  test("is deterministic — same inputs and now produce an identical receipt", () => {
    const args = {
      buddy: "veritas",
      effector: REVIEW,
      granted: true,
      posture: "work" as const,
      purpose: "answer_current_policy" as const,
      frame: emptyFrame("answer_current_policy"),
      now: NOW,
    };
    expect(authorizeEffectorAction(args)).toEqual(authorizeEffectorAction(args));
  });

  test("every receipt carries a stable id and a non-empty derivation trail", () => {
    const receipt = authorizeEffectorAction({
      buddy: "veritas",
      effector: REVIEW,
      granted: true,
      posture: "work",
      purpose: "summarize_history",
      frame: emptyFrame("summarize_history"),
      now: NOW,
    });
    expect(receipt.receipt_id).toBe(`action:veritas:receipt_review:${NOW}`);
    expect(receipt.rules.length).toBeGreaterThan(0);
  });
});

function actionMemory(): RetrievedMemory {
  const packet: MemoryPacket = {
    packet_id: "mem_pkt_action",
    content_hash: "sha256:action",
    source: { type: "user_note", id: "test/action", created_at: "2026-06-01T00:00:00Z" },
    claim_type: "instruction",
    authority: "high",
    sensitivity: "internal",
    valid_until: null,
    permissions: {
      may_retrieve: true,
      may_quote: true,
      may_assert: true,
      may_use_for_action: true,
      requires_verification_before_assertion: false,
    },
    labels: [],
    policy: { id: "test-policy", version: "0.1.0" },
    derivation: [],
    review: { mode: "strict", requires_review: false, reviewed_by: null, reviewed_at: null },
  };
  return { chunk_id: "chunk_action", text: "act on this", score: 1, packet };
}
