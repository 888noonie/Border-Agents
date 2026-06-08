import type { Grade, PromptRenderResult, RetrievedMemory, SafeContextFrame } from "./types";
import type { DerivationStep } from "./types";

export const GRADE_ORDER: Grade[] = ["trusted", "limited", "reference_only", "blocked", "quarantined"];

export type PromptEntryStatus = "included" | "excluded" | "unknown";

export interface ReceiptSummaryItem {
  receipt_id: string;
  chunk_id: string;
  packet_id: string;
  purpose: string;
  grade: Grade;
  final_reason: string;
  policy_rules: string[];
  source_id: string | null;
  source_type: string | null;
  prompt_status: PromptEntryStatus;
  prompt_reason: string | null;
  rule_details: DerivationStep[];
}

export type ReceiptSummaryByGrade = Record<Grade, ReceiptSummaryItem[]>;

export interface FrameReceiptSummary {
  purpose: string;
  counts: Record<Grade, number>;
  total_retrieved: number;
  total_receipts: number;
  prompt_included: number;
  prompt_excluded: number;
  by_grade: ReceiptSummaryByGrade;
  items: ReceiptSummaryItem[];
}

export function summarizeFrameReceipts(frame: SafeContextFrame, prompt?: PromptRenderResult): FrameReceiptSummary {
  const memoryByChunk = new Map<string, RetrievedMemory>();

  for (const grade of GRADE_ORDER) {
    for (const memory of frame[grade]) {
      memoryByChunk.set(memory.chunk_id, memory);
    }
  }

  const promptStatusByChunk = buildPromptStatusMap(prompt);
  const byGrade = emptyByGrade();
  const items = frame.receipts.map((receipt) => {
    const memory = memoryByChunk.get(receipt.chunk_id);
    const promptStatus = promptStatusByChunk.get(receipt.chunk_id);
    const finalRule = receipt.rules[receipt.rules.length - 1];
    const item: ReceiptSummaryItem = {
      receipt_id: receipt.receipt_id,
      chunk_id: receipt.chunk_id,
      packet_id: receipt.packet_id,
      purpose: receipt.purpose,
      grade: receipt.grade,
      final_reason: finalRule?.reason ?? "No receipt reason recorded",
      policy_rules: receipt.rules.map((rule) => rule.policy_rule),
      source_id: memory?.packet.source.id ?? null,
      source_type: memory?.packet.source.type ?? null,
      prompt_status: promptStatus?.status ?? "unknown",
      prompt_reason: promptStatus?.reason ?? null,
      rule_details: receipt.rules,
    };

    byGrade[item.grade].push(item);
    return item;
  });

  return {
    purpose: frame.purpose,
    counts: countGrades(frame),
    total_retrieved: GRADE_ORDER.reduce((count, grade) => count + frame[grade].length, 0),
    total_receipts: frame.receipts.length,
    prompt_included: prompt?.included.length ?? 0,
    prompt_excluded: prompt?.excluded.length ?? 0,
    by_grade: byGrade,
    items,
  };
}

function emptyByGrade(): ReceiptSummaryByGrade {
  return {
    trusted: [],
    limited: [],
    reference_only: [],
    blocked: [],
    quarantined: [],
  };
}

function countGrades(frame: SafeContextFrame): Record<Grade, number> {
  return {
    trusted: frame.trusted.length,
    limited: frame.limited.length,
    reference_only: frame.reference_only.length,
    blocked: frame.blocked.length,
    quarantined: frame.quarantined.length,
  };
}

function buildPromptStatusMap(prompt: PromptRenderResult | undefined) {
  const map = new Map<string, { status: PromptEntryStatus; reason: string | null }>();

  if (!prompt) {
    return map;
  }

  for (const item of prompt.included) {
    map.set(item.chunk_id, {
      status: "included",
      reason: null,
    });
  }

  for (const item of prompt.excluded) {
    map.set(item.chunk_id, {
      status: "excluded",
      reason: item.reason,
    });
  }

  return map;
}
