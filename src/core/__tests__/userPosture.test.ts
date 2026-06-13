import { describe, expect, test } from "vitest";
import { BUILT_IN_PURPOSES, getPurposePolicy } from "../policies";
import {
  DEFAULT_USER_POSTURE,
  USER_POSTURES,
  getInteractionPosture,
  isUserPosture,
  requiresConfirmation,
  resolvePosture,
  resolvePosturePolicy,
} from "../userPosture";
import type { Grade, RenderMode, Sensitivity } from "../types";

const GRADE_PERMISSIVENESS: Grade[] = ["trusted", "limited", "reference_only", "blocked", "quarantined"];
const SENSITIVITY_PERMISSIVENESS: Sensitivity[] = ["public", "internal", "confidential", "restricted"];
const RENDER_STRENGTH: RenderMode[] = ["strict", "clean", "annotated"]; // strongest first

function isSubset<T>(subset: T[], superset: T[]): boolean {
  const sup = new Set(superset);
  return subset.every((value) => sup.has(value));
}

describe("UserPosture", () => {
  test("posture ids and guard are consistent", () => {
    expect(USER_POSTURES).toEqual(["private", "work", "play"]);
    expect(DEFAULT_USER_POSTURE).toBe("work");
    expect(isUserPosture("work")).toBe(true);
    expect(isUserPosture("paranoid")).toBe(false);
    expect(isUserPosture(null)).toBe(false);
  });

  // The non-negotiable: a posture may only ever TIGHTEN a purpose policy.
  test("no posture ever widens any purpose policy", () => {
    for (const posture of USER_POSTURES) {
      for (const purpose of BUILT_IN_PURPOSES) {
        const base = getPurposePolicy(purpose);
        const { policy } = resolvePosturePolicy(posture, purpose);

        // Grades / sensitivity can only be a subset of the base (never added to).
        expect(isSubset(policy.allow_grades_in_prompt, base.allow_grades_in_prompt)).toBe(true);
        expect(isSubset(policy.allow_sensitive, base.allow_sensitive)).toBe(true);

        // Render mode can only get stronger or stay equal (never weaker).
        expect(RENDER_STRENGTH.indexOf(policy.render_mode)).toBeLessThanOrEqual(
          RENDER_STRENGTH.indexOf(base.render_mode),
        );

        // Freshness / authority requirements can only be turned on, never off.
        if (base.require_current) expect(policy.require_current).toBe(true);
        if (base.requires_assertion_authority) expect(policy.requires_assertion_authority).toBe(true);

        // Required permissions are never removed (posture doesn't touch them).
        expect(isSubset(base.require_permissions, policy.require_permissions)).toBe(true);
      }
    }
  });

  test("Work posture is the identity baseline (no clamps)", () => {
    for (const purpose of BUILT_IN_PURPOSES) {
      const base = getPurposePolicy(purpose);
      const { policy, clamps } = resolvePosturePolicy("work", purpose);
      expect(clamps).toHaveLength(0);
      expect(policy).toEqual(base);
    }
  });

  test("Play has identical authorization to Work (only friction differs)", () => {
    for (const purpose of BUILT_IN_PURPOSES) {
      const work = resolvePosturePolicy("work", purpose).policy;
      const play = resolvePosturePolicy("play", purpose).policy;
      expect(play).toEqual(work);
    }
  });

  test("Private clamps a low-risk purpose to trusted/public/strict and records each clamp", () => {
    // summarize_history starts wide: trusted+limited grades, public+internal, annotated.
    const base = getPurposePolicy("summarize_history");
    expect(base.allow_grades_in_prompt).toContain("limited");
    expect(base.allow_sensitive).toContain("internal");

    const { policy, clamps } = resolvePosturePolicy("private", "summarize_history");
    expect(policy.allow_grades_in_prompt).toEqual(["trusted"]);
    expect(policy.allow_sensitive).toEqual(["public"]);
    expect(policy.render_mode).toBe("strict");
    expect(policy.require_current).toBe(true);
    expect(policy.requires_assertion_authority).toBe(true);

    // Every tightening is recorded for the receipt trail with a stable rule id.
    const fields = clamps.map((c) => c.field);
    expect(fields).toEqual(
      expect.arrayContaining([
        "allow_grades_in_prompt",
        "allow_sensitive",
        "render_mode",
        "require_current",
        "requires_assertion_authority",
      ]),
    );
    for (const clamp of clamps) {
      expect(clamp.policy_rule).toBe("user_posture.tighten_only");
      expect(clamp.source).toBe("user_posture:private");
    }
  });

  test("Private produces no clamp where a purpose is already at the floor", () => {
    // external_share is already trusted/public/strict — Private changes nothing there.
    const { policy, clamps } = resolvePosturePolicy("private", "external_share");
    const base = getPurposePolicy("external_share");
    expect(policy).toEqual(base);
    expect(clamps).toHaveLength(0);
  });

  test("resolvePosture covers every built-in purpose", () => {
    const resolved = resolvePosture("private");
    expect(resolved.map((r) => r.purpose)).toEqual([...BUILT_IN_PURPOSES]);
  });

  // The hard floor that keeps "Play" from becoming a security hole.
  test("medium/high-risk actions always confirm, in every posture", () => {
    for (const posture of USER_POSTURES) {
      expect(requiresConfirmation(posture, "high")).toBe(true);
      expect(requiresConfirmation(posture, "medium")).toBe(true);
    }
  });

  test("only Play waives confirmation on low-risk actions", () => {
    expect(requiresConfirmation("private", "low")).toBe(true);
    expect(requiresConfirmation("work", "low")).toBe(true);
    expect(requiresConfirmation("play", "low")).toBe(false);
  });

  test("interaction posture is presentation-only and well-formed", () => {
    expect(getInteractionPosture("private")).toEqual({
      posture: "private",
      confirm_low_risk_actions: true,
      notification_verbosity: "quiet",
      auto_collapse_chrome: true,
    });
    expect(getInteractionPosture("play").confirm_low_risk_actions).toBe(false);
  });
});
