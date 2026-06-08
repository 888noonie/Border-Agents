import {
  DEFAULT_DOCK_SETTINGS,
  normalizeDockSettings,
  type DockSettings,
} from "./dockSettings";
import {
  DEFAULT_GATEWAY_SETTINGS,
  normalizeGatewaySettings,
  type GatewaySettings,
} from "./gatewaySettings";

export type UserMode = "work" | "play" | "adjust";

export type UserModeReceiptDetail = "compact" | "standard" | "verbose";
export type UserModeFilePosture = "review_changes" | "light_preview" | "confirm_each_step";

export type UserModeSettings = {
  dock: DockSettings;
  gateway: GatewaySettings;
  receiptDetail: UserModeReceiptDetail;
  filePosture: UserModeFilePosture;
};

export type UserModeState = {
  activeMode: UserMode;
  modes: Record<UserMode, UserModeSettings>;
};

export const USER_MODE_STORAGE_KEY = "border-agents:user-modes:v1";

export const USER_MODE_LABELS: Record<UserMode, string> = {
  work: "Work",
  play: "Play",
  adjust: "Adjust",
};

export const USER_MODE_DESCRIPTIONS: Record<UserMode, string> = {
  work: "Project work with visible receipts and reviewed file changes",
  play: "Lighter interaction while keeping governed outputs",
  adjust: "Open dock controls for layout, recovery, and interaction posture",
};

export const USER_MODE_ORDER: UserMode[] = ["work", "play", "adjust"];

export const DEFAULT_USER_MODE_STATE: UserModeState = {
  activeMode: "work",
  modes: {
    work: {
      dock: DEFAULT_DOCK_SETTINGS,
      gateway: DEFAULT_GATEWAY_SETTINGS,
      receiptDetail: "standard",
      filePosture: "review_changes",
    },
    play: {
      dock: {
        collapsed: false,
        renderMode: "head+bubble",
      },
      gateway: DEFAULT_GATEWAY_SETTINGS,
      receiptDetail: "compact",
      filePosture: "light_preview",
    },
    adjust: {
      dock: {
        collapsed: false,
        renderMode: "head+bubble",
      },
      gateway: DEFAULT_GATEWAY_SETTINGS,
      receiptDetail: "verbose",
      filePosture: "confirm_each_step",
    },
  },
};

export function normalizeUserMode(candidate: unknown): UserMode {
  if (candidate === "private") {
    return "adjust";
  }

  return isUserMode(candidate) ? candidate : DEFAULT_USER_MODE_STATE.activeMode;
}

export function normalizeUserModeState(candidate: unknown): UserModeState {
  const source = candidate && typeof candidate === "object" ? candidate as Partial<UserModeState> : {};
  const activeMode = normalizeUserMode(source.activeMode);
  const sourceModes = source.modes && typeof source.modes === "object" ? source.modes : {};

  return {
    activeMode,
    modes: USER_MODE_ORDER.reduce<Record<UserMode, UserModeSettings>>((modes, mode) => {
      const legacyMode = mode === "adjust" ? "private" : mode;
      const storedMode =
        (sourceModes as Partial<Record<UserMode | "private", Partial<UserModeSettings>>>)[mode] ??
        (sourceModes as Partial<Record<UserMode | "private", Partial<UserModeSettings>>>)[legacyMode];

      modes[mode] = normalizeUserModeSettings(
        storedMode,
        DEFAULT_USER_MODE_STATE.modes[mode],
      );
      return modes;
    }, {} as Record<UserMode, UserModeSettings>),
  };
}

export function loadStoredUserModeState(
  fallback: UserModeState = DEFAULT_USER_MODE_STATE,
): UserModeState {
  try {
    const raw = localStorage.getItem(USER_MODE_STORAGE_KEY);
    if (!raw) {
      return fallback;
    }

    return normalizeUserModeState(JSON.parse(raw));
  } catch {
    return fallback;
  }
}

export function updateUserModeSettings(
  state: UserModeState,
  mode: UserMode,
  settings: Partial<UserModeSettings>,
): UserModeState {
  return {
    ...state,
    modes: {
      ...state.modes,
      [mode]: normalizeUserModeSettings({
        ...state.modes[mode],
        ...settings,
      }, state.modes[mode]),
    },
  };
}

function normalizeUserModeSettings(
  candidate: Partial<UserModeSettings> | null | undefined,
  fallback: UserModeSettings,
): UserModeSettings {
  return {
    dock: normalizeDockSettings(candidate?.dock ?? fallback.dock),
    gateway: normalizeGatewaySettings(candidate?.gateway ?? fallback.gateway),
    receiptDetail: isReceiptDetail(candidate?.receiptDetail)
      ? candidate.receiptDetail
      : fallback.receiptDetail,
    filePosture: isFilePosture(candidate?.filePosture)
      ? candidate.filePosture
      : fallback.filePosture,
  };
}

function isUserMode(value: unknown): value is UserMode {
  return typeof value === "string" && USER_MODE_ORDER.includes(value as UserMode);
}

function isReceiptDetail(value: unknown): value is UserModeReceiptDetail {
  return value === "compact" || value === "standard" || value === "verbose";
}

function isFilePosture(value: unknown): value is UserModeFilePosture {
  return value === "review_changes" || value === "light_preview" || value === "confirm_each_step";
}
