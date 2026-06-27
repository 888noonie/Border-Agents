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
import { buildOnboardingPanelModel } from "./onboardingPanelModel";
import type {
  PresencePanelField,
  PresencePanelOption,
  PresencePanelRow,
  PresencePanelSection,
} from "./presenceProtocol";
import { createWizardHostDraft, type WizardHostDraft } from "./wizardHostDraft";

/** The body cues to present when the buddy is sitting on `state`'s act, in order. */
export function actCues(state: OnboardingState): readonly OnboardingCue[] {
  return currentAct(state).cues;
}

/**
 * The onboarding form section to render for `state`'s act, as a `panel` cue payload (Build C).
 * This is the soul-side twin of the React `OnboardingWizardPanel`: it maps the canonical
 * `onboardingPanelModel` section into the generic wire shape the native body draws. The Host owns
 * the words and the `primaryPanel` token (law 7); the body only presents and reports a
 * `clicked{panel: primaryPanel}` on confirm.
 *
 * Always returns a payload — a no-form act (Act 0/4) yields `section: "none"`, which tells the
 * body to close any open panel, so the body and the script never drift apart about what's shown.
 */
export interface HostPanel {
  section: PresencePanelSection;
  title: string;
  prompt?: string;
  options?: PresencePanelOption[];
  fields?: PresencePanelField[];
  rows?: PresencePanelRow[];
  primaryLabel?: string;
  primaryPanel?: string;
}

export function actPanel(
  state: OnboardingState,
  receiptKinds: readonly string[],
  draft: WizardHostDraft = createWizardHostDraft(),
): HostPanel {
  const model = buildOnboardingPanelModel({ state, receiptKinds });
  const title = model.act.title;
  const section = model.section;
  if (!section) {
    // No-form act (first_contact / find_me): close any open panel.
    return { section: "none", title };
  }
  switch (section.kind) {
    case "connect": {
      return {
        section: "connect",
        title,
        prompt: "Pick a provider and paste its API key.",
        options: section.providers.map((p) => ({
          id: p.id,
          label: p.label,
          detail: p.helper,
          selected: p.id === draft.provider,
        })),
        fields: [
          { key: "apiKey", label: "API key", control: "paste_key", masked: true },
          { key: "model", label: "Model", control: "text", value: draft.model },
        ],
        primaryLabel: section.primaryActionLabel,
        primaryPanel: "connection_ok",
      };
    }
    case "posture":
      return {
        section: "posture",
        title,
        prompt: "How much should I check with you before acting?",
        options: section.options.map((o) => ({
          id: o.posture,
          label: o.label,
          detail: o.consequence,
          selected: o.posture === draft.posture,
        })),
        primaryLabel: "Set posture",
        primaryPanel: "posture_set",
      };
    case "placement":
      return {
        section: "placement",
        title,
        prompt: "Pick which buddies appear and where they sit.",
        options: section.buddyChoices.map((b) => ({
          id: b.buddyId,
          label: b.label,
          detail: `${capitalize(draft.buddyEdges[b.buddyId] ?? b.defaultEdge)} edge`,
          selected: draft.enabledBuddyIds.includes(b.buddyId),
        })),
        primaryLabel: "Place them",
        primaryPanel: "next",
      };
    case "summary":
      return {
        section: "summary",
        title,
        prompt: "Here's what we set up.",
        rows: section.rows.map((r) => ({ label: r.title, status: r.status })),
        primaryLabel: "Finish",
        primaryPanel: "done",
      };
  }
}

function capitalize(text: string): string {
  return text.charAt(0).toUpperCase() + text.slice(1);
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
