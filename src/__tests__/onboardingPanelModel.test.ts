import { describe, expect, it } from "vitest";
import {
  buildOnboardingPanelModel,
  DEFAULT_HERMES_SYSTEM_PROMPT,
  HERMES_PROVIDER_PRESETS,
} from "../onboardingPanelModel";
import type { OnboardingState } from "../wizardOnboarding";

describe("onboarding panel model", () => {
  it("builds the connect section from real Hermes provider presets", () => {
    const model = buildOnboardingPanelModel({
      state: { actIndex: 1, completed: false },
      receiptKinds: [],
    });

    expect(model.mode).toBe("linear");
    expect(model.section?.kind).toBe("connect");
    if (!model.section || model.section.kind !== "connect") {
      throw new Error("expected connect section");
    }
    expect(model.section.providers).toEqual(HERMES_PROVIDER_PRESETS);
    expect(model.section.defaultProvider).toBe("xai");
    expect(model.section.fields.find((field) => field.key === "systemPrompt")?.placeholder).toBe(
      DEFAULT_HERMES_SYSTEM_PROMPT,
    );
  });

  it("marks hosted providers as key-required and local providers as keyless", () => {
    const byId = Object.fromEntries(HERMES_PROVIDER_PRESETS.map((preset) => [preset.id, preset]));

    expect(byId.xai.requiresApiKey).toBe(true);
    expect(byId.openrouter.requiresApiKey).toBe(true);
    expect(byId.lm_studio.requiresApiKey).toBe(false);
    expect(byId.ollama.requiresApiKey).toBe(false);
  });

  it("maps each preset id to a distinct BuddyProvider for settings writes", () => {
    const byId = Object.fromEntries(HERMES_PROVIDER_PRESETS.map((preset) => [preset.id, preset]));

    expect(byId.xai.buddyProvider).toBe("grok");
    expect(byId.openrouter.buddyProvider).toBe("openrouter");
    expect(byId.lm_studio.buddyProvider).toBe("lm_studio");
    expect(byId.ollama.buddyProvider).toBe("ollama");
  });

  it("surfaces posture cards with trust and interaction summaries", () => {
    const model = buildOnboardingPanelModel({
      state: { actIndex: 2, completed: false },
      receiptKinds: [],
    });

    expect(model.section?.kind).toBe("posture");
    if (!model.section || model.section.kind !== "posture") {
      throw new Error("expected posture section");
    }

    const play = model.section.options.find((option) => option.posture === "play");
    const privatePosture = model.section.options.find((option) => option.posture === "private");

    expect(play?.authorizationSummary).toContain("Built-in purpose policies");
    expect(play?.interactionSummary).toContain("without confirm");
    expect(privatePosture?.authorizationSummary).toContain("Trusted-only");
    expect(privatePosture?.interactionSummary).toContain("confirm low-risk actions");
  });

  it("locks future sections during the first linear pass", () => {
    const model = buildOnboardingPanelModel({
      state: { actIndex: 1, completed: false },
      receiptKinds: [],
    });

    expect(model.nav).toEqual([
      { section: "connect", label: "Connect", active: true, enabled: true },
      { section: "posture", label: "Posture", active: false, enabled: false },
      { section: "placement", label: "Placement", active: false, enabled: false },
      { section: "summary", label: "Receipts", active: false, enabled: false },
    ]);
  });

  it("unlocks the full hub once onboarding has completed", () => {
    const state: OnboardingState = { actIndex: 5, completed: true };
    const model = buildOnboardingPanelModel({
      state,
      receiptKinds: ["credential.stored", "posture.set", "placement.set", "onboarding.completed"],
    });

    expect(model.mode).toBe("hub");
    expect(model.nav.every((item) => item.enabled)).toBe(true);
    expect(model.section?.kind).toBe("summary");
    if (!model.section || model.section.kind !== "summary") {
      throw new Error("expected summary section");
    }
    expect(model.section.rows.map((row) => row.status)).toEqual(["recorded", "recorded", "recorded", "recorded"]);
  });
});
