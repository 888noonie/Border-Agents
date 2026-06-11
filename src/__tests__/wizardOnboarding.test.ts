import { describe, expect, it } from "vitest";
import {
  INITIAL_ONBOARDING_STATE,
  ONBOARDING_ACTS,
  ONBOARDING_RECEIPT_DONE,
  advanceOnboarding,
  currentAct,
  entryMode,
  isFirstRun,
  isLastAct,
  type OnboardingState,
} from "../wizardOnboarding";
import { parsePresenceMessage } from "../presenceProtocol";

describe("wizard onboarding script", () => {
  it("opens on Act 0 — first contact, no forms, no receipt", () => {
    const act = currentAct(INITIAL_ONBOARDING_STATE);
    expect(act.id).toBe("first_contact");
    expect(act.panelSection).toBe("none");
    expect(act.receipt).toBeNull();
    expect(act.advanceOn).toContain("clicked");
  });

  it("Act 0 cues are valid to-body presence messages", () => {
    // Every Act 0 cue must round-trip as a real presence envelope the body accepts —
    // the Host can't invent a cue the protocol doesn't carry.
    for (const cue of ONBOARDING_ACTS[0].cues) {
      const envelope = {
        protocol: "presence",
        v: 0,
        kind: cue.kind,
        buddy: "host",
        ts: Date.now(),
        ...cueProps(cue),
      };
      expect(parsePresenceMessage(envelope), `cue ${cue.kind} invalid`).not.toBeNull();
    }
  });

  it("a click advances Act 0 → Act 1 (connect)", () => {
    const next = advanceOnboarding(INITIAL_ONBOARDING_STATE, "clicked");
    expect(currentAct(next).id).toBe("connect_engine");
    expect(currentAct(next).panelSection).toBe("connect");
  });

  it("ignores events the current act does not listen for", () => {
    expect(advanceOnboarding(INITIAL_ONBOARDING_STATE, "dropped")).toEqual(
      INITIAL_ONBOARDING_STATE,
    );
    expect(advanceOnboarding(INITIAL_ONBOARDING_STATE, "panel:posture_set")).toEqual(
      INITIAL_ONBOARDING_STATE,
    );
  });

  it("walks the full script to completion via each act's advancing event", () => {
    let state: OnboardingState = INITIAL_ONBOARDING_STATE;
    const path = ["clicked", "panel:connection_ok", "panel:posture_set", "dropped", "dropped", "panel:done"] as const;
    const visited: string[] = [currentAct(state).id];
    for (const event of path) {
      state = advanceOnboarding(state, event);
      visited.push(currentAct(state).id);
    }
    expect(state.completed).toBe(true);
    expect(visited).toEqual([
      "first_contact",
      "connect_engine",
      "choose_posture",
      "place_buddies",
      "find_me",
      "done",
      "done",
    ]);
  });

  it("is idempotent once completed", () => {
    const done: OnboardingState = { actIndex: ONBOARDING_ACTS.length - 1, completed: true };
    expect(advanceOnboarding(done, "panel:done")).toBe(done);
    expect(isLastAct(done)).toBe(true);
  });

  it("re-entry switches from linear to hub once onboarding is recorded", () => {
    expect(isFirstRun([])).toBe(true);
    expect(entryMode([])).toBe("linear");
    expect(isFirstRun(["credential.stored"])).toBe(true);
    expect(entryMode([ONBOARDING_RECEIPT_DONE])).toBe("hub");
    expect(isFirstRun([ONBOARDING_RECEIPT_DONE])).toBe(false);
  });

  it("only the final act writes the completion receipt", () => {
    const withDone = ONBOARDING_ACTS.filter((a) => a.receipt === ONBOARDING_RECEIPT_DONE);
    expect(withDone).toHaveLength(1);
    expect(withDone[0].id).toBe("done");
  });
});

function cueProps(cue: (typeof ONBOARDING_ACTS)[number]["cues"][number]) {
  switch (cue.kind) {
    case "move_to":
      return { position: cue.position };
    case "express":
      return cue.intensity === undefined
        ? { emotion: cue.emotion }
        : { emotion: cue.emotion, intensity: cue.intensity };
    case "say":
      return { text: cue.text };
    case "attention":
      return { focus: cue.focus };
  }
}
