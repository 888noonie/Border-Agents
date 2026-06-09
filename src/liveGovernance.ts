import type { BuddySettings } from "./buddyProfiles";
import {
  buildNexusPanelData,
  buildVeritasPanelData,
  getPurposePolicy,
  gradeMemories,
  renderPromptContext,
  type BuiltInPurpose,
  type MemoryPacket,
  type PromptRenderResult,
  type RetrievedMemory,
  type SafeContextFrame,
} from "./core";

export type SessionChatLine = {
  role: "user" | "assistant" | "status";
  text: string;
};

export interface BuddyGovernanceSnapshot {
  purpose: BuiltInPurpose;
  retrieved: RetrievedMemory[];
  frame: SafeContextFrame;
  prompt: PromptRenderResult;
}

export interface GovernanceBuddyMessages {
  crab: string;
  owl: string;
  fox: string;
}

export function buildBuddyGovernanceSnapshot(args: {
  buddyId: string;
  history: SessionChatLine[];
  settings: BuddySettings;
  now?: string;
}): BuddyGovernanceSnapshot | null {
  if (args.settings.memoryMode === "off") {
    return null;
  }

  const purpose = selectPurpose(args.settings);
  const policy = getPurposePolicy(purpose);
  const retrieved = buildRetrievedMemories(args);

  if (retrieved.length === 0) {
    return null;
  }

  const frame = gradeMemories({
    purpose: policy,
    retrieved,
    now: args.now,
  });
  const prompt = renderPromptContext(frame, policy);

  return {
    purpose,
    retrieved,
    frame,
    prompt,
  };
}

export function selectPurpose(settings: BuddySettings): BuiltInPurpose {
  if (settings.allowExternalShare) {
    return "external_share";
  }

  if (settings.allowAction) {
    return "agent_action";
  }

  if (settings.memoryMode === "reference_only") {
    return "answer_current_policy";
  }

  return "summarize_history";
}

export function buildGovernanceBuddyMessages(snapshot: BuddyGovernanceSnapshot): GovernanceBuddyMessages {
  const nexus = buildNexusPanelData({
    frame: snapshot.frame,
    prompt: snapshot.prompt,
  });
  const veritas = buildVeritasPanelData({
    frame: snapshot.frame,
    prompt: snapshot.prompt,
  });
  const heldCount =
    nexus.frameBuckets.reference_only +
    nexus.frameBuckets.blocked +
    nexus.frameBuckets.quarantined;

  return {
    crab: `${nexus.frameBuckets.trusted} trusted · ${nexus.frameBuckets.limited} limited · ${heldCount} held back.`,
    owl:
      veritas.warnings.length > 0
        ? `${veritas.warnings.length} receipt warnings need review.`
        : `${veritas.evidenceReady.length} prompt entries backed by receipts.`,
    fox: `${purposeLabel(snapshot.purpose)} context: ${nexus.promptSummary.included} in · ${nexus.promptSummary.excluded} out.`,
  };
}

function buildRetrievedMemories(args: {
  buddyId: string;
  history: SessionChatLine[];
  settings: BuddySettings;
  now?: string;
}): RetrievedMemory[] {
  const createdAt = args.now ?? new Date().toISOString();
  const lines = args.history.filter((line) => line.role !== "status" && line.text.trim().length > 0);
  const lastIndex = Math.max(lines.length - 1, 0);

  return lines.map((line, index) => {
    const packet = buildPacket({
      buddyId: args.buddyId,
      createdAt,
      index,
      line,
      settings: args.settings,
    });

    return {
      chunk_id: `chunk:${args.buddyId}:${index}`,
      text: line.text.trim(),
      score: Math.max(0.25, 1 - (lastIndex - index) * 0.08),
      packet,
    };
  });
}

function buildPacket(args: {
  buddyId: string;
  createdAt: string;
  index: number;
  line: SessionChatLine;
  settings: BuddySettings;
}): MemoryPacket {
  const assistantLine = args.line.role === "assistant";
  const sensitivity =
    assistantLine && args.settings.allowExternalShare
      ? "public"
      : "internal";

  return {
    packet_id: `packet:${args.buddyId}:${args.index}`,
    content_hash: `sha256:session-${args.buddyId}-${args.index}`,
    source: {
      type: "chat_session",
      id: `${args.buddyId}/session`,
      created_at: args.createdAt,
    },
    claim_type: assistantLine ? "observed_fact" : "instruction",
    authority: assistantLine ? "medium" : "high",
    sensitivity,
    valid_until: null,
    permissions: {
      may_retrieve: true,
      may_quote: assistantLine && args.settings.allowExternalShare,
      may_assert: assistantLine,
      may_use_for_action: args.settings.allowAction,
      requires_verification_before_assertion: false,
    },
    labels: [],
    policy: {
      id: "border-agents-live-session",
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
}

function purposeLabel(purpose: BuiltInPurpose) {
  if (purpose === "summarize_history") {
    return "History";
  }

  if (purpose === "answer_current_policy") {
    return "Policy";
  }

  if (purpose === "agent_action") {
    return "Action";
  }

  return "Share";
}
