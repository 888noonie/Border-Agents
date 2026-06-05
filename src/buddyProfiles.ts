export type BuddySurface = "border" | "browser" | "watch" | "auto" | "boat";

export type BuddyProvider =
  | "codex"
  | "claude"
  | "grok"
  | "lm_studio"
  | "ollama"
  | "openrouter"
  | "custom";

export type BuddyMemoryMode = "off" | "reference_only" | "purpose_graded";

export type BuddyAuthorityDefaults = {
  allowAction: boolean;
  allowExternalShare: boolean;
  memoryMode: BuddyMemoryMode;
};

export type BuddyAdapterDefaults = {
  provider: BuddyProvider;
  modelLabel: string;
  connectionLabel: string;
};

export type BuddyIdentity = {
  id: string;
  name: string;
  shortName: string;
  ownerKind: "model" | "agent" | "project" | "subscription" | "workflow";
  ownerLabel: string;
  role: string;
};

export type BuddyAppearance = {
  color: string;
  accentColor?: string;
  defaultEdge: "top" | "right" | "bottom" | "left";
  defaultDockSlot: number;
};

export type BuddyProfile = {
  schemaVersion: 1;
  identity: BuddyIdentity;
  adapterDefaults: BuddyAdapterDefaults;
  authorityDefaults: BuddyAuthorityDefaults;
  appearance: BuddyAppearance;
  supportedSurfaces: BuddySurface[];
};

export type BuddySettings = {
  enabled: boolean;
  provider: BuddyProvider;
  modelLabel: string;
  connectionLabel: string;
  allowAction: boolean;
  allowExternalShare: boolean;
  memoryMode: BuddyMemoryMode;
};

export type AgentProfile = BuddyProfile;

export const BUDDY_PROVIDER_LABELS: Record<BuddyProvider, string> = {
  claude: "Claude",
  codex: "Codex",
  custom: "Custom",
  grok: "Grok",
  lm_studio: "LM Studio",
  ollama: "Ollama",
  openrouter: "OpenRouter",
};

export const BUDDY_MEMORY_LABELS: Record<BuddyMemoryMode, string> = {
  off: "Off",
  purpose_graded: "Purpose graded",
  reference_only: "Reference only",
};

export const BUDDY_PROFILES: Record<string, BuddyProfile> = {
  hermes: {
    schemaVersion: 1,
    identity: {
      id: "hermes",
      name: "Hermes",
      shortName: "Hermes",
      ownerKind: "model",
      ownerLabel: "Grok",
      role: "Fast Signal Companion",
    },
    adapterDefaults: {
      provider: "grok",
      modelLabel: "Grok subscription",
      connectionLabel: "Not connected",
    },
    authorityDefaults: {
      allowAction: false,
      allowExternalShare: false,
      memoryMode: "purpose_graded",
    },
    appearance: {
      color: "#2f7dff",
      accentColor: "#7df9ff",
      defaultEdge: "right",
      defaultDockSlot: 0.58,
    },
    supportedSurfaces: ["border", "browser"],
  },
  crab: {
    schemaVersion: 1,
    identity: {
      id: "crab",
      name: "Claw",
      shortName: "Claw",
      ownerKind: "model",
      ownerLabel: "Codex",
      role: "Memory Grading Companion",
    },
    adapterDefaults: {
      provider: "codex",
      modelLabel: "Codex",
      connectionLabel: "Local session",
    },
    authorityDefaults: {
      allowAction: false,
      allowExternalShare: false,
      memoryMode: "purpose_graded",
    },
    appearance: {
      color: "#ff4d4d",
      defaultEdge: "left",
      defaultDockSlot: 0.72,
    },
    supportedSurfaces: ["border"],
  },
  owl: {
    schemaVersion: 1,
    identity: {
      id: "owl",
      name: "Veritas",
      shortName: "Veritas",
      ownerKind: "model",
      ownerLabel: "Claude",
      role: "Claim Checking Companion",
    },
    adapterDefaults: {
      provider: "claude",
      modelLabel: "Claude subscription",
      connectionLabel: "Not connected",
    },
    authorityDefaults: {
      allowAction: false,
      allowExternalShare: false,
      memoryMode: "reference_only",
    },
    appearance: {
      color: "#7c5cff",
      defaultEdge: "top",
      defaultDockSlot: 0.24,
    },
    supportedSurfaces: ["border"],
  },
  fox: {
    schemaVersion: 1,
    identity: {
      id: "fox",
      name: "Nexus",
      shortName: "Nexus",
      ownerKind: "workflow",
      ownerLabel: "OpenRouter",
      role: "Context Routing Companion",
    },
    adapterDefaults: {
      provider: "openrouter",
      modelLabel: "OpenRouter route",
      connectionLabel: "Not connected",
    },
    authorityDefaults: {
      allowAction: false,
      allowExternalShare: false,
      memoryMode: "purpose_graded",
    },
    appearance: {
      color: "#ef6a3a",
      defaultEdge: "bottom",
      defaultDockSlot: 0.68,
    },
    supportedSurfaces: ["border"],
  },
};

export function buddyHasGateway(profile: BuddyProfile): boolean {
  return profile.identity.id === "hermes";
}

export function createDefaultBuddySettings(profile: BuddyProfile): BuddySettings {
  return {
    enabled: true,
    provider: profile.adapterDefaults.provider,
    modelLabel: profile.adapterDefaults.modelLabel,
    connectionLabel: profile.adapterDefaults.connectionLabel,
    allowAction: profile.authorityDefaults.allowAction,
    allowExternalShare: profile.authorityDefaults.allowExternalShare,
    memoryMode: profile.authorityDefaults.memoryMode,
  };
}

export function normalizeBuddySettings(profile: BuddyProfile, settings: unknown): BuddySettings {
  const defaults = createDefaultBuddySettings(profile);
  const candidate = settings && typeof settings === "object" ? settings as Partial<BuddySettings> : {};
  const provider = isBuddyProvider(candidate.provider) ? candidate.provider : defaults.provider;
  const memoryMode = isBuddyMemoryMode(candidate.memoryMode) ? candidate.memoryMode : defaults.memoryMode;

  return {
    enabled: candidate.enabled !== false,
    provider,
    modelLabel: normalizeShortText(candidate.modelLabel, defaults.modelLabel),
    connectionLabel: normalizeShortText(candidate.connectionLabel, defaults.connectionLabel),
    allowAction: candidate.allowAction === true,
    allowExternalShare: candidate.allowExternalShare === true,
    memoryMode,
  };
}

function normalizeShortText(value: unknown, fallback: string) {
  const text = String(value ?? "").trim();
  return text || fallback;
}

function isBuddyProvider(value: unknown): value is BuddyProvider {
  return typeof value === "string" && value in BUDDY_PROVIDER_LABELS;
}

function isBuddyMemoryMode(value: unknown): value is BuddyMemoryMode {
  return typeof value === "string" && value in BUDDY_MEMORY_LABELS;
}
