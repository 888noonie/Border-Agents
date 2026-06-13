import { describe, expect, test } from "vitest";
import { authorizeEffectorAction, isProtectedTarget, type ActionIntent, type ActionRoute } from "../actionGate";
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
// repo_edit is the first true `act` effector going behind the membrane; fabricate a
// wired copy so the gate exercises intent-level authorization without flipping the manifest.
const WIRED_REPO_EDIT: EffectorSpec = { ...EFFECTOR_SPECS.repo_edit, wired: true };

function repoIntent(path: string, operation = "write_patch"): ActionIntent {
  return {
    effectorId: "repo_edit",
    operation,
    target: { kind: "repo_path", path },
    payloadDigest: "sha256:patch",
    summary: `${operation} ${path}`,
  };
}

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

// The execution membrane: the gate authorizes the EFFECT (intent + target), not just
// the effector. This is the proof that converts "very good demo" into "infrastructure".
describe("execution membrane — intent-level authorization", () => {
  const base = {
    buddy: "forge",
    effector: WIRED_REPO_EDIT,
    granted: true,
    posture: "work" as const,
    purpose: "agent_action" as const, // high risk — repo_edit is real code change
    now: NOW,
  };

  test("isProtectedTarget guards the laws, the gate, version control, and deps", () => {
    expect(isProtectedTarget(repoIntent("AGENTS.md"))).toBe(true);
    expect(isProtectedTarget(repoIntent("src/core/actionGate.ts"))).toBe(true);
    expect(isProtectedTarget(repoIntent("./src/core/grader.ts"))).toBe(true);
    expect(isProtectedTarget(repoIntent("src/core"))).toBe(true); // the dir itself
    expect(isProtectedTarget(repoIntent("package.json"))).toBe(true);
    expect(isProtectedTarget(repoIntent(".border-agents/proofs/first-act.patch"))).toBe(false);
    expect(isProtectedTarget(repoIntent("src/components/foo.tsx"))).toBe(false);
  });

  test("isProtectedTarget resolves `..` so traversal cannot disguise a protected target", () => {
    // A naive prefix match would miss all of these — canonicalization catches them.
    expect(isProtectedTarget(repoIntent("src/foo/../../AGENTS.md"))).toBe(true);
    expect(isProtectedTarget(repoIntent("docs/../src/core/grader.ts"))).toBe(true);
    expect(isProtectedTarget(repoIntent(".border-agents/proofs/../../package.json"))).toBe(true);
    // Any path that climbs out of the repo root is never writable.
    expect(isProtectedTarget(repoIntent("../../etc/passwd"))).toBe(true);
    expect(isProtectedTarget(repoIntent(".border-agents/proofs/../../../home/x"))).toBe(true);
    // A `..` that stays inside and resolves to a safe path is fine.
    expect(isProtectedTarget(repoIntent("src/components/../components/foo.tsx"))).toBe(false);
  });

  test("a traversal target is hard-blocked by the gate even when granted, backed, and confirmed", () => {
    const receipt = authorizeEffectorAction({
      ...base,
      frame: backedFrame(),
      intent: repoIntent(".border-agents/proofs/../../AGENTS.md", "apply_patch"),
      confirmed: true,
    });
    expect(receipt.decision).toBe("blocked");
    expect(receipt.rules.some((r) => r.policy_rule === "action.blocked.protected_target")).toBe(true);
  });

  // Case A — blocked before confirmation: no trusted action backing.
  test("Case A: a safe target with no action-backed memory is blocked (no_action_grant), executor never reached", () => {
    const receipt = authorizeEffectorAction({
      ...base,
      frame: emptyFrame("agent_action"),
      intent: repoIntent(".border-agents/proofs/first-act.patch"),
    });
    expect(receipt.decision).toBe("blocked");
    expect(receipt.rules.some((r) => r.policy_rule === "action.blocked.no_action_grant")).toBe(true);
  });

  // Case B — blocked EVEN AFTER confirmation: a protected target is a hard block.
  test("Case B: a protected target is blocked even when granted, backed, and confirmed", () => {
    const receipt = authorizeEffectorAction({
      ...base,
      frame: backedFrame(),
      intent: repoIntent("AGENTS.md", "apply_patch"),
      confirmed: true,
    });
    expect(receipt.decision).toBe("blocked");
    expect(receipt.rules.some((r) => r.policy_rule === "action.blocked.protected_target")).toBe(true);
  });

  test("the protected-target block runs before backing — it cannot be reached by adding memory", () => {
    const receipt = authorizeEffectorAction({
      ...base,
      frame: backedFrame(),
      intent: repoIntent("src/core/actionGate.ts"),
    });
    expect(receipt.decision).toBe("blocked");
    expect(receipt.rules.some((r) => r.policy_rule === "action.blocked.protected_target")).toBe(true);
  });

  // Case C — allowed only on a safe target, with backing and confirmation.
  test("Case C: a safe target with backing needs confirmation, then allows once confirmed", () => {
    const pending = authorizeEffectorAction({
      ...base,
      frame: backedFrame(),
      intent: repoIntent(".border-agents/proofs/first-act.patch"),
    });
    expect(pending.decision).toBe("needs_confirmation");

    const allowed = authorizeEffectorAction({
      ...base,
      frame: backedFrame(),
      intent: repoIntent(".border-agents/proofs/first-act.patch"),
      confirmed: true,
    });
    expect(allowed.decision).toBe("allow");
    expect(allowed.rules.some((r) => r.policy_rule === "action.allow.confirmed")).toBe(true);
  });
});

// A route change is a trust-boundary crossing: a downgrade to a lower-trust provider
// is never silent — it re-enters the confirmation floor.
describe("route downgrade — degradation is never silent", () => {
  const localRoute: ActionRoute = { provider: "lm_studio", locality: "local", downgraded: false };
  const downgradedRoute: ActionRoute = {
    provider: "gpt",
    fallbackOf: "claude",
    locality: "cloud",
    downgraded: true,
  };

  // A low-risk reach under play would normally allow outright; a downgraded route forces confirmation.
  const lowRiskAllow = {
    buddy: "veritas",
    effector: REVIEW,
    granted: true,
    posture: "play" as const,
    purpose: "summarize_history" as const, // low risk → allow under play
    frame: emptyFrame("summarize_history"),
    now: NOW,
  };

  test("a non-downgraded route does not add a confirmation floor", () => {
    const receipt = authorizeEffectorAction({ ...lowRiskAllow, route: localRoute });
    expect(receipt.decision).toBe("allow");
  });

  test("a downgraded route turns an otherwise-allow into needs_confirmation", () => {
    const receipt = authorizeEffectorAction({ ...lowRiskAllow, route: downgradedRoute });
    expect(receipt.decision).toBe("needs_confirmation");
    expect(receipt.rules.some((r) => r.policy_rule === "action.confirm.route_downgrade")).toBe(true);
  });

  test("confirming clears the route-downgrade floor", () => {
    const receipt = authorizeEffectorAction({ ...lowRiskAllow, route: downgradedRoute, confirmed: true });
    expect(receipt.decision).toBe("allow");
  });

  test("a downgraded route can never turn a hard block into an allow", () => {
    const receipt = authorizeEffectorAction({
      buddy: "veritas",
      effector: REVIEW,
      granted: false, // hard block
      posture: "play",
      purpose: "summarize_history",
      frame: emptyFrame("summarize_history"),
      route: downgradedRoute,
      confirmed: true,
      now: NOW,
    });
    expect(receipt.decision).toBe("blocked");
    expect(receipt.rules.some((r) => r.policy_rule === "action.blocked.ungranted")).toBe(true);
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
