import { getPurposePolicy, BUILT_IN_PURPOSES, type BuiltInPurpose } from "./policies";
import type { DerivationStep, Grade, PurposePolicy, RenderMode, Sensitivity } from "./types";

/**
 * User posture — the Wizard's single "Work / Play / Private" choice.
 *
 * A posture is a global stance the user picks during onboarding (and can change
 * any time from the settings hub). It is *sugar over the existing governance
 * primitives*, never a new authority of its own.
 *
 * NON-NEGOTIABLE DESIGN RULE (AGENTS.md laws 4 & 7): a posture may only ever
 * *tighten* a purpose policy, never widen it. Widening authorization requires an
 * explicit override receipt and cannot ride in on a UX preference. This module is
 * therefore built so the authorization resolver is *structurally incapable* of
 * widening — it only intersects grade/sensitivity sets and strengthens render
 * mode. Empty result = fail closed (maximally tight), which is safe.
 *
 * "Play" feels low-friction NOT by relaxing the trust boundary but through a
 * separate INTERACTION layer (confirmation cadence, notification verbosity) that
 * never changes *whether* something is authorized — only how often the user is
 * interrupted about things already authorized. High/medium-risk purposes always
 * confirm, in every posture.
 */
export const USER_POSTURES = ["private", "work", "play"] as const;
export type UserPosture = (typeof USER_POSTURES)[number];

export const DEFAULT_USER_POSTURE: UserPosture = "work";

export function isUserPosture(value: unknown): value is UserPosture {
  return typeof value === "string" && (USER_POSTURES as readonly string[]).includes(value);
}

// Strongest-first. A posture may strengthen render mode (move left), never weaken it.
const RENDER_MODE_STRENGTH: RenderMode[] = ["strict", "clean", "annotated"];

function strongerRenderMode(a: RenderMode, b: RenderMode): RenderMode {
  return RENDER_MODE_STRENGTH.indexOf(a) <= RENDER_MODE_STRENGTH.indexOf(b) ? a : b;
}

function intersect<T>(base: T[], allowed: readonly T[]): T[] {
  const allow = new Set(allowed);
  return base.filter((value) => allow.has(value));
}

/**
 * Authorization clamp a posture applies to *every* purpose. Only tightening keys.
 * `undefined` means "leave the purpose's own value untouched".
 */
interface PostureClamp {
  grades?: readonly Grade[];
  sensitivity?: readonly Sensitivity[];
  render_mode?: RenderMode;
  require_current?: true;
  requires_assertion_authority?: true;
}

const POSTURE_CLAMPS: Record<UserPosture, PostureClamp> = {
  // Tightest. Nothing leaves without being trusted + public; show the least;
  // demand freshness and assertion authority everywhere.
  private: {
    grades: ["trusted"],
    sensitivity: ["public"],
    render_mode: "strict",
    require_current: true,
    requires_assertion_authority: true,
  },
  // Balanced baseline — the built-in purpose policies as authored. No clamp.
  work: {},
  // Same authorization as Work (see module doc — friction differs, not trust).
  play: {},
};

export interface PostureResolution {
  posture: UserPosture;
  purpose: BuiltInPurpose;
  policy: PurposePolicy;
  /** Every field the posture actually tightened, for the receipt trail. */
  clamps: DerivationStep[];
}

/**
 * Resolve the effective policy for a purpose under a posture. The result is the
 * built-in policy intersected with the posture clamp — only ever narrower.
 */
export function resolvePosturePolicy(posture: UserPosture, purpose: BuiltInPurpose): PostureResolution {
  const policy = getPurposePolicy(purpose); // already a deep clone
  const clamp = POSTURE_CLAMPS[posture];
  const clamps: DerivationStep[] = [];

  const record = (field: string, value: unknown, reason: string) =>
    clamps.push({ field, value, source: `user_posture:${posture}`, reason, policy_rule: "user_posture.tighten_only" });

  if (clamp.grades) {
    const next = intersect(policy.allow_grades_in_prompt, clamp.grades);
    if (next.length !== policy.allow_grades_in_prompt.length) {
      record("allow_grades_in_prompt", next, `${posture} posture restricts prompt grades`);
      policy.allow_grades_in_prompt = next;
    }
  }

  if (clamp.sensitivity) {
    const next = intersect(policy.allow_sensitive, clamp.sensitivity);
    if (next.length !== policy.allow_sensitive.length) {
      record("allow_sensitive", next, `${posture} posture restricts sensitivity`);
      policy.allow_sensitive = next;
    }
  }

  if (clamp.render_mode) {
    const next = strongerRenderMode(policy.render_mode, clamp.render_mode);
    if (next !== policy.render_mode) {
      record("render_mode", next, `${posture} posture strengthens render mode`);
      policy.render_mode = next;
    }
  }

  if (clamp.require_current && !policy.require_current) {
    record("require_current", true, `${posture} posture requires freshness`);
    policy.require_current = true;
  }

  if (clamp.requires_assertion_authority && !policy.requires_assertion_authority) {
    record("requires_assertion_authority", true, `${posture} posture requires assertion authority`);
    policy.requires_assertion_authority = true;
  }

  return { posture, purpose, policy, clamps };
}

/** Resolve every built-in purpose under a posture — the full effective posture. */
export function resolvePosture(posture: UserPosture): PostureResolution[] {
  return BUILT_IN_PURPOSES.map((purpose) => resolvePosturePolicy(posture, purpose));
}

// --- interaction layer (NOT authorization — never gates whether, only how often) ---

export type NotificationVerbosity = "quiet" | "normal" | "chatty";

export interface InteractionPosture {
  posture: UserPosture;
  /** Confirm even low-risk, already-authorized actions before they run. */
  confirm_low_risk_actions: boolean;
  notification_verbosity: NotificationVerbosity;
  /** Collapse buddy chrome to stay out of the way. */
  auto_collapse_chrome: boolean;
}

const INTERACTION_POSTURES: Record<UserPosture, Omit<InteractionPosture, "posture">> = {
  private: { confirm_low_risk_actions: true, notification_verbosity: "quiet", auto_collapse_chrome: true },
  work: { confirm_low_risk_actions: true, notification_verbosity: "normal", auto_collapse_chrome: false },
  play: { confirm_low_risk_actions: false, notification_verbosity: "chatty", auto_collapse_chrome: false },
};

export function getInteractionPosture(posture: UserPosture): InteractionPosture {
  return { posture, ...INTERACTION_POSTURES[posture] };
}

/**
 * Hard confirmation floor. Medium/high-risk purposes ALWAYS confirm, in every
 * posture — posture can only waive confirmation on low-risk, already-authorized
 * actions. This is the line that keeps "Play" from becoming a security hole.
 */
export function requiresConfirmation(posture: UserPosture, risk: PurposePolicy["risk"]): boolean {
  if (risk === "high" || risk === "medium") return true;
  return getInteractionPosture(posture).confirm_low_risk_actions;
}
