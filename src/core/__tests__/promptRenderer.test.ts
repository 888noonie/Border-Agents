import { describe, expect, test } from "vitest";
import { gradeMemories } from "../grader";
import { getPurposePolicy } from "../policies";
import { renderPromptContext } from "../promptRenderer";
import type { MemoryPacket, MemoryPermissions, RetrievedMemory } from "../types";

const NOW = "2026-06-07T12:00:00Z";

describe("PromptRenderer", () => {
  test("limited chunks render with constraints in annotated mode", () => {
    const policy = getPurposePolicy("summarize_history");
    const limited = memory({
      id: "limited",
      text: "This is useful history but cannot be asserted.",
      permissions: { may_assert: false },
    });

    const frame = gradeMemories({ purpose: policy, retrieved: [limited], now: NOW });
    const rendered = renderPromptContext(frame, policy);

    expect(frame.limited).toEqual([limited]);
    expect(rendered.included).toEqual([
      {
        chunk_id: "chunk_limited",
        packet_id: "mem_pkt_limited",
        grade: "limited",
      },
    ]);
    expect(rendered.context).toContain("[limited; constraints apply; packet=mem_pkt_limited]");
    expect(rendered.context).toContain(limited.text);
  });

  test("strict mode excludes limited, reference-only, blocked, and quarantined content from prompt context", () => {
    const policy = getPurposePolicy("agent_action");
    const trusted = memory({
      id: "trusted",
      text: "This action instruction has action permission.",
    });
    const limited = memory({
      id: "limited",
      text: "This action instruction requires verification.",
      permissions: { requires_verification_before_assertion: true },
    });
    const referenceOnly = memory({
      id: "reference_only",
      text: "This action instruction is expired.",
      validUntil: "2026-06-01T00:00:00Z",
    });
    const blocked = memory({
      id: "blocked",
      text: "This action instruction has no action permission.",
      permissions: { may_use_for_action: false },
    });
    const quarantined = memory({
      id: "quarantined",
      text: "This action instruction is held for review.",
      labels: ["quarantined"],
    });

    const frame = gradeMemories({
      purpose: policy,
      retrieved: [trusted, limited, referenceOnly, blocked, quarantined],
      now: NOW,
    });
    const rendered = renderPromptContext(frame, policy);

    expect(rendered.context).toContain(trusted.text);
    expect(rendered.context).not.toContain(limited.text);
    expect(rendered.context).not.toContain(referenceOnly.text);
    expect(rendered.context).not.toContain(blocked.text);
    expect(rendered.context).not.toContain(quarantined.text);
    expect(rendered.excluded.map((item) => item.grade).sort()).toEqual([
      "blocked",
      "limited",
      "quarantined",
      "reference_only",
    ]);
  });
});

function memory(args: {
  id: string;
  text: string;
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
    claim_type: "observed_fact",
    authority: "high",
    sensitivity: "internal",
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
