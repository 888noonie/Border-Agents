import { BUDDY_PROFILES, type BuddyProvider } from "./buddyProfiles";
import {
  DEFAULT_USER_POSTURE,
  USER_POSTURES,
  getInteractionPosture,
  requiresConfirmation,
  resolvePosturePolicy,
  type UserPosture,
} from "./core/userPosture";
import {
  currentAct,
  entryMode,
  ONBOARDING_ACTS,
  type OnboardingAct,
  type OnboardingCue,
  type OnboardingEntryMode,
  type OnboardingState,
} from "./wizardOnboarding";

export type HermesProviderPresetId = "xai" | "openrouter" | "lm_studio" | "ollama";

export interface HermesProviderPreset {
  id: HermesProviderPresetId;
  label: string;
  apiBase: string;
  modelPlaceholder: string;
  helper: string;
  // Hosted providers authenticate with an API key; local OpenAI-compatible
  // servers (LM Studio, Ollama) accept connections without one.
  requiresApiKey: boolean;
  // The BuddyProvider value this preset maps to (used when writing BuddySettings on connect).
  buddyProvider: BuddyProvider;
}

export const HERMES_PROVIDER_PRESETS: readonly HermesProviderPreset[] = [
  {
    id: "xai",
    label: "xAI / Grok",
    apiBase: "https://api.x.ai/v1",
    modelPlaceholder: "grok-4",
    helper: "Hosted Grok via the OpenAI-compatible xAI endpoint.",
    requiresApiKey: true,
    buddyProvider: "grok",
  },
  {
    id: "openrouter",
    label: "OpenRouter",
    apiBase: "https://openrouter.ai/api/v1",
    modelPlaceholder: "openai/gpt-5",
    helper: "Route Hermes through a hosted OpenRouter model.",
    requiresApiKey: true,
    buddyProvider: "openrouter",
  },
  {
    id: "lm_studio",
    label: "LM Studio",
    apiBase: "http://127.0.0.1:1234/v1",
    modelPlaceholder: "loaded-model-name",
    helper: "Use a local LM Studio model exposed on localhost.",
    requiresApiKey: false,
    buddyProvider: "lm_studio",
  },
  {
    id: "ollama",
    label: "Ollama",
    apiBase: "http://127.0.0.1:11434/v1",
    modelPlaceholder: "llama3.1",
    helper: "Use Ollama's OpenAI-compatible local endpoint.",
    requiresApiKey: false,
    buddyProvider: "ollama",
  },
] as const;

export const DEFAULT_HERMES_SYSTEM_PROMPT =
  "You are Hermes, a concise desktop companion speaking through the Border Agents buddy gateway. Be direct, useful, and clear.";

const PANEL_SECTIONS = ["connect", "posture", "placement", "summary"] as const;

export type OnboardingPanelSection = (typeof PANEL_SECTIONS)[number];

export interface OnboardingPanelNavItem {
  section: OnboardingPanelSection;
  label: string;
  active: boolean;
  enabled: boolean;
}

export interface ConnectSectionModel {
  kind: "connect";
  receipt: "credential.stored";
  providers: readonly HermesProviderPreset[];
  defaultProvider: HermesProviderPresetId;
  fields: readonly WizardFieldModel[];
  primaryActionLabel: string;
}

export interface WizardFieldModel {
  key: "provider" | "apiBase" | "apiKey" | "model" | "systemPrompt";
  label: string;
  control: "select" | "text" | "password" | "textarea";
  placeholder?: string;
}

export interface PostureSectionModel {
  kind: "posture";
  receipt: "posture.set";
  defaultPosture: UserPosture;
  options: readonly PostureOptionModel[];
}

export interface PostureOptionModel {
  posture: UserPosture;
  label: string;
  consequence: string;
  authorizationSummary: string;
  interactionSummary: string;
}

export interface PlacementSectionModel {
  kind: "placement";
  receipt: "placement.set";
  buddyChoices: readonly PlacementBuddyChoice[];
  edgeChoices: readonly PlacementEdgeChoice[];
  outputChoices: readonly PlacementOutputChoice[];
}

export interface PlacementBuddyChoice {
  buddyId: string;
  label: string;
  defaultEnabled: boolean;
  defaultEdge: PlacementEdgeChoice["edge"];
}

export interface PlacementEdgeChoice {
  edge: "top" | "right" | "bottom" | "left";
  label: string;
}

export interface PlacementOutputChoice {
  value: number;
  label: string;
}

export interface SummarySectionModel {
  kind: "summary";
  receipt: "onboarding.completed";
  rows: readonly SummaryRowModel[];
}

export interface SummaryRowModel {
  actId: OnboardingAct["id"];
  title: string;
  receipt: string;
  status: "recorded" | "pending";
}

export type OnboardingPanelSectionModel =
  | ConnectSectionModel
  | PostureSectionModel
  | PlacementSectionModel
  | SummarySectionModel;

// What the panel shows for a "no forms" act (panelSection: "none"). The copy comes
// from the act's own `say` cue so the panel and the body speak the same script.
export interface OnboardingPanelIdleModel {
  text: string;
  // Label for the begin button when the act advances on "clicked"; null when the
  // act advances on its own and the panel should just wait.
  beginLabel: string | null;
  // Milliseconds before the panel host fires "timeout" for acts that advance on it
  // (the panel stands in for the Host soul's timer); null otherwise.
  autoAdvanceMs: number | null;
}

// How long the panel lingers on a timeout-advancing act ("find_me") before moving
// on — enough to read the cue line.
export const IDLE_AUTO_ADVANCE_MS = 6000;

export interface OnboardingPanelModel {
  mode: OnboardingEntryMode;
  act: OnboardingAct;
  nav: readonly OnboardingPanelNavItem[];
  section: OnboardingPanelSectionModel | null;
  // Present exactly when `section` is null.
  idle: OnboardingPanelIdleModel | null;
}

export function buildOnboardingPanelModel(args: {
  state: OnboardingState;
  receiptKinds: readonly string[];
  sectionOverride?: OnboardingPanelSection | null;
}): OnboardingPanelModel {
  const act = currentAct(args.state);
  const mode = entryMode(args.receiptKinds);
  const activeSection =
    mode === "hub" && args.sectionOverride ? args.sectionOverride : act.panelSection;
  return {
    mode,
    act,
    nav: buildNav(activeSection, mode),
    section: buildSection(activeSection, args.receiptKinds),
    idle: activeSection === "none" ? buildIdle(act) : null,
  };
}

function buildIdle(act: OnboardingAct): OnboardingPanelIdleModel {
  const say = act.cues.find(
    (cue): cue is Extract<OnboardingCue, { kind: "say" }> => cue.kind === "say",
  );
  return {
    text: say?.text ?? "The host is ready.",
    beginLabel: act.advanceOn.includes("clicked") ? "Let's set up" : null,
    autoAdvanceMs: act.advanceOn.includes("timeout") ? IDLE_AUTO_ADVANCE_MS : null,
  };
}

function buildNav(
  activeSection: OnboardingAct["panelSection"],
  mode: OnboardingEntryMode,
): OnboardingPanelNavItem[] {
  const currentIndex = activeSection === "none" ? -1 : PANEL_SECTIONS.indexOf(activeSection);
  return PANEL_SECTIONS.map((section, index) => ({
    section,
    label: sectionLabel(section),
    active: activeSection === section,
    enabled: mode === "hub" || index <= currentIndex,
  }));
}

function buildSection(
  activeSection: OnboardingAct["panelSection"],
  receiptKinds: readonly string[],
): OnboardingPanelSectionModel | null {
  switch (activeSection) {
    case "connect":
      return {
        kind: "connect",
        receipt: "credential.stored",
        providers: HERMES_PROVIDER_PRESETS,
        defaultProvider: "xai",
        primaryActionLabel: "Test connection",
        fields: [
          { key: "provider", label: "Provider", control: "select" },
          { key: "apiBase", label: "API base", control: "text", placeholder: preset("xai").apiBase },
          { key: "apiKey", label: "API key", control: "password" },
          { key: "model", label: "Model", control: "text", placeholder: preset("xai").modelPlaceholder },
          {
            key: "systemPrompt",
            label: "System prompt",
            control: "textarea",
            placeholder: DEFAULT_HERMES_SYSTEM_PROMPT,
          },
        ],
      };
    case "posture":
      return {
        kind: "posture",
        receipt: "posture.set",
        defaultPosture: DEFAULT_USER_POSTURE,
        options: USER_POSTURES.map(buildPostureOption),
      };
    case "placement":
      return {
        kind: "placement",
        receipt: "placement.set",
        buddyChoices: ["hermes", "owl", "crab"]
          .map((buddyId) => BUDDY_PROFILES[buddyId])
          .filter(Boolean)
          .map((profile) => ({
            buddyId: profile.identity.id,
            label: profile.identity.name,
            defaultEnabled: true,
            defaultEdge: profile.appearance.defaultEdge,
          })),
        edgeChoices: [
          { edge: "left", label: "Left edge" },
          { edge: "right", label: "Right edge" },
          { edge: "top", label: "Top edge" },
          { edge: "bottom", label: "Bottom edge" },
        ],
        outputChoices: [
          { value: 0, label: "Primary display" },
          { value: 1, label: "Display 2" },
        ],
      };
    case "summary":
      return {
        kind: "summary",
        receipt: "onboarding.completed",
        rows: ONBOARDING_ACTS.filter((candidate) => candidate.receipt !== null).map((candidate) => ({
          actId: candidate.id,
          title: candidate.title,
          receipt: candidate.receipt!,
          status: receiptKinds.includes(candidate.receipt!) ? "recorded" : "pending",
        })),
      };
    case "none":
      return null;
  }
}

function preset(id: HermesProviderPresetId): HermesProviderPreset {
  return HERMES_PROVIDER_PRESETS.find((provider) => provider.id === id) ?? HERMES_PROVIDER_PRESETS[0];
}

function buildPostureOption(posture: UserPosture): PostureOptionModel {
  const interaction = getInteractionPosture(posture);
  const policy = resolvePosturePolicy(posture, "agent_action").policy;
  return {
    posture,
    label: titleCase(posture),
    consequence: postureConsequence(posture),
    authorizationSummary: authorizationSummary(posture, policy.allow_grades_in_prompt),
    interactionSummary:
      `${titleCase(interaction.notification_verbosity)} updates; ` +
      `${requiresConfirmation(posture, "low") ? "confirm low-risk actions" : "low-risk actions can flow without confirm"}.`,
  };
}

function postureConsequence(posture: UserPosture): string {
  switch (posture) {
    case "private":
      return "Nothing leaves without your say-so.";
    case "work":
      return "Balanced default for normal trusted work.";
    case "play":
      return "Same trust line as Work, with lighter interaction friction.";
  }
}

function authorizationSummary(posture: UserPosture, grades: readonly string[]): string {
  if (posture === "private") {
    return `Trusted-only prompts; public-only sensitivity; ${grades.join(", ")} for action context.`;
  }
  return "Built-in purpose policies as authored; no widening beyond the trust baseline.";
}

function sectionLabel(section: OnboardingPanelSection): string {
  switch (section) {
    case "connect":
      return "Connect";
    case "posture":
      return "Posture";
    case "placement":
      return "Placement";
    case "summary":
      return "Receipts";
  }
}

function titleCase(text: string): string {
  return text.charAt(0).toUpperCase() + text.slice(1);
}
