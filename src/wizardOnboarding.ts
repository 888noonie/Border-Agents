// Wizard onboarding — the canonical, typed script + advance reducer.
//
// This is the SINGLE source of truth for the onboarding flow described in
// docs/WIZARD_ONBOARDING_SCRIPT.md. The React settings panel renders these acts;
// the soul-side Host driver (today: scripts/gateway-dev.mjs under BB_SOUL=wizard)
// emits each act's body cues and advances on the matching to-soul event.
//
// Law 7 holds here too: an act's `cues` are things the body PRESENTS (move_to /
// express / say / attention). The body never decides which act it's in — the Host
// soul runs this reducer and pushes cues. The body only reports the raw pointer
// events (`clicked`/`dropped`/...) that advance it.
//
// Acts 1–5 also name a real config target and a governance receipt; those are wired
// in their own commits. Act 0 ("first contact") writes no state and is fully live.

import type { PresenceEmotion, PresenceFocus, PresencePosition } from "./presenceProtocol";

// ---------------------------------------------------------------------------
// Body cues an act emits (a typed subset of the to-body protocol)
// ---------------------------------------------------------------------------

export type OnboardingCue =
  | { kind: "move_to"; position: PresencePosition }
  | { kind: "express"; emotion: PresenceEmotion; intensity?: number }
  | { kind: "say"; text: string }
  | { kind: "attention"; focus: PresenceFocus };

// ---------------------------------------------------------------------------
// Advancing events — what moves the flow forward
// ---------------------------------------------------------------------------

// Either a raw to-soul pointer event from the body, or a "panel:<name>" signal the
// settings panel sends when a form section is satisfied. `timeout` lets a no-form act
// (Act 4) advance on its own.
export type OnboardingEvent =
  | "clicked"
  | "grabbed"
  | "dragged"
  | "dropped"
  | "summoned"
  | "dismissed"
  | "timeout"
  | `panel:${string}`;

// ---------------------------------------------------------------------------
// Acts
// ---------------------------------------------------------------------------

export type OnboardingActId =
  | "first_contact"
  | "connect_engine"
  | "choose_posture"
  | "place_buddies"
  | "find_me"
  | "done";

export type OnboardingAct = {
  id: OnboardingActId;
  title: string;
  // Which settings-panel section this act drives, if any. Act 0 and Act 4 are
  // "no forms" — the panel stays closed / shows only the body.
  panelSection: "none" | "connect" | "posture" | "placement" | "summary";
  // Body cues the Host emits when the act begins, in order.
  cues: OnboardingCue[];
  // Events that advance to the next act. Any one of them is sufficient.
  advanceOn: OnboardingEvent[];
  // Governance receipt this act writes on completion (via receiptLedger), or null.
  receipt: string | null;
};

const RIGHT_EDGE: PresencePosition = { mode: "anchored", edge: "right", offset: { x: 24, y: 48 } };

// The script. Act 0 is fully authored and live; Acts 1–5 carry their cues, panel
// section, advancing events, and receipt names so the panel + later commits slot in
// without re-deciding the flow.
export const ONBOARDING_ACTS: readonly OnboardingAct[] = [
  {
    id: "first_contact",
    title: "First contact",
    panelSection: "none",
    cues: [
      { kind: "move_to", position: RIGHT_EDGE },
      { kind: "express", emotion: "curious" },
      { kind: "attention", focus: "user" },
      { kind: "say", text: "Hi — I'm your setup host. Two minutes to get you wired up. Ready?" },
    ],
    advanceOn: ["clicked"],
    receipt: null,
  },
  {
    id: "connect_engine",
    title: "Connect your engine",
    panelSection: "connect",
    cues: [{ kind: "express", emotion: "thinking" }],
    advanceOn: ["panel:connection_ok"],
    receipt: "credential.stored",
  },
  {
    id: "choose_posture",
    title: "Choose your posture",
    panelSection: "posture",
    cues: [{ kind: "express", emotion: "neutral" }],
    advanceOn: ["panel:posture_set"],
    receipt: "posture.set",
  },
  {
    id: "place_buddies",
    title: "Place your buddies",
    panelSection: "placement",
    cues: [{ kind: "express", emotion: "curious" }],
    advanceOn: ["dropped", "panel:next"],
    receipt: "placement.set",
  },
  {
    id: "find_me",
    title: "Where to find me",
    panelSection: "none",
    cues: [
      { kind: "say", text: "Tug me out whenever you need me. Click me to reopen this anytime." },
    ],
    advanceOn: ["dropped", "timeout"],
    receipt: null,
  },
  {
    id: "done",
    title: "Done & handoff",
    panelSection: "summary",
    cues: [
      { kind: "express", emotion: "happy" },
      { kind: "say", text: "You're set. Bringing in your companion now." },
    ],
    advanceOn: ["panel:done"],
    receipt: "onboarding.completed",
  },
];

export const ONBOARDING_RECEIPT_DONE = "onboarding.completed";

// ---------------------------------------------------------------------------
// State + reducer (pure)
// ---------------------------------------------------------------------------

export type OnboardingState = {
  actIndex: number;
  completed: boolean;
};

export const INITIAL_ONBOARDING_STATE: OnboardingState = { actIndex: 0, completed: false };

export function currentAct(state: OnboardingState): OnboardingAct {
  return ONBOARDING_ACTS[Math.min(state.actIndex, ONBOARDING_ACTS.length - 1)];
}

export function isLastAct(state: OnboardingState): boolean {
  return state.actIndex >= ONBOARDING_ACTS.length - 1;
}

// Advance the flow if `event` satisfies the current act. Unrelated events are a
// no-op (returns the same state), so the Host can pass every body event through
// without filtering. Advancing past the last act marks onboarding completed.
export function advanceOnboarding(state: OnboardingState, event: OnboardingEvent): OnboardingState {
  if (state.completed) {
    return state;
  }
  const act = currentAct(state);
  if (!act.advanceOn.includes(event)) {
    return state;
  }
  if (isLastAct(state)) {
    return { actIndex: state.actIndex, completed: true };
  }
  return { actIndex: state.actIndex + 1, completed: false };
}

// ---------------------------------------------------------------------------
// Re-entry — the "go-to" contract
// ---------------------------------------------------------------------------

// First run is a linear script; every run after opens the same panel as a settings
// hub. The decision is purely "has the user ever completed onboarding?", recorded by
// the presence of an `onboarding.completed` receipt.
export function isFirstRun(receiptKinds: readonly string[]): boolean {
  return !receiptKinds.includes(ONBOARDING_RECEIPT_DONE);
}

export type OnboardingEntryMode = "linear" | "hub";

export function entryMode(receiptKinds: readonly string[]): OnboardingEntryMode {
  return isFirstRun(receiptKinds) ? "linear" : "hub";
}
