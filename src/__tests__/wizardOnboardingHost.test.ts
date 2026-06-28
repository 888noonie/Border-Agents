import { describe, it, expect } from "vitest";

import {
  INITIAL_ONBOARDING_STATE,
  ONBOARDING_ACTS,
  currentAct,
  type OnboardingEvent,
  type OnboardingState,
} from "../wizardOnboarding";
import { actCues, actPanel, onHostEvent } from "../wizardOnboardingHost";
import { applyPanelChoices, createWizardHostDraft } from "../wizardHostDraft";

// The advancing event each act listens for, in order — the canonical "happy path"
// walk a Host would receive (Act 4 self-advances on a timeout the soul fires).
const WALK: readonly OnboardingEvent[] = [
  "clicked", // first_contact → connect_engine
  "panel:connection_ok", // connect_engine → choose_posture
  "panel:posture_set", // choose_posture → place_buddies
  "panel:next", // place_buddies → find_me
  "timeout", // find_me → done
  "panel:done", // done → completed
];

describe("actCues", () => {
  it("presents the current act's authored cues", () => {
    expect(actCues(INITIAL_ONBOARDING_STATE)).toEqual(ONBOARDING_ACTS[0].cues);
    const act2: OnboardingState = { actIndex: 2, completed: false };
    expect(actCues(act2)).toEqual(ONBOARDING_ACTS[2].cues);
  });

  it("opens with a greeting that attends the user", () => {
    const cues = actCues(INITIAL_ONBOARDING_STATE);
    expect(cues.some((cue) => cue.kind === "attention" && cue.focus === "user")).toBe(true);
    expect(cues.some((cue) => cue.kind === "say")).toBe(true);
  });

  it("clamps a past-the-end index to the final act instead of throwing", () => {
    const overrun: OnboardingState = { actIndex: 99, completed: true };
    expect(actCues(overrun)).toEqual(ONBOARDING_ACTS[ONBOARDING_ACTS.length - 1].cues);
  });
});

describe("actPanel", () => {
  const stateAt = (actIndex: number): OnboardingState => ({ actIndex, completed: false });

  it("emits the right section + primary token for each act of the walk", () => {
    // The native body advances each form act on `clicked{panel: primaryPanel}`; the token the
    // Host stamps here must match the act's `advanceOn`, or the body could never advance it.
    const sections = ONBOARDING_ACTS.map((_, i) => {
      const panel = actPanel(stateAt(i), []);
      return { section: panel.section, primary: panel.primaryPanel };
    });
    expect(sections).toEqual([
      { section: "none", primary: undefined }, // first_contact
      { section: "connect", primary: "connection_ok" },
      { section: "posture", primary: "posture_set" },
      { section: "placement", primary: "next" },
      { section: "none", primary: undefined }, // find_me
      { section: "summary", primary: "done" },
    ]);
    // Every form section's primary token must be an event the act actually advances on.
    ONBOARDING_ACTS.forEach((act, i) => {
      const token = actPanel(stateAt(i), []).primaryPanel;
      if (token) expect(act.advanceOn).toContain(`panel:${token}` as OnboardingEvent);
    });
  });

  it("offers a masked paste_key credential field on the connect section", () => {
    const connect = actPanel(stateAt(1), []);
    expect(connect.options?.length).toBeGreaterThan(0);
    const key = connect.fields?.find((f) => f.key === "apiKey");
    expect(key?.control).toBe("paste_key");
    expect(key?.masked).toBe(true);
  });

  it("reflects the wizard draft in connect/posture panel highlights", () => {
    const draft = applyPanelChoices(createWizardHostDraft(), "connection_ok", {
      selectedOptionIds: ["ollama"],
      fieldValues: { model: "llama3.1" },
    });
    const connect = actPanel(stateAt(1), [], draft);
    expect(connect.options?.find((option) => option.id === "ollama")?.selected).toBe(true);
    expect(connect.fields?.find((field) => field.key === "model")?.value).toBe("llama3.1");

    const postureDraft = applyPanelChoices(createWizardHostDraft(), "posture_set", {
      selectedOptionIds: ["private"],
    });
    const posture = actPanel(stateAt(2), [], postureDraft);
    expect(posture.options?.find((option) => option.id === "private")?.selected).toBe(true);
  });

  it("marks summary rows recorded as their lifecycle receipts land", () => {
    const summaryIndex = ONBOARDING_ACTS.findIndex((a) => a.panelSection === "summary");
    const before = actPanel(stateAt(summaryIndex), []);
    expect(before.rows?.every((r) => r.status === "pending")).toBe(true);
    const after = actPanel(stateAt(summaryIndex), ["credential.stored", "posture.set", "placement.set"]);
    const recorded = after.rows?.filter((r) => r.status === "recorded").length ?? 0;
    expect(recorded).toBe(3);
  });
});

describe("onHostEvent", () => {
  it("advances and earns the act's receipt only on the matching event", () => {
    const result = onHostEvent(INITIAL_ONBOARDING_STATE, "clicked");
    expect(result.next.actIndex).toBe(1);
    expect(result.receipt).toBeNull(); // first_contact carries no receipt
    expect(result.completedNow).toBe(false);
  });

  it("is a no-op on an unrelated event (same state, no receipt)", () => {
    const result = onHostEvent(INITIAL_ONBOARDING_STATE, "dropped");
    expect(result.next).toBe(INITIAL_ONBOARDING_STATE); // reference-equal
    expect(result.receipt).toBeNull();
    expect(result.completedNow).toBe(false);
  });

  it("records the receipt a value-bearing act earns", () => {
    const onConnect: OnboardingState = { actIndex: 1, completed: false };
    expect(onHostEvent(onConnect, "panel:connection_ok").receipt).toBe("credential.stored");
  });

  it("yields the exact receipt sequence across a full walk", () => {
    let state = INITIAL_ONBOARDING_STATE;
    const receipts: (string | null)[] = [];
    for (const event of WALK) {
      const result = onHostEvent(state, event);
      receipts.push(result.receipt);
      state = result.next;
    }
    expect(receipts).toEqual([
      null,
      "credential.stored",
      "posture.set",
      "placement.set",
      null,
      "onboarding.completed",
    ]);
    expect(state.completed).toBe(true);
  });

  it("flags completedNow exactly once — on the final advance", () => {
    let state = INITIAL_ONBOARDING_STATE;
    const flags: boolean[] = [];
    for (const event of WALK) {
      const result = onHostEvent(state, event);
      flags.push(result.completedNow);
      state = result.next;
    }
    expect(flags).toEqual([false, false, false, false, false, true]);
  });

  it("is idempotent past completion — no further receipts or handoff", () => {
    const done: OnboardingState = {
      actIndex: ONBOARDING_ACTS.length - 1,
      completed: true,
    };
    const again = onHostEvent(done, "panel:done");
    expect(again.next).toBe(done); // reference-equal, no churn
    expect(again.receipt).toBeNull();
    expect(again.completedNow).toBe(false);
    // ...and still parked on the final act.
    expect(currentAct(again.next).id).toBe("done");
  });
});
