export type DockRenderMode = "head" | "bubble" | "head+bubble";

export type DockSettings = {
  collapsed: boolean;
  renderMode: DockRenderMode;
  fullscreen: boolean;
  windowBounds?: {
    x: number;
    y: number;
    width: number;
    height: number;
  };
};

export const DOCK_SETTINGS_STORAGE_KEY = "border-buddies:dock:v2";

export const DOCK_RENDER_MODE_LABELS: Record<DockRenderMode, string> = {
  head: "Heads",
  bubble: "Bubbles",
  "head+bubble": "Both",
};

export const DEFAULT_DOCK_SETTINGS: DockSettings = {
  collapsed: false,
  renderMode: "head+bubble",
  fullscreen: true,
};

const RENDER_MODES: DockRenderMode[] = ["head", "bubble", "head+bubble"];

export function normalizeDockSettings(candidate: Partial<DockSettings> | null | undefined): DockSettings {
  const renderMode = candidate?.renderMode;
  const validMode = renderMode && RENDER_MODES.includes(renderMode) ? renderMode : DEFAULT_DOCK_SETTINGS.renderMode;

  return {
    collapsed: candidate?.collapsed === true,
    renderMode: validMode,
    fullscreen: candidate?.fullscreen ?? DEFAULT_DOCK_SETTINGS.fullscreen,
    windowBounds: candidate?.windowBounds,
  };
}

export function loadStoredDockSettings(fallback: DockSettings = DEFAULT_DOCK_SETTINGS): DockSettings {
  try {
    const raw = localStorage.getItem(DOCK_SETTINGS_STORAGE_KEY);
    if (!raw) {
      return fallback;
    }

    return normalizeDockSettings(JSON.parse(raw) as Partial<DockSettings>);
  } catch {
    return fallback;
  }
}

export function cycleDockRenderMode(current: DockRenderMode): DockRenderMode {
  const index = RENDER_MODES.indexOf(current);
  return RENDER_MODES[(index + 1) % RENDER_MODES.length];
}
