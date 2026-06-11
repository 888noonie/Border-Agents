import { DEFAULT_HERMES_SYSTEM_PROMPT, HERMES_PROVIDER_PRESETS, type HermesProviderPresetId } from "./onboardingPanelModel";
import { DEFAULT_USER_POSTURE, isUserPosture, type UserPosture } from "./core/userPosture";
import { INITIAL_ONBOARDING_STATE, type OnboardingState } from "./wizardOnboarding";

export const ONBOARDING_SURFACE_STATE_KEY = "border-agents:onboarding-surface:v1";

export type OnboardingBuddyEdge = "top" | "right" | "bottom" | "left";

export interface OnboardingSurfaceDraft {
  provider: HermesProviderPresetId;
  apiBase: string;
  apiKey: string;
  model: string;
  systemPrompt: string;
  posture: UserPosture;
  enabledBuddyIds: string[];
  buddyEdges: Record<string, OnboardingBuddyEdge>;
  outputIndex: number;
}

export interface OnboardingSurfaceState {
  progress: OnboardingState;
  receiptKinds: string[];
  draft: OnboardingSurfaceDraft;
}

export function createDefaultOnboardingSurfaceState(): OnboardingSurfaceState {
  const preset = HERMES_PROVIDER_PRESETS[0];
  return {
    progress: INITIAL_ONBOARDING_STATE,
    receiptKinds: [],
    draft: {
      provider: preset.id,
      apiBase: preset.apiBase,
      apiKey: "",
      model: preset.modelPlaceholder,
      systemPrompt: DEFAULT_HERMES_SYSTEM_PROMPT,
      posture: DEFAULT_USER_POSTURE,
      enabledBuddyIds: ["hermes", "owl", "crab"],
      buddyEdges: {
        hermes: "right",
        owl: "top",
        crab: "left",
      },
      outputIndex: 0,
    },
  };
}

export function loadStoredOnboardingSurfaceState(
  storage: Storage = window.localStorage,
): OnboardingSurfaceState {
  const fallback = createDefaultOnboardingSurfaceState();
  try {
    const raw = storage.getItem(ONBOARDING_SURFACE_STATE_KEY);
    if (!raw) {
      return fallback;
    }
    return normalizeOnboardingSurfaceState(JSON.parse(raw), fallback);
  } catch {
    return fallback;
  }
}

export function saveStoredOnboardingSurfaceState(
  state: OnboardingSurfaceState,
  storage: Storage = window.localStorage,
) {
  try {
    storage.setItem(
      ONBOARDING_SURFACE_STATE_KEY,
      JSON.stringify(normalizeOnboardingSurfaceState(state, createDefaultOnboardingSurfaceState())),
    );
  } catch {
    // Best-effort UI persistence only.
  }
}

function normalizeOnboardingSurfaceState(
  candidate: unknown,
  fallback: OnboardingSurfaceState,
): OnboardingSurfaceState {
  const parsed = candidate && typeof candidate === "object" ? candidate as Partial<OnboardingSurfaceState> : {};
  const draft = parsed.draft && typeof parsed.draft === "object"
    ? parsed.draft as Partial<OnboardingSurfaceDraft>
    : {};
  const provider = isProviderPresetId(draft.provider) ? draft.provider : fallback.draft.provider;
  const preset = HERMES_PROVIDER_PRESETS.find((item) => item.id === provider) ?? HERMES_PROVIDER_PRESETS[0];

  return {
    progress: normalizeOnboardingState(parsed.progress, fallback.progress),
    receiptKinds: Array.isArray(parsed.receiptKinds)
      ? parsed.receiptKinds.filter((value): value is string => typeof value === "string")
      : fallback.receiptKinds,
    draft: {
      provider,
      apiBase: normalizeText(draft.apiBase, preset.apiBase),
      apiKey: normalizeText(draft.apiKey, ""),
      model: normalizeText(draft.model, preset.modelPlaceholder),
      systemPrompt: normalizeText(draft.systemPrompt, DEFAULT_HERMES_SYSTEM_PROMPT),
      posture: isUserPosture(draft.posture) ? draft.posture : fallback.draft.posture,
      enabledBuddyIds: Array.isArray(draft.enabledBuddyIds)
        ? draft.enabledBuddyIds.filter((value): value is string => typeof value === "string")
        : fallback.draft.enabledBuddyIds,
      buddyEdges: normalizeBuddyEdges(draft.buddyEdges, fallback.draft.buddyEdges),
      outputIndex: typeof draft.outputIndex === "number" ? Math.max(0, Math.round(draft.outputIndex)) : fallback.draft.outputIndex,
    },
  };
}

function normalizeOnboardingState(
  candidate: unknown,
  fallback: OnboardingState,
): OnboardingState {
  const parsed = candidate && typeof candidate === "object" ? candidate as Partial<OnboardingState> : {};
  return {
    actIndex: typeof parsed.actIndex === "number" ? Math.max(0, Math.round(parsed.actIndex)) : fallback.actIndex,
    completed: parsed.completed === true,
  };
}

function normalizeBuddyEdges(
  candidate: unknown,
  fallback: Record<string, OnboardingBuddyEdge>,
): Record<string, OnboardingBuddyEdge> {
  if (!candidate || typeof candidate !== "object") {
    return fallback;
  }

  const next: Record<string, OnboardingBuddyEdge> = { ...fallback };
  for (const [key, value] of Object.entries(candidate)) {
    if (isBuddyEdge(value)) {
      next[key] = value;
    }
  }
  return next;
}

function normalizeText(value: unknown, fallback: string): string {
  const text = String(value ?? "").trim();
  return text || fallback;
}

function isProviderPresetId(value: unknown): value is HermesProviderPresetId {
  return HERMES_PROVIDER_PRESETS.some((preset) => preset.id === value);
}

function isBuddyEdge(value: unknown): value is OnboardingBuddyEdge {
  return value === "top" || value === "right" || value === "bottom" || value === "left";
}
