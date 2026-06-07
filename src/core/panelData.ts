import { GRADE_ORDER, summarizeFrameReceipts, type ReceiptSummaryItem } from "./receiptSummary";
import type { Grade, PromptRenderResult, SafeContextFrame } from "./types";

export type TrustBadgeState = Grade;

export interface NexusPanelData {
  purpose: string;
  retrievedCount: number;
  frameBuckets: Record<Grade, number>;
  promptSummary: {
    included: number;
    excluded: number;
  };
  trustBadgeState: TrustBadgeState;
  topSources: NexusSourceSummary[];
}

export interface NexusSourceSummary {
  sourceId: string;
  sourceType: string;
  count: number;
  highestGrade: Grade;
}

export interface VeritasPanelData {
  purpose: string;
  receiptGroups: Record<Grade, VeritasReceiptItem[]>;
  warnings: VeritasWarning[];
  evidenceReady: VeritasReceiptItem[];
}

export interface VeritasReceiptItem {
  chunkId: string;
  packetId: string;
  grade: Grade;
  promptStatus: "included" | "excluded" | "unknown";
  finalReason: string;
  policyRules: string[];
}

export interface VeritasWarning extends VeritasReceiptItem {
  warningType: "blocked" | "quarantined";
}

export function buildNexusPanelData(args: {
  frame: SafeContextFrame;
  prompt?: PromptRenderResult;
}): NexusPanelData {
  const summary = summarizeFrameReceipts(args.frame, args.prompt);

  return {
    purpose: summary.purpose,
    retrievedCount: summary.total_retrieved,
    frameBuckets: summary.counts,
    promptSummary: {
      included: summary.prompt_included,
      excluded: summary.prompt_excluded,
    },
    trustBadgeState: deriveTrustBadgeState(summary.counts),
    topSources: summarizeSources(summary.items),
  };
}

export function buildVeritasPanelData(args: {
  frame: SafeContextFrame;
  prompt?: PromptRenderResult;
}): VeritasPanelData {
  const summary = summarizeFrameReceipts(args.frame, args.prompt);
  const receiptGroups = emptyReceiptGroups();
  const warnings: VeritasWarning[] = [];
  const evidenceReady: VeritasReceiptItem[] = [];

  for (const item of summary.items) {
    const receiptItem = toVeritasReceiptItem(item);
    receiptGroups[receiptItem.grade].push(receiptItem);

    if (receiptItem.grade === "blocked" || receiptItem.grade === "quarantined") {
      warnings.push({
        ...receiptItem,
        warningType: receiptItem.grade,
      });
    }

    if (receiptItem.promptStatus === "included" && (receiptItem.grade === "trusted" || receiptItem.grade === "limited")) {
      evidenceReady.push(receiptItem);
    }
  }

  return {
    purpose: summary.purpose,
    receiptGroups,
    warnings,
    evidenceReady,
  };
}

function deriveTrustBadgeState(counts: Record<Grade, number>): TrustBadgeState {
  if (counts.quarantined > 0) return "quarantined";
  if (counts.blocked > 0) return "blocked";
  if (counts.reference_only > 0) return "reference_only";
  if (counts.limited > 0) return "limited";
  return "trusted";
}

function summarizeSources(items: ReceiptSummaryItem[]): NexusSourceSummary[] {
  const bySource = new Map<string, NexusSourceSummary>();

  for (const item of items) {
    if (!item.source_id || !item.source_type) {
      continue;
    }

    const key = `${item.source_type}:${item.source_id}`;
    const existing = bySource.get(key);

    if (existing) {
      existing.count += 1;
      existing.highestGrade = maxGrade(existing.highestGrade, item.grade);
    } else {
      bySource.set(key, {
        sourceId: item.source_id,
        sourceType: item.source_type,
        count: 1,
        highestGrade: item.grade,
      });
    }
  }

  return Array.from(bySource.values()).sort((left, right) => {
    if (right.count !== left.count) {
      return right.count - left.count;
    }

    return left.sourceId.localeCompare(right.sourceId);
  });
}

function maxGrade(left: Grade, right: Grade): Grade {
  return gradeSeverity(right) > gradeSeverity(left) ? right : left;
}

function gradeSeverity(grade: Grade) {
  return GRADE_ORDER.indexOf(grade);
}

function emptyReceiptGroups(): Record<Grade, VeritasReceiptItem[]> {
  return {
    trusted: [],
    limited: [],
    reference_only: [],
    blocked: [],
    quarantined: [],
  };
}

function toVeritasReceiptItem(item: ReceiptSummaryItem): VeritasReceiptItem {
  return {
    chunkId: item.chunk_id,
    packetId: item.packet_id,
    grade: item.grade,
    promptStatus: item.prompt_status,
    finalReason: item.final_reason,
    policyRules: item.policy_rules,
  };
}
