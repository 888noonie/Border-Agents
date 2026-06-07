import type { Grade, PromptRenderResult, PurposePolicy, RetrievedMemory, SafeContextFrame } from "./types";

export function renderPromptContext(frame: SafeContextFrame, policy: PurposePolicy): PromptRenderResult {
  const allowed = allowedGrades(policy);
  const included: PromptRenderResult["included"] = [];
  const excluded: PromptRenderResult["excluded"] = [];
  const lines: string[] = [];

  for (const grade of ["trusted", "limited", "reference_only", "blocked", "quarantined"] as const) {
    for (const memory of frame[grade]) {
      if (allowed.includes(grade)) {
        included.push({
          chunk_id: memory.chunk_id,
          packet_id: memory.packet.packet_id,
          grade,
        });
        lines.push(renderMemoryLine(memory, grade));
      } else {
        excluded.push({
          chunk_id: memory.chunk_id,
          packet_id: memory.packet.packet_id,
          grade,
          reason: `${grade} content is excluded by ${policy.render_mode} mode`,
        });
      }
    }
  }

  return {
    mode: policy.render_mode,
    purpose: policy.id,
    context: lines.join("\n\n"),
    included,
    excluded,
  };
}

function allowedGrades(policy: PurposePolicy): Grade[] {
  if (policy.render_mode === "strict" || policy.render_mode === "clean") {
    return policy.allow_grades_in_prompt.includes("trusted") ? ["trusted"] : [];
  }

  return policy.allow_grades_in_prompt.filter((grade) => grade === "trusted" || grade === "limited");
}

function renderMemoryLine(memory: RetrievedMemory, grade: Grade) {
  if (grade === "limited") {
    return `[limited; constraints apply; packet=${memory.packet.packet_id}]\n${memory.text}`;
  }

  return `[trusted; packet=${memory.packet.packet_id}]\n${memory.text}`;
}
