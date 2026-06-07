import { describe, expect, test } from "vitest";
import { gradeMemories } from "../grader";
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
