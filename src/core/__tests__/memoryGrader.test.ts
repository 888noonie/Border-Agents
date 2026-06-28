import { describe, expect, test } from "vitest";
import { getReceiptForChunk, gradeMemories } from "../grader";
import type { Grade } from "../types";
import { createCustomPurposePolicy, getPurposePolicy } from "../policies";
import type { ClaimType, MemoryPacket, MemoryPermissions, RetrievedMemory, Sensitivity } from "../types";

const NOW = "2026-06-07T12:00:00Z";

describe("MemoryGrader", () => {
  test("expired chunks are not trusted for current policy answers", () => {
    const policy = getPurposePolicy("answer_current_policy");
    const expired = memory({
      id: "expired_policy",
      text: "This policy expired before the request time.",
      claimType: "current_policy",
      validUntil: "2026-06-01T00:00:00Z",
    });

    const frame = gradeMemories({ purpose: policy, retrieved: [expired], now: NOW });

    expect(frame.trusted).toHaveLength(0);
    expect(frame.reference_only).toContain(expired);
    expect(frame.receipts[0].grade).toBe("reference_only");
    expect(frame.receipts[0].rules.some((rule) => rule.policy_rule === "purpose.require_current")).toBe(true);
  });

  test("chunks without may_use_for_action cannot influence agent_action", () => {
    const policy = getPurposePolicy("agent_action");
    const noActionPermission = memory({
      id: "no_action",
      text: "This chunk can be read but cannot authorize action.",
      permissions: { may_use_for_action: false },
    });

    const frame = gradeMemories({ purpose: policy, retrieved: [noActionPermission], now: NOW });

    expect(frame.trusted).toHaveLength(0);
    expect(frame.blocked).toContain(noActionPermission);
    expect(frame.receipts[0].rules.some((rule) => rule.field === "may_use_for_action")).toBe(true);
  });

  test("blocked chunks are preserved in the frame ledger", () => {
    const policy = getPurposePolicy("answer_current_policy");
    const blocked = memory({
      id: "blocked",
      text: "This chunk is blocked by label.",
      labels: ["blocked"],
    });

    const frame = gradeMemories({ purpose: policy, retrieved: [blocked], now: NOW });

    expect(frame.blocked).toEqual([blocked]);
    expect(frame.receipts).toHaveLength(1);
    expect(frame.receipts[0]).toMatchObject({
      chunk_id: "chunk_blocked",
      packet_id: "mem_pkt_blocked",
      grade: "blocked",
    });
  });

  test("custom purposes cannot widen permissions without an override receipt", () => {
    const base = getPurposePolicy("external_share");
    const { policy, overrideReceipt } = createCustomPurposePolicy({
      id: "custom_external_share",
      base,
      changes: {
        allow_sensitive: ["public", "internal"],
        require_permissions: ["may_retrieve"],
        render_mode: "annotated",
      },
      now: NOW,
    });

    expect(policy.allow_sensitive).toEqual(["public"]);
    expect(policy.require_permissions).toEqual(["may_retrieve", "may_quote"]);
    expect(policy.render_mode).toBe("strict");
    expect(overrideReceipt).toBeNull();
  });

  test("custom purposes can widen permissions with an override receipt", () => {
    const base = getPurposePolicy("external_share");
    const { policy, overrideReceipt } = createCustomPurposePolicy({
      id: "custom_external_share_override",
      base,
      changes: {
        allow_sensitive: ["public", "internal"],
      },
      override: {
        id: "override_001",
        reason: "manual approval for internal demo",
        approved_by: "owner",
        approved_at: NOW,
      },
      now: NOW,
    });

    expect(policy.allow_sensitive).toEqual(["public", "internal"]);
    expect(overrideReceipt).not.toBeNull();
    expect(overrideReceipt?.rules[0]).toMatchObject({
      field: "allow_sensitive",
      policy_rule: "custom_purpose.no_widening_without_override",
    });
  });
});

// Contract (not coverage) for getReceiptForChunk: it is the chunk-id resolver the governance
// wire (soulActions.actionGradeSummary → action_result.backedBy) leans on, so these tests pin
// the INVARIANT that frame.receipts is the authoritative chunk-id-indexed store — not that a
// .find() loop runs. If a future change to how the frame is populated breaks any of these, the
// wire's `backedBy` would silently lie while the suite still goes green; that is the exact shape
// this slice exists to prevent.
describe("getReceiptForChunk contract", () => {
  const ALL_GRADES: Grade[] = ["trusted", "limited", "reference_only", "blocked", "quarantined"];

  // A spread of grades from one purpose: a usable chunk (trusted), one missing the action
  // permission (blocked), and one expired-but-readable claim (reference_only).
  function mixedFrame() {
    const policy = getPurposePolicy("agent_action");
    const usable = memory({ id: "usable", text: "trusted for action." });
    const noAction = memory({ id: "no_action", text: "cannot authorize action.", permissions: { may_use_for_action: false } });
    const expired = memory({ id: "stale", text: "expired claim.", claimType: "current_policy", validUntil: "2026-06-01T00:00:00Z" });
    return gradeMemories({ purpose: policy, retrieved: [usable, noAction, expired], now: NOW });
  }

  test("every chunk in any grade bucket resolves to its own receipt", () => {
    const frame = mixedFrame();
    for (const grade of ALL_GRADES) {
      for (const mem of frame[grade]) {
        const receipt = getReceiptForChunk(frame, mem.chunk_id);
        expect(receipt).toBeDefined();
        expect(receipt!.chunk_id).toBe(mem.chunk_id);
        // The resolved receipt's grade matches the bucket the chunk lives in — buckets and
        // receipts cannot disagree, which is what makes backedBy's "trusted" count honest.
        expect(receipt!.grade).toBe(grade);
      }
    }
  });

  test("a chunk absent from the frame returns undefined, not a throw", () => {
    // The wire layer calls this on ids it does not control; it must degrade to undefined.
    const frame = mixedFrame();
    expect(getReceiptForChunk(frame, "chunk_does_not_exist")).toBeUndefined();
  });

  test("two distinct chunk ids never resolve to the same receipt", () => {
    const frame = mixedFrame();
    const a = getReceiptForChunk(frame, "chunk_usable");
    const b = getReceiptForChunk(frame, "chunk_no_action");
    expect(a).toBeDefined();
    expect(b).toBeDefined();
    expect(a!.receipt_id).not.toBe(b!.receipt_id);
    expect(a).not.toBe(b);
  });
});

function memory(args: {
  id: string;
  text: string;
  claimType?: ClaimType;
  sensitivity?: Sensitivity;
  labels?: string[];
  permissions?: Partial<MemoryPermissions>;
  validUntil?: string | null;
}): RetrievedMemory {
  const packet: MemoryPacket = {
    packet_id: `mem_pkt_${args.id}`,
    content_hash: `sha256:${args.id}`,
    source: {
      type: "user_note",
      id: `test/${args.id}`,
      created_at: "2026-06-01T00:00:00Z",
    },
    claim_type: args.claimType ?? "observed_fact",
    authority: "high",
    sensitivity: args.sensitivity ?? "internal",
    valid_until: args.validUntil ?? null,
    permissions: {
      may_retrieve: true,
      may_quote: true,
      may_assert: true,
      may_use_for_action: true,
      requires_verification_before_assertion: false,
      ...args.permissions,
    },
    labels: args.labels ?? [],
    policy: {
      id: "test-policy",
      version: "0.1.0",
    },
    derivation: [],
    review: {
      mode: "strict",
      requires_review: false,
      reviewed_by: null,
      reviewed_at: null,
    },
  };

  return {
    chunk_id: `chunk_${args.id}`,
    text: args.text,
    score: 0.9,
    packet,
  };
}
