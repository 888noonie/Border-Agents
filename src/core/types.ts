export type Grade = "trusted" | "limited" | "reference_only" | "blocked" | "quarantined";

export type Sensitivity = "public" | "internal" | "confidential" | "restricted";

export type ClaimType =
  | "observed_fact"
  | "current_policy"
  | "historical_summary"
  | "preference"
  | "instruction"
  | "unverified_claim";

export type Authority = "low" | "medium" | "high";

export type RenderMode = "clean" | "annotated" | "strict";

export type PermissionKey = keyof MemoryPermissions;

export interface MemoryPermissions {
  may_retrieve: boolean;
  may_quote: boolean;
  may_assert: boolean;
  may_use_for_action: boolean;
  requires_verification_before_assertion: boolean;
}

export interface MemorySource {
  type: "repo_file" | "chat_session" | "user_note" | "system_note" | "external_document";
  id: string;
  created_at: string;
}

export interface DerivationStep {
  field: string;
  value: unknown;
  source: string;
  reason: string;
  policy_rule: string;
}

export interface MemoryPacket {
  packet_id: string;
  content_hash: string;
  source: MemorySource;
  claim_type: ClaimType;
  authority: Authority;
  sensitivity: Sensitivity;
  valid_until: string | null;
  permissions: MemoryPermissions;
  labels: string[];
  policy: {
    id: string;
    version: string;
  };
  derivation: DerivationStep[];
  review: {
    mode: RenderMode;
    requires_review: boolean;
    reviewed_by: string | null;
    reviewed_at: string | null;
  };
}

export interface RetrievedMemory {
  chunk_id: string;
  text: string;
  score: number;
  packet: MemoryPacket;
}

export interface PurposePolicy {
  id: string;
  risk: "low" | "medium" | "high";
  allow_grades_in_prompt: Grade[];
  require_permissions: PermissionKey[];
  assertion_requires: PermissionKey[];
  action_requires: PermissionKey[];
  allow_sensitive: Sensitivity[];
  render_mode: RenderMode;
  require_current: boolean;
  requires_assertion_authority: boolean;
}

export interface GradeReceipt {
  receipt_id: string;
  packet_id: string;
  chunk_id: string;
  purpose: string;
  grade: Grade;
  derived_at: string;
  rules: DerivationStep[];
}

export type FrameBuckets = Record<Grade, RetrievedMemory[]>;

export interface SafeContextFrame extends FrameBuckets {
  purpose: string;
  receipts: GradeReceipt[];
}

export interface PromptRenderResult {
  mode: RenderMode;
  purpose: string;
  context: string;
  included: Array<{
    chunk_id: string;
    packet_id: string;
    grade: Grade;
  }>;
  excluded: Array<{
    chunk_id: string;
    packet_id: string;
    grade: Grade;
    reason: string;
  }>;
}

export interface PolicyOverrideReceipt {
  receipt_id: string;
  purpose: string;
  derived_at: string;
  rules: DerivationStep[];
}

export interface PolicyOverrideAuthorization {
  id: string;
  reason: string;
  approved_by: string;
  approved_at: string;
}
