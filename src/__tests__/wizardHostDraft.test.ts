import { describe, expect, it } from "vitest";

import { DEFAULT_USER_POSTURE } from "../core/userPosture";
import {
  applyPanelChoices,
  createWizardHostDraft,
  receiptDetailForAct,
} from "../wizardHostDraft";

describe("wizardHostDraft", () => {
  it("applies connect choices without echoing the api key in receipt detail", () => {
    const draft = applyPanelChoices(createWizardHostDraft(), "connection_ok", {
      selectedOptionIds: ["ollama"],
      fieldValues: { apiKey: "secret-key", model: "llama3.1" },
    });
    expect(draft.provider).toBe("ollama");
    expect(draft.apiBase).toContain("11434");
    expect(draft.model).toBe("llama3.1");
    expect(draft.apiKey).toBe("secret-key");

    const detail = receiptDetailForAct("credential.stored", draft);
    expect(detail?.apiKeyPresent).toBe(true);
    expect(detail).not.toHaveProperty("apiKey");
    expect(JSON.stringify(detail)).not.toContain("secret-key");
  });

  it("applies posture_set with tighten-only posture ids", () => {
    const draft = applyPanelChoices(createWizardHostDraft(), "posture_set", {
      selectedOptionIds: ["private"],
    });
    expect(draft.posture).toBe("private");
    expect(receiptDetailForAct("posture.set", draft)).toEqual({ posture: "private" });
  });

  it("leaves draft unchanged when choices are absent", () => {
    const base = createWizardHostDraft();
    expect(applyPanelChoices(base, "posture_set")).toBe(base);
    expect(base.posture).toBe(DEFAULT_USER_POSTURE);
  });

  it("applies placement multi-select as enabled buddy ids", () => {
    const draft = applyPanelChoices(createWizardHostDraft(), "next", {
      selectedOptionIds: ["hermes", "owl"],
    });
    expect(draft.enabledBuddyIds).toEqual(["hermes", "owl"]);
  });

  it("keeps prior enabled buddies when placement confirm carries an empty selection", () => {
    const prior = applyPanelChoices(createWizardHostDraft(), "next", {
      selectedOptionIds: ["hermes", "owl"],
    });
    const again = applyPanelChoices(prior, "next", { selectedOptionIds: [] });
    expect(again.enabledBuddyIds).toEqual(["hermes", "owl"]);
  });
});