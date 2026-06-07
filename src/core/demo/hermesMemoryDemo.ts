import { gradeMemories } from "../grader";
import { getPurposePolicy, type BuiltInPurpose } from "../policies";
import { renderPromptContext } from "../promptRenderer";
import type { ClaimType, MemoryPacket, MemoryPermissions, RetrievedMemory, Sensitivity } from "../types";

const NOW = "2026-06-07T12:00:00Z";

export const HERMES_DEMO_PURPOSES: BuiltInPurpose[] = [
  "summarize_history",
  "answer_current_policy",
  "agent_action",
  "external_share",
];

export const hermesRetrievedMemories: RetrievedMemory[] = [
  demoMemory({
    id: "user_profile",
    text: "User profile: name alias is Richard; prefers concise plans with enough context to act.",
    claimType: "preference",
    sensitivity: "internal",
    permissions: { may_use_for_action: true },
    labels: ["profile"],
  }),
  demoMemory({
    id: "project_context",
    text: "Project context: Border Agents proves purpose-aware memory grading before expanding into agent runtime features.",
    claimType: "observed_fact",
    sensitivity: "internal",
    permissions: { may_use_for_action: true },
    labels: ["project"],
  }),
  demoMemory({
    id: "old_policy",
    text: "Prior policy note: UI chrome work is the next priority.",
    claimType: "current_policy",
    sensitivity: "internal",
    validUntil: "2026-06-01T00:00:00Z",
    labels: ["stale"],
  }),
  demoMemory({
    id: "active_goal",
    text: "Active goal: implement MemoryPacket, PurposePolicy, MemoryGrader, SafeContextFrame, PromptRenderer, and GradeReceipt tests.",
    claimType: "instruction",
    sensitivity: "internal",
    labels: ["goal"],
  }),
  demoMemory({
    id: "external_note",
    text: "External share note: public project descriptions may be quoted when the packet allows quoting.",
    claimType: "observed_fact",
    sensitivity: "public",
    permissions: { may_quote: true, may_use_for_action: false },
    labels: ["shareable"],
  }),
  demoMemory({
    id: "unverified_preference",
    text: "Unverified note: the user may want all future branches pushed automatically.",
    claimType: "unverified_claim",
    sensitivity: "internal",
    permissions: { requires_verification_before_assertion: true },
    labels: ["unverified"],
  }),
  demoMemory({
    id: "review_required",
    text: "Review-required note: this memory mentions sensitive workflow details and must be inspected before use.",
    claimType: "observed_fact",
    sensitivity: "confidential",
    labels: ["quarantined"],
  }),
];

export function runHermesMemoryDemo(now = NOW) {
  return HERMES_DEMO_PURPOSES.map((purposeId) => {
    const policy = getPurposePolicy(purposeId);
    const frame = gradeMemories({ purpose: policy, retrieved: hermesRetrievedMemories, now });
    const prompt = renderPromptContext(frame, policy);

    return {
      purpose: purposeId,
      frame,
      prompt,
    };
  });
}

function demoMemory(args: {
  id: string;
  text: string;
  claimType: ClaimType;
  sensitivity: Sensitivity;
  permissions?: Partial<MemoryPermissions>;
  labels?: string[];
  validUntil?: string | null;
}): RetrievedMemory {
  const packet: MemoryPacket = {
    packet_id: `mem_pkt_${args.id}`,
    content_hash: `sha256:demo-${args.id}`,
    source: {
      type: "chat_session",
      id: `hermes/${args.id}`,
      created_at: "2026-06-07T09:00:00Z",
    },
    claim_type: args.claimType,
    authority: "high",
    sensitivity: args.sensitivity,
    valid_until: args.validUntil ?? null,
    permissions: {
      may_retrieve: true,
      may_quote: false,
      may_assert: true,
      may_use_for_action: false,
      requires_verification_before_assertion: false,
      ...args.permissions,
    },
    labels: args.labels ?? [],
    policy: {
      id: "border-agents-default",
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
