import type {
  DerivationStep,
  Grade,
  PermissionKey,
  PolicyOverrideAuthorization,
  PolicyOverrideReceipt,
  PurposePolicy,
  RenderMode,
  Sensitivity,
} from "./types";

export const BUILT_IN_PURPOSES = [
  "summarize_history",
  "answer_current_policy",
  "agent_action",
  "external_share",
] as const;

export type BuiltInPurpose = (typeof BUILT_IN_PURPOSES)[number];

const PROMPT_GRADE_ORDER: Grade[] = [
  "trusted",
  "limited",
  "reference_only",
  "blocked",
  "quarantined",
];

const SENSITIVITY_ORDER: Sensitivity[] = ["public", "internal", "confidential", "restricted"];

const RENDER_MODE_ORDER: RenderMode[] = ["strict", "clean", "annotated"];

export const PURPOSE_POLICIES: Record<BuiltInPurpose, PurposePolicy> = {
  summarize_history: {
    id: "summarize_history",
    risk: "low",
    allow_grades_in_prompt: ["trusted", "limited"],
    require_permissions: ["may_retrieve"],
    assertion_requires: [],
    action_requires: [],
    allow_sensitive: ["public", "internal"],
    render_mode: "annotated",
    require_current: false,
    requires_assertion_authority: false,
  },
  answer_current_policy: {
    id: "answer_current_policy",
    risk: "medium",
    allow_grades_in_prompt: ["trusted", "limited"],
    require_permissions: ["may_retrieve", "may_assert"],
    assertion_requires: ["may_assert"],
    action_requires: [],
    allow_sensitive: ["public", "internal"],
    render_mode: "annotated",
    require_current: true,
    requires_assertion_authority: true,
  },
  agent_action: {
    id: "agent_action",
    risk: "high",
    allow_grades_in_prompt: ["trusted"],
    require_permissions: ["may_retrieve", "may_use_for_action"],
    assertion_requires: [],
    action_requires: ["may_use_for_action"],
    allow_sensitive: ["public", "internal"],
    render_mode: "strict",
    require_current: true,
    requires_assertion_authority: true,
  },
  external_share: {
    id: "external_share",
    risk: "high",
    allow_grades_in_prompt: ["trusted"],
    require_permissions: ["may_retrieve", "may_quote"],
    assertion_requires: ["may_quote"],
    action_requires: [],
    allow_sensitive: ["public"],
    render_mode: "strict",
    require_current: true,
    requires_assertion_authority: true,
  },
};

export function getPurposePolicy(id: BuiltInPurpose): PurposePolicy {
  return clonePolicy(PURPOSE_POLICIES[id]);
}

export function createCustomPurposePolicy(args: {
  id: string;
  base: PurposePolicy;
  changes: Partial<PurposePolicy>;
  override?: PolicyOverrideAuthorization;
  now?: string;
}): { policy: PurposePolicy; overrideReceipt: PolicyOverrideReceipt | null } {
  const widenedRules = collectWideningRules(args.base, args.changes);
  const derivedAt = args.now ?? new Date().toISOString();
  const allowWidening = Boolean(args.override);
  const allowedChanges = allowWidening ? args.changes : omitWideningChanges(args.base, args.changes);
  const policy = {
    ...args.base,
    ...allowedChanges,
    id: args.id,
    allow_grades_in_prompt:
      allowedChanges.allow_grades_in_prompt?.slice() ?? args.base.allow_grades_in_prompt.slice(),
    require_permissions: allowedChanges.require_permissions?.slice() ?? args.base.require_permissions.slice(),
    assertion_requires: allowedChanges.assertion_requires?.slice() ?? args.base.assertion_requires.slice(),
    action_requires: allowedChanges.action_requires?.slice() ?? args.base.action_requires.slice(),
    allow_sensitive: allowedChanges.allow_sensitive?.slice() ?? args.base.allow_sensitive.slice(),
  };

  return {
    policy,
    overrideReceipt:
      allowWidening && widenedRules.length > 0
        ? {
            receipt_id: `policy_override:${args.id}:${derivedAt}`,
            purpose: args.id,
            derived_at: derivedAt,
            rules: widenedRules.map((rule) => ({
              ...rule,
              source: args.override?.id ?? rule.source,
              reason: `${rule.reason}; override approved by ${args.override?.approved_by}`,
            })),
          }
        : null,
  };
}

function clonePolicy(policy: PurposePolicy): PurposePolicy {
  return {
    ...policy,
    allow_grades_in_prompt: policy.allow_grades_in_prompt.slice(),
    require_permissions: policy.require_permissions.slice(),
    assertion_requires: policy.assertion_requires.slice(),
    action_requires: policy.action_requires.slice(),
    allow_sensitive: policy.allow_sensitive.slice(),
  };
}

function collectWideningRules(base: PurposePolicy, changes: Partial<PurposePolicy>): DerivationStep[] {
  const rules: DerivationStep[] = [];

  if (changes.allow_grades_in_prompt && hasAdded(base.allow_grades_in_prompt, changes.allow_grades_in_prompt)) {
    rules.push({
      field: "allow_grades_in_prompt",
      value: changes.allow_grades_in_prompt,
      source: "custom_policy",
      reason: "custom purpose adds prompt grades beyond the base policy",
      policy_rule: "custom_purpose.no_widening_without_override",
    });
  }

  if (changes.allow_sensitive && hasAdded(base.allow_sensitive, changes.allow_sensitive)) {
    rules.push({
      field: "allow_sensitive",
      value: changes.allow_sensitive,
      source: "custom_policy",
      reason: "custom purpose allows additional sensitivity levels",
      policy_rule: "custom_purpose.no_widening_without_override",
    });
  }

  if (changes.require_permissions && hasRemoved(base.require_permissions, changes.require_permissions)) {
    rules.push({
      field: "require_permissions",
      value: changes.require_permissions,
      source: "custom_policy",
      reason: "custom purpose removes required permissions",
      policy_rule: "custom_purpose.no_widening_without_override",
    });
  }

  if (changes.assertion_requires && hasRemoved(base.assertion_requires, changes.assertion_requires)) {
    rules.push({
      field: "assertion_requires",
      value: changes.assertion_requires,
      source: "custom_policy",
      reason: "custom purpose removes assertion requirements",
      policy_rule: "custom_purpose.no_widening_without_override",
    });
  }

  if (changes.action_requires && hasRemoved(base.action_requires, changes.action_requires)) {
    rules.push({
      field: "action_requires",
      value: changes.action_requires,
      source: "custom_policy",
      reason: "custom purpose removes action requirements",
      policy_rule: "custom_purpose.no_widening_without_override",
    });
  }

  if (changes.render_mode && isWeakerRenderMode(base.render_mode, changes.render_mode)) {
    rules.push({
      field: "render_mode",
      value: changes.render_mode,
      source: "custom_policy",
      reason: "custom purpose weakens prompt rendering mode",
      policy_rule: "custom_purpose.no_widening_without_override",
    });
  }

  if (changes.require_current === false && base.require_current) {
    rules.push({
      field: "require_current",
      value: changes.require_current,
      source: "custom_policy",
      reason: "custom purpose removes freshness requirement",
      policy_rule: "custom_purpose.no_widening_without_override",
    });
  }

  if (changes.requires_assertion_authority === false && base.requires_assertion_authority) {
    rules.push({
      field: "requires_assertion_authority",
      value: changes.requires_assertion_authority,
      source: "custom_policy",
      reason: "custom purpose removes assertion authority requirement",
      policy_rule: "custom_purpose.no_widening_without_override",
    });
  }

  return rules;
}

function omitWideningChanges(base: PurposePolicy, changes: Partial<PurposePolicy>): Partial<PurposePolicy> {
  const allowed = { ...changes };

  if (changes.allow_grades_in_prompt && hasAdded(base.allow_grades_in_prompt, changes.allow_grades_in_prompt)) {
    delete allowed.allow_grades_in_prompt;
  }
  if (changes.allow_sensitive && hasAdded(base.allow_sensitive, changes.allow_sensitive)) {
    delete allowed.allow_sensitive;
  }
  if (changes.require_permissions && hasRemoved(base.require_permissions, changes.require_permissions)) {
    delete allowed.require_permissions;
  }
  if (changes.assertion_requires && hasRemoved(base.assertion_requires, changes.assertion_requires)) {
    delete allowed.assertion_requires;
  }
  if (changes.action_requires && hasRemoved(base.action_requires, changes.action_requires)) {
    delete allowed.action_requires;
  }
  if (changes.render_mode && isWeakerRenderMode(base.render_mode, changes.render_mode)) {
    delete allowed.render_mode;
  }
  if (changes.require_current === false && base.require_current) {
    delete allowed.require_current;
  }
  if (changes.requires_assertion_authority === false && base.requires_assertion_authority) {
    delete allowed.requires_assertion_authority;
  }

  return allowed;
}

function hasAdded<T extends Grade | Sensitivity>(base: T[], next: T[]) {
  const baseSet = new Set(base);
  return next.some((value) => !baseSet.has(value));
}

function hasRemoved<T extends PermissionKey>(base: T[], next: T[]) {
  const nextSet = new Set(next);
  return base.some((value) => !nextSet.has(value));
}

function isWeakerRenderMode(base: RenderMode, next: RenderMode) {
  return RENDER_MODE_ORDER.indexOf(next) > RENDER_MODE_ORDER.indexOf(base);
}

export function isGradePromptWidening(base: Grade, next: Grade) {
  return PROMPT_GRADE_ORDER.indexOf(next) > PROMPT_GRADE_ORDER.indexOf(base);
}

export function isSensitivityWidening(base: Sensitivity, next: Sensitivity) {
  return SENSITIVITY_ORDER.indexOf(next) > SENSITIVITY_ORDER.indexOf(base);
}
