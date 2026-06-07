import { summarizeFrameReceipts } from "./receiptSummary";
import type { Grade, PromptRenderResult, SafeContextFrame } from "./types";

export interface TraceReport {
  purpose: string;
  retrievedCount: number;
  frame: Record<Grade, number>;
  prompt: {
    included: number;
    excluded: number;
  };
  traceLines: TraceLine[];
}

export interface TraceLine {
  chunkId: string;
  grade: Grade;
  promptStatus: "included" | "excluded" | "unknown";
  reason: string;
  rules: string[];
}

export function buildTrace(args: { frame: SafeContextFrame; prompt?: PromptRenderResult }): TraceReport {
  const summary = summarizeFrameReceipts(args.frame, args.prompt);

  return {
    purpose: summary.purpose,
    retrievedCount: summary.total_retrieved,
    frame: summary.counts,
    prompt: {
      included: summary.prompt_included,
      excluded: summary.prompt_excluded,
    },
    traceLines: summary.items.map((item) => ({
      chunkId: item.chunk_id,
      grade: item.grade,
      promptStatus: item.prompt_status,
      reason: item.final_reason,
      rules: item.policy_rules,
    })),
  };
}

export function formatTrace(report: TraceReport): string {
  const lines = [
    `Purpose: ${report.purpose}`,
    `Retrieved: ${report.retrievedCount}`,
    `Frame: trusted=${report.frame.trusted} limited=${report.frame.limited} reference_only=${report.frame.reference_only} blocked=${report.frame.blocked} quarantined=${report.frame.quarantined}`,
    `Prompt: included=${report.prompt.included} excluded=${report.prompt.excluded}`,
    "",
    "Trace:",
  ];

  for (const traceLine of report.traceLines) {
    lines.push(`- ${traceLine.chunkId}`);
    lines.push(`  grade: ${traceLine.grade}`);
    lines.push(`  prompt: ${traceLine.promptStatus}`);
    lines.push(`  reason: ${traceLine.reason}`);
    lines.push(`  rules: ${traceLine.rules.join(", ")}`);
  }

  return lines.join("\n");
}
