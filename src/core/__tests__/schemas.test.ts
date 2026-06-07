import { describe, expect, test } from "vitest";
import { PURPOSE_POLICIES } from "../policies";
import { memoryPacketSchema, purposePolicySchema } from "../schemas";

const MEMORY_PACKET_REQUIRED = [
  "packet_id",
  "content_hash",
  "source",
  "claim_type",
  "authority",
  "sensitivity",
  "valid_until",
  "permissions",
  "labels",
  "policy",
  "derivation",
  "review",
];

const PURPOSE_POLICY_REQUIRED = [
  "id",
  "risk",
  "allow_grades_in_prompt",
  "require_permissions",
  "assertion_requires",
  "action_requires",
  "allow_sensitive",
  "render_mode",
  "require_current",
  "requires_assertion_authority",
];

describe("governance JSON Schemas", () => {
  test("MemoryPacket schema exposes the v0.1 required contract", () => {
    expect(memoryPacketSchema.$schema).toBe("https://json-schema.org/draft/2020-12/schema");
    expect(memoryPacketSchema.title).toBe("MemoryPacket");
    expect(memoryPacketSchema.additionalProperties).toBe(false);
    expect(memoryPacketSchema.required).toEqual(MEMORY_PACKET_REQUIRED);
    expect(memoryPacketSchema.properties.claim_type.enum).toEqual([
      "observed_fact",
      "current_policy",
      "historical_summary",
      "preference",
      "instruction",
      "unverified_claim",
    ]);
    expect(memoryPacketSchema.properties.permissions.required).toEqual([
      "may_retrieve",
      "may_quote",
      "may_assert",
      "may_use_for_action",
      "requires_verification_before_assertion",
    ]);
  });

  test("PurposePolicy schema exposes grades, permissions, sensitivities, and render modes", () => {
    expect(purposePolicySchema.$schema).toBe("https://json-schema.org/draft/2020-12/schema");
    expect(purposePolicySchema.title).toBe("PurposePolicy");
    expect(purposePolicySchema.additionalProperties).toBe(false);
    expect(purposePolicySchema.required).toEqual(PURPOSE_POLICY_REQUIRED);
    expect(purposePolicySchema.$defs.grade.enum).toEqual([
      "trusted",
      "limited",
      "reference_only",
      "blocked",
      "quarantined",
    ]);
    expect(purposePolicySchema.$defs.permission.enum).toEqual([
      "may_retrieve",
      "may_quote",
      "may_assert",
      "may_use_for_action",
      "requires_verification_before_assertion",
    ]);
    expect(purposePolicySchema.$defs.sensitivity.enum).toEqual([
      "public",
      "internal",
      "confidential",
      "restricted",
    ]);
    expect(purposePolicySchema.$defs.renderMode.enum).toEqual(["clean", "annotated", "strict"]);
  });

  test("built-in policies only use schema-declared enum values", () => {
    const grades = new Set(purposePolicySchema.$defs.grade.enum);
    const permissions = new Set(purposePolicySchema.$defs.permission.enum);
    const sensitivities = new Set(purposePolicySchema.$defs.sensitivity.enum);
    const renderModes = new Set(purposePolicySchema.$defs.renderMode.enum);

    for (const policy of Object.values(PURPOSE_POLICIES)) {
      expect(renderModes.has(policy.render_mode)).toBe(true);
      expect(policy.allow_grades_in_prompt.every((grade) => grades.has(grade))).toBe(true);
      expect(policy.require_permissions.every((permission) => permissions.has(permission))).toBe(true);
      expect(policy.assertion_requires.every((permission) => permissions.has(permission))).toBe(true);
      expect(policy.action_requires.every((permission) => permissions.has(permission))).toBe(true);
      expect(policy.allow_sensitive.every((sensitivity) => sensitivities.has(sensitivity))).toBe(true);
    }
  });
});
