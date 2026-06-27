// Wizard Host draft — the soul-side mirror of the browser's OnboardingSurfaceDraft.
//
// The native body reports the user's in-progress picks on `clicked{panel:*}` via the additive
// `panelChoices` field (Build C Slice 4). The Host applies them here, re-emits `panel` cues
// that reflect the draft, and writes honest lifecycle receipt detail — never the API key value.

import {
  HERMES_PROVIDER_PRESETS,
  type HermesProviderPresetId,
} from "./onboardingPanelModel";
import {
  createDefaultOnboardingSurfaceState,
  type OnboardingSurfaceDraft,
} from "./onboardingSurfaceState";
import { isUserPosture } from "./core/userPosture";
import type { PresenceHermesHydrateDraft, PresencePanelChoices } from "./presenceProtocol";
import type { OnboardingAct } from "./wizardOnboarding";

export type WizardHostDraft = OnboardingSurfaceDraft;

export function createWizardHostDraft(): WizardHostDraft {
  return createDefaultOnboardingSurfaceState().draft;
}

/** Build the in-process Hermes handoff draft for `hydrate` — includes `apiKey`; never log this. */
export function hermesHydrateDraftFromWizard(draft: WizardHostDraft): PresenceHermesHydrateDraft {
  return {
    provider: draft.provider,
    apiBase: draft.apiBase,
    apiKey: draft.apiKey,
    model: draft.model,
    systemPrompt: draft.systemPrompt,
    posture: draft.posture,
    enabledBuddyIds: [...draft.enabledBuddyIds],
    buddyEdges: { ...draft.buddyEdges },
  };
}

function isProviderPresetId(value: string): value is HermesProviderPresetId {
  return HERMES_PROVIDER_PRESETS.some((preset) => preset.id === value);
}

/** Fold a body's `panelChoices` into the draft when the user confirms a section. */
export function applyPanelChoices(
  draft: WizardHostDraft,
  panel: string,
  choices?: PresencePanelChoices,
): WizardHostDraft {
  if (!choices) {
    return draft;
  }
  const next: WizardHostDraft = {
    ...draft,
    buddyEdges: { ...draft.buddyEdges },
    enabledBuddyIds: [...draft.enabledBuddyIds],
  };

  switch (panel) {
    case "connection_ok": {
      const selected = choices.selectedOptionIds?.[0];
      if (selected && isProviderPresetId(selected)) {
        const preset = HERMES_PROVIDER_PRESETS.find((item) => item.id === selected)!;
        next.provider = selected;
        next.apiBase = preset.apiBase;
        // Model resets to the preset placeholder when the user switches provider without
        // supplying a model — stale model names must not follow a preset change. apiKey is
        // the opposite: absent fieldValues.apiKey keeps the prior value (never silently wipe
        // a secret the body may not have re-reported on this confirm).
        if (!choices.fieldValues?.model) {
          next.model = preset.modelPlaceholder;
        }
      }
      const apiKey = choices.fieldValues?.apiKey;
      if (typeof apiKey === "string") {
        next.apiKey = apiKey;
      }
      const model = choices.fieldValues?.model;
      if (typeof model === "string" && model.trim()) {
        next.model = model.trim();
      }
      break;
    }
    case "posture_set": {
      const selected = choices.selectedOptionIds?.[0];
      if (selected && isUserPosture(selected)) {
        next.posture = selected;
      }
      break;
    }
    case "next": {
      const selected = choices.selectedOptionIds ?? [];
      // v0.1: an empty selection keeps the prior set — at least one buddy must stay enabled.
      next.enabledBuddyIds = selected.length > 0 ? [...new Set(selected)] : next.enabledBuddyIds;
      break;
    }
    default:
      break;
  }

  return next;
}

/** Honest receipt detail for a lifecycle milestone — mirrors BuddySurface.tsx `receiptDetail`. */
export function receiptDetailForAct(
  receipt: string,
  draft: WizardHostDraft,
): Record<string, string | number | boolean> | undefined {
  switch (receipt) {
    case "credential.stored":
      return {
        provider: presetBuddyProvider(draft.provider),
        model: draft.model,
        apiBase: draft.apiBase,
        apiKeyPresent: draft.apiKey.trim().length > 0,
      };
    case "posture.set":
      return { posture: draft.posture };
    case "placement.set":
      return { enabledBuddyCount: draft.enabledBuddyIds.length };
    default:
      return undefined;
  }
}

export function receiptDetailForOnboardingAct(
  act: OnboardingAct,
  draft: WizardHostDraft,
): Record<string, string | number | boolean> | undefined {
  return act.receipt ? receiptDetailForAct(act.receipt, draft) : undefined;
}

function presetBuddyProvider(id: HermesProviderPresetId): string {
  return HERMES_PROVIDER_PRESETS.find((preset) => preset.id === id)?.buddyProvider ?? "custom";
}