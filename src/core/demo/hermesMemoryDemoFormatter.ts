import type { runHermesMemoryDemo } from "./hermesMemoryDemo";
import type { Grade, SafeContextFrame } from "../types";

type HermesDemoResult = ReturnType<typeof runHermesMemoryDemo>[number];

const GRADES: Grade[] = ["trusted", "limited", "reference_only", "blocked", "quarantined"];

export function formatHermesMemoryDemo(results: HermesDemoResult[]): string {
  const lines: string[] = [
    "Border Agents Hermes Memory Demo",
    "same retrieved memories + different purposes = different SafeContextFrames",
    "",
  ];

  for (const result of results) {
    lines.push(`Purpose: ${result.purpose}`);
    lines.push(formatFrameSummary(result.frame));

    for (const grade of GRADES) {
      lines.push(`${grade}:`);

      if (result.frame[grade].length === 0) {
        lines.push("  - none");
        continue;
      }

      for (const memory of result.frame[grade]) {
        const receipt = result.frame.receipts.find((item) => item.chunk_id === memory.chunk_id);
        const reason = receipt?.rules[receipt.rules.length - 1]?.reason ?? "no receipt reason";
        lines.push(`  - ${memory.chunk_id} (${memory.packet.packet_id})`);
        lines.push(`    text: ${memory.text}`);
        lines.push(`    reason: ${reason}`);
      }
    }

    lines.push("prompt_context:");
    if (result.prompt.context) {
      for (const promptLine of result.prompt.context.split("\n")) {
        lines.push(`  ${promptLine}`);
      }
    } else {
      lines.push("  none");
    }

    lines.push("receipts:");
    for (const receipt of result.frame.receipts) {
      const rules = receipt.rules.map((rule) => rule.policy_rule).join(", ");
      lines.push(`  - ${receipt.chunk_id}: ${receipt.grade} [${rules}]`);
    }

    lines.push("");
  }

  return lines.join("\n").trimEnd();
}

function formatFrameSummary(frame: SafeContextFrame) {
  return `Frame: trusted=${frame.trusted.length} limited=${frame.limited.length} reference_only=${frame.reference_only.length} blocked=${frame.blocked.length} quarantined=${frame.quarantined.length} receipts=${frame.receipts.length}`;
}
