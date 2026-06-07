import type {
  DerivationStep,
  Grade,
  GradeReceipt,
  PurposePolicy,
  RetrievedMemory,
  SafeContextFrame,
} from "./types";

const GRADES: Grade[] = ["trusted", "limited", "reference_only", "blocked", "quarantined"];

export function gradeMemories(args: {
  purpose: PurposePolicy;
  retrieved: RetrievedMemory[];
  now?: string;
}): SafeContextFrame {
  const derivedAt = args.now ?? new Date().toISOString();
  const frame: SafeContextFrame = {
    purpose: args.purpose.id,
    trusted: [],
    limited: [],
    reference_only: [],
    blocked: [],
    quarantined: [],
    receipts: [],
  };

  for (const memory of args.retrieved) {
    const { grade, rules } = gradeMemory(memory, args.purpose, derivedAt);

    frame[grade].push(memory);
    frame.receipts.push({
      receipt_id: `grade:${args.purpose.id}:${memory.chunk_id}:${derivedAt}`,
      packet_id: memory.packet.packet_id,
      chunk_id: memory.chunk_id,
      purpose: args.purpose.id,
      grade,
      derived_at: derivedAt,
      rules,
    });
  }

  return frame;
}

export function gradeMemory(
  memory: RetrievedMemory,
  purpose: PurposePolicy,
  now: string = new Date().toISOString(),
): { grade: Grade; rules: DerivationStep[] } {
  const packet = memory.packet;
  const rules: DerivationStep[] = [];

  const finalRule = (grade: Grade, reason: string, policyRule: string): { grade: Grade; rules: DerivationStep[] } => {
    rules.push(rule("grade", grade, purpose.id, reason, policyRule));
    return { grade, rules };
  };

  if (packet.labels.includes("quarantined") || packet.review.requires_review) {
    rules.push(rule("labels", packet.labels, "packet", "packet requires review before use", "packet.review"));
    return finalRule("quarantined", "packet is held for review", "grade.quarantined.review_required");
  }

  if (packet.labels.includes("blocked")) {
    rules.push(rule("labels", packet.labels, "packet", "packet carries a blocked label", "packet.labels.blocked"));
    return finalRule("blocked", "packet label blocks use for the active purpose", "grade.blocked.label");
  }

  if (!packet.permissions.may_retrieve) {
    rules.push(
      rule("may_retrieve", false, "packet.permissions", "retrieval permission is required", "permissions.may_retrieve"),
    );
    return finalRule("blocked", "packet is not authorized for retrieval", "grade.blocked.permission");
  }

  if (!purpose.allow_sensitive.includes(packet.sensitivity)) {
    rules.push(
      rule(
        "sensitivity",
        packet.sensitivity,
        "packet.sensitivity",
        `purpose allows ${purpose.allow_sensitive.join(", ")}`,
        "purpose.allow_sensitive",
      ),
    );

    if (packet.sensitivity === "confidential" || packet.sensitivity === "restricted") {
      return finalRule("quarantined", "packet sensitivity requires review for this purpose", "grade.quarantined.sensitivity");
    }

    return finalRule("blocked", "packet sensitivity is not permitted for this purpose", "grade.blocked.sensitivity");
  }

  if (purpose.require_current && isExpired(packet.valid_until, now)) {
    rules.push(
      rule("valid_until", packet.valid_until, "packet.valid_until", "purpose requires current memory", "purpose.require_current"),
    );
    return finalRule("reference_only", "expired packet may be inspected but not trusted", "grade.reference_only.expired");
  }

  for (const permission of purpose.require_permissions) {
    if (!packet.permissions[permission]) {
      rules.push(
        rule(
          permission,
          packet.permissions[permission],
          "packet.permissions",
          `purpose requires ${permission}`,
          "purpose.require_permissions",
        ),
      );
      return finalRule("blocked", "packet lacks a required permission", "grade.blocked.required_permission");
    }
  }

  for (const permission of purpose.action_requires) {
    if (!packet.permissions[permission]) {
      rules.push(
        rule(
          permission,
          packet.permissions[permission],
          "packet.permissions",
          `action use requires ${permission}`,
          "purpose.action_requires",
        ),
      );
      return finalRule("blocked", "packet cannot influence an action", "grade.blocked.action_permission");
    }
  }

  const limitedReasons: DerivationStep[] = [];

  for (const permission of purpose.assertion_requires) {
    if (!packet.permissions[permission]) {
      limitedReasons.push(
        rule(
          permission,
          packet.permissions[permission],
          "packet.permissions",
          `assertion requires ${permission}`,
          "purpose.assertion_requires",
        ),
      );
    }
  }

  if (purpose.requires_assertion_authority && packet.permissions.requires_verification_before_assertion) {
    limitedReasons.push(
      rule(
        "requires_verification_before_assertion",
        true,
        "packet.permissions",
        "assertion needs verification before trusted use",
        "permissions.requires_verification_before_assertion",
      ),
    );
  }

  if (purpose.requires_assertion_authority && packet.authority === "low") {
    limitedReasons.push(
      rule("authority", packet.authority, "packet.authority", "low authority cannot be trusted for assertions", "packet.authority"),
    );
  }

  if (!packet.permissions.may_assert && purpose.assertion_requires.length === 0) {
    limitedReasons.push(
      rule("may_assert", false, "packet.permissions", "packet can be retrieved but not asserted", "permissions.may_assert"),
    );
  }

  if (limitedReasons.length > 0) {
    rules.push(...limitedReasons);
    return finalRule("limited", "packet is relevant but constrained", "grade.limited.constraints");
  }

  rules.push(
    rule(
      "permissions",
      purpose.require_permissions,
      "purpose.require_permissions",
      "all required permissions are present",
      "grade.trusted.permissions",
    ),
  );
  return finalRule("trusted", "packet is authorized for the active purpose", "grade.trusted");
}

export function emptyFrame(purpose: string): SafeContextFrame {
  return {
    purpose,
    trusted: [],
    limited: [],
    reference_only: [],
    blocked: [],
    quarantined: [],
    receipts: [],
  };
}

export function getReceiptForChunk(frame: SafeContextFrame, chunkId: string): GradeReceipt | undefined {
  return frame.receipts.find((receipt) => receipt.chunk_id === chunkId);
}

export function allFrameMemories(frame: SafeContextFrame): RetrievedMemory[] {
  return GRADES.flatMap((grade) => frame[grade]);
}

function isExpired(validUntil: string | null, now: string) {
  if (!validUntil) {
    return false;
  }

  return new Date(validUntil).getTime() < new Date(now).getTime();
}

function rule(field: string, value: unknown, source: string, reason: string, policyRule: string): DerivationStep {
  return {
    field,
    value,
    source,
    reason,
    policy_rule: policyRule,
  };
}
