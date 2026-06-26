// Wizard onboarding Host — the soul-side driver of the onboarding script.
//
// This is the pure, testable heart of the Host persona that the real soul
// (scripts/soul-server.ts, under BB_SOUL=wizard) runs. It owns NO sockets, NO
// storage, and NO timers — it only answers two questions over the canonical script
// in src/wizardOnboarding.ts:
//
//   actCues(state)        → which body cues to PRESENT for the act we're on now
//   onHostEvent(state, e)  → did this event advance us, what receipt does that earn,
//                            and did it just complete onboarding?
//
// Law 7 holds: the Host (soul) decides which act we're in and which receipt is
// earned; the body only PRESENTS the cues and REPORTS the raw events that advance
// them. The browser path proves the same advance+receipt logic in
// components/buddy/BuddySurface.tsx (advanceWizard); this is its soul-side twin, so
// the two halves can never disagree about the flow.

import {
  advanceOnboarding,
  currentAct,
  type OnboardingCue,
  type OnboardingEvent,
  type OnboardingState,
} from "./wizardOnboarding";

/** The body cues to present when the buddy is sitting on `state`'s act, in order. */
export function actCues(state: OnboardingState): readonly OnboardingCue[] {
  return currentAct(state).cues;
}

export interface HostEventResult {
  /** State after applying the event. Reference-equal to `state` when nothing advanced. */
  next: OnboardingState;
  /**
   * The lifecycle receipt kind this advance earned, or null. Mirrors the browser
   * guard (BuddySurface.tsx:730-739): a receipt is earned ONLY when the act genuinely
   * advances AND carries one (first_contact / find_me carry none). The Host records
   * this against the durable lifecycle ledger — never on a non-advancing event.
   */
  receipt: string | null;
  /** True exactly on the event that completes the final act (the handoff trigger). */
  completedNow: boolean;
}

/**
 * Fold one to-soul event into the onboarding flow. Unrelated events are a no-op
 * (`next` is reference-equal to `state`, `receipt` null), so the Host can pass every
 * body/panel event through without pre-filtering.
 *
 * Idempotent past completion: once onboarding is done, further events earn no receipt
 * and never re-fire the handoff — re-running steps in the settings hub is the deferred
 * panel-window's concern, not the linear Host's.
 */
export function onHostEvent(state: OnboardingState, event: OnboardingEvent): HostEventResult {
  if (state.completed) {
    return { next: state, receipt: null, completedNow: false };
  }
  const act = currentAct(state);
  const advances = act.advanceOn.includes(event);
  const next = advanceOnboarding(state, event);
  return {
    next,
    receipt: advances && act.receipt ? act.receipt : null,
    completedNow: next.completed,
  };
}
