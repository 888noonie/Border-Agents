import {
  type CSSProperties,
  type MutableRefObject,
  type PointerEvent as ReactPointerEvent,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  useCallback,
} from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { register, unregister } from "@tauri-apps/plugin-global-shortcut";
import clawManifest from "../characters/crab/manifest.json";
import hermesManifest from "../characters/hermes/manifest.json";
import owlManifest from "../characters/owl/manifest.json";
import {
  BUDDY_PROFILES,
  buddyHasGateway,
  type BuddySettings,
  createDefaultBuddySettings,
  normalizeBuddySettings,
} from "../src/buddyProfiles";
import { bbLog } from "../src/bbDiagnostics";
import type { GatewayConnectionState } from "../src/gatewayProtocol";
import {
  DEFAULT_GATEWAY_SETTINGS,
  GATEWAY_SETTINGS_STORAGE_KEY,
  loadStoredGatewaySettings,
  normalizeGatewaySettings,
  type GatewaySettings,
} from "../src/gatewaySettings";
import { connectionLabelForState, useBuddyGateway } from "../src/useBuddyGateway";
import { BuddySurface, type BuddySurfaceHandle } from "./buddy/BuddySurface";
import {
  createHealReport,
  DOCK_RECOVER_SHORTCUT,
  SELF_HEAL_INTERVAL_MS,
  STUCK_DRAG_TIMEOUT_MS,
  type DockHealAction,
  type DockHealReport,
} from "../src/dockSelfHeal";
import {
  cycleDockRenderMode,
  DEFAULT_DOCK_SETTINGS,
  DOCK_RENDER_MODE_LABELS,
  DOCK_SETTINGS_STORAGE_KEY,
  loadStoredDockSettings,
  type DockRenderMode,
  type DockSettings,
} from "../src/dockSettings";
import "./BorderDock.css";

export interface Hitbox {
  x: number;
  y: number;
  w: number;
  h: number;
}

export function useBuddyHitbox(overlayDragActiveRef?: MutableRefObject<boolean>) {
  const pending = useRef<Hitbox[]>([]);
  const raf = useRef<number | null>(null);
  const failureCount = useRef(0);

  const flush = useCallback(() => {
    if (overlayDragActiveRef?.current) {
      return;
    }

    if (raf.current) cancelAnimationFrame(raf.current);
    raf.current = requestAnimationFrame(async () => {
      if (overlayDragActiveRef?.current) {
        return;
      }

      try {
        await invoke("set_input_hitboxes", { boxes: pending.current });
        failureCount.current = 0;
      } catch (error) {
        failureCount.current += 1;
        void bbLog("error", "set_input_hitboxes failed", {
          failureCount: failureCount.current,
          boxCount: pending.current.length,
          boxes: pending.current,
          error: String(error),
        });
      }
    });
  }, [overlayDragActiveRef]);

  const hasHitboxFailures = useCallback(() => failureCount.current > 0, []);

  const setHitboxes = useCallback(
    (boxes: Hitbox[]) => {
      pending.current = boxes;
      flush();
    },
    [flush],
  );

  useEffect(() => () => {
    if (raf.current) cancelAnimationFrame(raf.current);
  }, []);

  return { setHitboxes, hasHitboxFailures };
}

function useDockHitboxRegistry(
  clickThroughRef: MutableRefObject<boolean>,
  overlayDragActiveRef: MutableRefObject<boolean>,
) {
  const boxesByBuddy = useRef<Map<string, Hitbox[]>>(new Map());
  const chromeNodeRef = useRef<HTMLDivElement | null>(null);
  const passBannerNodeRef = useRef<HTMLDivElement | null>(null);
  const [chromeNode, setChromeNode] = useState<HTMLDivElement | null>(null);
  const [passBannerNode, setPassBannerNode] = useState<HTMLDivElement | null>(null);
  const { setHitboxes, hasHitboxFailures } = useBuddyHitbox(overlayDragActiveRef);

  const controlsRef = useCallback((node: HTMLDivElement | null) => {
    chromeNodeRef.current = node;
    setChromeNode(node);
  }, []);

  const passBannerRef = useCallback((node: HTMLDivElement | null) => {
    passBannerNodeRef.current = node;
    setPassBannerNode(node);
  }, []);

  const flushHitboxes = useCallback(() => {
    if (overlayDragActiveRef.current) {
      return;
    }

    const boxes = Array.from(boxesByBuddy.current.values()).flat();
    const chrome = chromeNodeRef.current;

    if (chrome) {
      const rect = chrome.getBoundingClientRect();
      boxes.push({
        x: Math.round(rect.left),
        y: Math.round(rect.top),
        w: Math.round(rect.width),
        h: Math.round(rect.height),
      });
    }

    const passBanner = passBannerNodeRef.current;
    if (passBanner) {
      const rect = passBanner.getBoundingClientRect();
      boxes.push({
        x: Math.round(rect.left),
        y: Math.round(rect.top),
        w: Math.round(rect.width),
        h: Math.round(rect.height),
      });
    }

    setHitboxes(boxes);
  }, [overlayDragActiveRef, setHitboxes]);

  const reportHitboxes = useCallback(
    (buddyId: string, boxes: Hitbox[]) => {
      if (clickThroughRef.current) {
        boxesByBuddy.current.delete(buddyId);
        flushHitboxes();
        return;
      }

      boxesByBuddy.current.set(buddyId, boxes);
      flushHitboxes();
    },
    [clickThroughRef, flushHitboxes],
  );

  const clearBuddyHitboxes = useCallback(
    (buddyId: string) => {
      boxesByBuddy.current.delete(buddyId);
      flushHitboxes();
    },
    [flushHitboxes],
  );

  const clearAllHitboxes = useCallback(() => {
    boxesByBuddy.current.clear();
    flushHitboxes();
  }, [flushHitboxes]);

  const refreshChromeHitboxes = useCallback(() => {
    flushHitboxes();
  }, [flushHitboxes]);

  useEffect(() => {
    if (!chromeNode || typeof ResizeObserver === "undefined") {
      return;
    }

    const observer = new ResizeObserver(() => {
      flushHitboxes();
    });

    observer.observe(chromeNode);

    return () => {
      observer.disconnect();
    };
  }, [chromeNode, flushHitboxes]);

  useEffect(() => {
    if (!passBannerNode || typeof ResizeObserver === "undefined") {
      return;
    }

    flushHitboxes();

    const observer = new ResizeObserver(() => {
      flushHitboxes();
    });

    observer.observe(passBannerNode);

    return () => {
      observer.disconnect();
    };
  }, [flushHitboxes, passBannerNode]);

  return {
    controlsRef,
    passBannerRef,
    reportHitboxes,
    clearBuddyHitboxes,
    clearAllHitboxes,
    refreshChromeHitboxes,
    hasHitboxFailures,
  };
}

function isBrowserPreviewSurface() {
  try {
    const label = getCurrentWebviewWindow().label;
    return label !== "border-dock" && !label.startsWith("buddy-");
  } catch {
    return true;
  }
}

type Edge = "top" | "right" | "bottom" | "left";
type ResizeDirection = "East" | "North" | "NorthEast" | "NorthWest" | "South" | "SouthEast" | "SouthWest" | "West";

type BuddyOwnerKind = "model" | "agent" | "project" | "subscription" | "workflow";

type DockBuddy = {
  id: string;
  name: string;
  shortName: string;
  ownerKind: BuddyOwnerKind;
  ownerLabel: string;
  role: string;
  personality: string;
  speechStyle: string;
  edge: Edge;
  dockSlot: number;
  color: string;
  accentColor?: string;
  message?: string;
  visible: "primary" | "faint";
};

type MonitorFrame = {
  id: string;
  name: string | null;
  x: number;
  y: number;
  width: number;
  height: number;
  scaleFactor: number;
  primary: boolean;
};

type DockLayout = {
  monitors: MonitorFrame[];
  activeMonitorIds: string[];
  bounds: {
    x: number;
    y: number;
    width: number;
    height: number;
  };
  multiMonitor: boolean;
};

type BuddyWindowLayout = {
  buddyId: string;
  monitor: MonitorFrame;
  bounds: DockLayout["bounds"];
  interactive: boolean;
};

type BuddySnapResult = {
  buddyId: string;
  snapped: boolean;
  edge: Edge | null;
  slot: number | null;
  bounds: DockLayout["bounds"];
};

type AgentPlacement =
  | {
      state: "tucked";
      edge: Edge;
      slot: number;
    }
  | {
      state: "free";
      edge: Edge;
      x: number;
      y: number;
    };

type FreeAgentPlacement = Extract<AgentPlacement, { state: "free" }>;
type AgentPlacements = Record<string, AgentPlacement>;
type BuddySettingsMap = Record<string, BuddySettings>;

type ActiveDrag = {
  agentId: string;
  offsetX: number;
  offsetY: number;
};

const INITIAL_LAYOUT: DockLayout = {
  monitors: [],
  activeMonitorIds: ["browser-preview"],
  bounds: { x: 0, y: 0, width: 1280, height: 800 },
  multiMonitor: false,
};

const FREE_AGENT_SIZE = 118;
const FREE_AGENT_MARGIN = 16;
const SNAP_DISTANCE = 96;
const DOCKED_HEAD_HITBOX_SIZE = 96;
const DOCKED_HEAD_EDGE_OVERLAP = 18;
// A press only becomes a drag after the pointer travels this far. Below the
// threshold the gesture is treated as a click so the buddy can be activated /
// opened. This is what makes "click the border buddy to test Hermes" reliable.
const DRAG_ACTIVATION_THRESHOLD = 6;

const PLACEMENT_STORAGE_KEY = "border-buddies:placements:v4";
const SETTINGS_STORAGE_KEY = "border-buddies:settings:v2";
const HIDDEN_NATIVE_WINDOW_ID = "__native_hidden__";
const DEFAULT_BORDER_BUDDY_ID = "hermes";
const PASS_THROUGH_SHORTCUT = "CommandOrControl+Alt+B";
const DOCK_COLLAPSE_SHORTCUT = "CommandOrControl+Alt+H";
const FULL_RENDER_MODE: DockRenderMode = "head+bubble";
const IDLE_FADE_DELAY = 5600;
const IDLE_CLICK_THROUGH_PULSE = 900;
const DOCK_SLOTS_BY_EDGE: Record<Edge, number[]> = {
  left: [0.22, 0.5, 0.78],
  right: [0.22, 0.5, 0.78],
  top: [0.24, 0.5, 0.76],
  bottom: [0.24, 0.5, 0.76],
};

const buddies: DockBuddy[] = [
  {
    id: hermesManifest.id,
    name: hermesManifest.name,
    shortName: "Hermes",
    ownerKind: "model",
    ownerLabel: hermesManifest.owner,
    role: hermesManifest.role,
    personality: hermesManifest.personality,
    speechStyle: hermesManifest.speech_style,
    edge: hermesManifest.border_position as Edge,
    dockSlot: 0.58,
    color: hermesManifest.color,
    accentColor: hermesManifest.accent_color,
    message: "",
    visible: "primary",
  },
  {
    id: clawManifest.id,
    name: clawManifest.name,
    shortName: "Claw",
    ownerKind: "model",
    ownerLabel: "Codex",
    role: clawManifest.role,
    personality: clawManifest.personality,
    speechStyle: "Short, celebratory, a little cheeky",
    edge: "left",
    dockSlot: 0.72,
    color: clawManifest.color,
    message: "Memory graded! Trusted pieces ready?",
    visible: "faint",
  },
  {
    id: owlManifest.id,
    name: owlManifest.name,
    shortName: "Veritas",
    ownerKind: "model",
    ownerLabel: "Claude",
    role: owlManifest.role,
    personality: owlManifest.personality,
    speechStyle: "Concise, exact, gently corrective",
    edge: owlManifest.border_position as Edge,
    dockSlot: 0.24,
    color: owlManifest.color,
    message: "One source is assertable.",
    visible: "faint",
  },
  {
    id: "fox",
    name: "Nexus",
    shortName: "Nexus",
    ownerKind: "model",
    ownerLabel: "Grok",
    role: "Memory / Context Guide",
    personality: "Curious, warm, careful",
    speechStyle: "Lively, curious, helpful",
    edge: "bottom",
    dockSlot: 0.68,
    color: "#ef6a3a",
    message: "5 memories found. 1 trusted.",
    visible: "faint",
  },
];

const defaultPlacements = buddies.reduce<AgentPlacements>((placements, buddy) => {
  placements[buddy.id] = {
    state: "tucked",
    edge: buddy.edge,
    slot: buddy.dockSlot,
  };

  return placements;
}, {});

const defaultBuddySettings = buddies.reduce<BuddySettingsMap>((settings, buddy) => {
  settings[buddy.id] = createDefaultBuddySettings(BUDDY_PROFILES[buddy.id]);
  return settings;
}, {});

async function setDockInputEnabled(enabled: boolean) {
  try {
    await getCurrentWindow().setIgnoreCursorEvents(!enabled);
  } catch {
    // Browser previews and platforms without the Tauri window permission keep CSS hit testing.
  }
}

export function BorderDock() {
  const stageRef = useRef<HTMLDivElement>(null);
  const dragRef = useRef<ActiveDrag | null>(null);
  const pendingDragCleanupRef = useRef<(() => void) | null>(null);
  const clickThroughRef = useRef(false);

  const idleTimerRef = useRef<number | null>(null);
  const nativeMoveSnapTimerRef = useRef<number | null>(null);
  const initialWindowBuddyId = useMemo(getCurrentBuddyIdFromUrl, []);
  const [windowBuddyId, setWindowBuddyId] = useState<string | null | undefined>(
    initialWindowBuddyId ?? undefined,
  );
  const [layout, setLayout] = useState<DockLayout>(INITIAL_LAYOUT);
  const [activeAgentId, setActiveAgentId] = useState(initialWindowBuddyId ?? DEFAULT_BORDER_BUDDY_ID);
  const [draggingAgentId, setDraggingAgentId] = useState<string | null>(null);
  const [clickThrough, setClickThrough] = useState(false);
  const [fullInputCapture, setFullInputCapture] = useState(false);
  // Bumped periodically in native border mode to force all BuddyHotspot components
  // to re-measure and re-report their head rects. This keeps the minimal clickable
  // head regions alive even when no chat surface is open.
  const [headForceKey, setHeadForceKey] = useState(0);
  const [idle, setIdle] = useState(false);
  const [placements, setPlacements] = useState<AgentPlacements>(() =>
    loadStoredPlacements(defaultPlacements),
  );
  const [buddySettings, setBuddySettings] = useState<BuddySettingsMap>(() =>
    loadStoredBuddySettings(defaultBuddySettings),
  );
  const [dockSettings, setDockSettings] = useState<DockSettings>(() =>
    loadStoredDockSettings(DEFAULT_DOCK_SETTINGS),
  );
  const [gatewaySettings, setGatewaySettings] = useState<GatewaySettings>(() =>
    loadStoredGatewaySettings(DEFAULT_GATEWAY_SETTINGS),
  );
  const [buddyMessages, setBuddyMessages] = useState<Record<string, string>>(() =>
    Object.fromEntries(buddies.map((buddy) => [buddy.id, buddy.message ?? ""])),
  );
  const placementsRef = useRef<AgentPlacements>(placements);
  const dragStartedAtRef = useRef<number | null>(null);
  const overlayDragActiveRef = useRef(false);
  const overlayDragEndTimerRef = useRef<number | null>(null);
  const [healReport, setHealReport] = useState<DockHealReport | null>(null);
  const {
    controlsRef,
    passBannerRef,
    reportHitboxes,
    clearBuddyHitboxes,
    clearAllHitboxes,
    refreshChromeHitboxes,
    hasHitboxFailures,
  } = useDockHitboxRegistry(clickThroughRef, overlayDragActiveRef);

  const windowBuddy = useMemo(
    () => buddies.find((buddy) => buddy.id === windowBuddyId) ?? null,
    [windowBuddyId],
  );
  const resolvingWindowBuddy = windowBuddyId === undefined;
  const hiddenNativeWindow = windowBuddyId === HIDDEN_NATIVE_WINDOW_ID;
  const perBuddyWindow = Boolean(windowBuddy);
  const unifiedDock = !perBuddyWindow;
  const dockCollapsed = unifiedDock && dockSettings.collapsed;
  const dockRenderMode = dockSettings.renderMode;
  const visibleBuddies = useMemo(() => {
    const roster = windowBuddy ? [windowBuddy] : buddies;
    return roster.filter((buddy) => buddySettings[buddy.id]?.enabled !== false);
  }, [windowBuddy, buddySettings]);
  const multiMonitor = useMemo(
    () => new URLSearchParams(window.location.search).get("multiMonitor") === "true",
    [],
  );
  const browserPreview = useMemo(() => isBrowserPreviewSurface(), []);
  const nativeUnifiedDock = useMemo(() => {
    if (!unifiedDock || browserPreview) {
      return false;
    }

    try {
      return getCurrentWebviewWindow().label === "border-dock";
    } catch {
      return false;
    }
  }, [browserPreview, unifiedDock]);
  const effectiveRenderMode = unifiedDock ? dockRenderMode : FULL_RENDER_MODE;
  const passThroughAvailable = !nativeUnifiedDock;
  const gatewayEnabled = nativeUnifiedDock || browserPreview;
  const gatewaySource = browserPreview ? "browser-preview" : "border-dock";
  const gateway = useBuddyGateway({
    settings: gatewaySettings,
    source: gatewaySource,
    enabled: gatewayEnabled,
    onBubble: (buddyId, text) => {
      setBuddyMessages((current) => ({
        ...current,
        [buddyId]: text,
      }));
      setActiveAgentId(buddyId);
    },
  });
  const windowBuddyPlacement = windowBuddy
    ? placements[windowBuddy.id] ?? defaultPlacements[windowBuddy.id]
    : null;
  const windowBuddyBubbleVisible = Boolean(
    windowBuddy &&
      windowBuddy.id === activeAgentId &&
      windowBuddy.message &&
      windowBuddyPlacement?.state === "tucked",
  );

  useEffect(() => {
    placementsRef.current = placements;
  }, [placements]);

  useEffect(() => {
    if (windowBuddyId !== undefined) {
      return;
    }

    let mounted = true;

    async function resolveWindowBuddyId() {
      const buddyId = await getCurrentBuddyId();

      if (mounted) {
        setWindowBuddyId(buddyId);
      }
    }

    resolveWindowBuddyId();

    return () => {
      mounted = false;
    };
  }, [windowBuddyId]);

  useEffect(() => {
    if (windowBuddy) {
      setActiveAgentId(windowBuddy.id);
    }
  }, [windowBuddy]);

  useEffect(() => {
    let mounted = true;

    async function configureWindow() {
      if (resolvingWindowBuddy || hiddenNativeWindow) {
        return;
      }

      await setDockInputEnabled(true);

      try {
        if (windowBuddy && windowBuddyPlacement) {
          const nextLayout = await invoke<BuddyWindowLayout>("configure_buddy_window", {
            request: {
              buddyId: windowBuddy.id,
              edge: windowBuddyPlacement.edge,
              state: windowBuddyPlacement.state,
              slot:
                windowBuddyPlacement.state === "tucked"
                  ? windowBuddyPlacement.slot
                  : windowBuddy.dockSlot,
              bubbleVisible: windowBuddyBubbleVisible,
            },
          });

          if (mounted) {
            setLayout({
              monitors: [nextLayout.monitor],
              activeMonitorIds: [nextLayout.monitor.id],
              bounds: nextLayout.bounds,
              multiMonitor: false,
            });
          }

          return;
        }

        const nextLayout = await invoke<DockLayout>("configure_border_dock", {
          multiMonitor,
        });

        if (mounted) {
          setLayout(nextLayout);
        }
      } catch {
        if (mounted) {
          setLayout(INITIAL_LAYOUT);
        }
      }
    }

    configureWindow();

    return () => {
      mounted = false;
      setDockInputEnabled(true);
    };
  }, [
    multiMonitor,
    hiddenNativeWindow,
    resolvingWindowBuddy,
    windowBuddy,
    windowBuddyPlacement?.edge,
    windowBuddyPlacement?.state,
    windowBuddyBubbleVisible,
  ]);

  useEffect(() => {
    localStorage.setItem(PLACEMENT_STORAGE_KEY, JSON.stringify(placements));
  }, [placements]);

  useEffect(() => {
    localStorage.setItem(SETTINGS_STORAGE_KEY, JSON.stringify(buddySettings));
  }, [buddySettings]);

  useEffect(() => {
    localStorage.setItem(DOCK_SETTINGS_STORAGE_KEY, JSON.stringify(dockSettings));
  }, [dockSettings]);

  useEffect(() => {
    localStorage.setItem(GATEWAY_SETTINGS_STORAGE_KEY, JSON.stringify(gatewaySettings));
  }, [gatewaySettings]);

  useEffect(() => {
    setBuddySettings((current) => {
      const hermesSettings = current.hermes;
      const nextLabel = connectionLabelForState(gateway.state);

      if (!hermesSettings || hermesSettings.connectionLabel === nextLabel) {
        return current;
      }

      return {
        ...current,
        hermes: {
          ...hermesSettings,
          connectionLabel: nextLabel,
        },
      };
    });
  }, [gateway.state]);

  useEffect(() => {
    if (!nativeUnifiedDock) {
      return;
    }

    void bbLog("info", "desktop dock ready", {
      gatewayEnabled,
      gatewayState: gateway.state,
      renderMode: effectiveRenderMode,
    });

    const heartbeat = window.setInterval(() => {
      void bbLog("info", "heartbeat", {
        gatewayState: gateway.state,
        gatewayDetail: gateway.detail,
        activeAgentId,
        clickThrough,
        dockCollapsed,
        draggingAgentId,
        hitboxFailures: hasHitboxFailures(),
      });
    }, 8000);

    return () => window.clearInterval(heartbeat);
  }, [
    activeAgentId,
    clickThrough,
    dockCollapsed,
    draggingAgentId,
    effectiveRenderMode,
    gateway.detail,
    gateway.state,
    gatewayEnabled,
    hasHitboxFailures,
    nativeUnifiedDock,
  ]);

  useEffect(() => {
    if (dockCollapsed) {
      clearAllHitboxes();
      return;
    }

    refreshChromeHitboxes();
  }, [dockCollapsed, effectiveRenderMode, clearAllHitboxes, refreshChromeHitboxes]);

  useEffect(() => {
    const fallbackId = "__dock_head_fallbacks__";

    if (!nativeUnifiedDock || dockCollapsed || clickThroughRef.current) {
      clearBuddyHitboxes(fallbackId);
      return;
    }

    const viewportWidth = window.innerWidth;
    const viewportHeight = window.innerHeight;
    const boxes = visibleBuddies.map((buddy) =>
      fallbackHitboxForPlacement(
        placements[buddy.id] ?? defaultPlacements[buddy.id],
        viewportWidth,
        viewportHeight,
      ),
    );

    reportHitboxes(fallbackId, boxes);
    void bbLog("info", "registered fallback buddy head hitboxes", {
      count: boxes.length,
      boxes,
    });
  }, [
    clearBuddyHitboxes,
    dockCollapsed,
    headForceKey,
    nativeUnifiedDock,
    placements,
    reportHitboxes,
    visibleBuddies,
    clickThrough,
  ]);

  useEffect(() => {
    const captureId = "__debug_full_input_capture__";

    if (!nativeUnifiedDock || dockCollapsed || clickThroughRef.current || !fullInputCapture) {
      clearBuddyHitboxes(captureId);
      return;
    }

    const box = {
      x: 0,
      y: 0,
      w: Math.max(1, window.innerWidth),
      h: Math.max(1, window.innerHeight),
    };

    reportHitboxes(captureId, [box]);
    void bbLog("warn", "debug full input capture enabled", { box });
  }, [
    clearBuddyHitboxes,
    clickThrough,
    dockCollapsed,
    fullInputCapture,
    nativeUnifiedDock,
    reportHitboxes,
  ]);

  const markOverlayDragActive = useCallback(() => {
    overlayDragActiveRef.current = true;

    if (overlayDragEndTimerRef.current) {
      window.clearTimeout(overlayDragEndTimerRef.current);
    }
  }, []);

  const markOverlayDragEnded = useCallback(() => {
    if (overlayDragEndTimerRef.current) {
      window.clearTimeout(overlayDragEndTimerRef.current);
    }

    overlayDragEndTimerRef.current = window.setTimeout(() => {
      overlayDragActiveRef.current = false;
      refreshChromeHitboxes();
    }, 280);
  }, [refreshChromeHitboxes]);

  const recallBuddiesToBorder = useCallback(async () => {
    dragRef.current = null;
    setDraggingAgentId(null);
    setActiveAgentId(DEFAULT_BORDER_BUDDY_ID);
    setClickThroughMode(false);

    setDockSettings((current) => ({
      ...current,
      collapsed: false,
      renderMode: "head+bubble",
    }));

    const nextPlacements = { ...defaultPlacements };
    setPlacements(nextPlacements);
    placementsRef.current = nextPlacements;

    if (nativeUnifiedDock) {
      try {
        const nextLayout = await invoke<DockLayout>("configure_border_dock", {
          multiMonitor,
        });
        setLayout(nextLayout);
      } catch {
        // Keep recalled placements even if the native window reset fails.
      }
    }

    if (!dockCollapsed && !clickThroughRef.current) {
      refreshChromeHitboxes();
    }

    if (nativeUnifiedDock) {
      // Force immediate re-measure of all heads after recall in border mode.
      setHeadForceKey((k) => k + 1);
      void bbLog("info", "border recall: forcing head hitbox re-measure");
      // Extra visibility for tomorrow's debug session.
      void bbLog("info", "border recall complete - heads should now be reporting via the hotspots + headForceKey");
    }

    void bbLog("info", "recalled buddies to border", {
      hermes: nextPlacements.hermes,
    });

    return nextPlacements;
  }, [dockCollapsed, multiMonitor, nativeUnifiedDock, refreshChromeHitboxes]);

  const performSelfHeal = useCallback(
    async (options?: { panic?: boolean; routine?: boolean }) => {
      if (overlayDragActiveRef.current) {
        return createHealReport([]);
      }

      const actions: DockHealAction[] = [];

      if (
        draggingAgentId &&
        dragStartedAtRef.current &&
        Date.now() - dragStartedAtRef.current > STUCK_DRAG_TIMEOUT_MS
      ) {
        dragRef.current = null;
        setDraggingAgentId(null);
        actions.push("cleared-stuck-drag");
      }

      if (options?.panic) {
        if (clickThroughRef.current) {
          clickThroughRef.current = false;
          setClickThrough(false);
          actions.push("disabled-pass-through");
        }

        if (dockCollapsed) {
          setDockSettings((current) => ({
            ...current,
            collapsed: false,
          }));
          actions.push("expanded-dock");
        }

        await recallBuddiesToBorder();
        actions.push("recalled-buddies", "restored-overlay");
      }

      const shouldResetPointer =
        options?.panic || hasHitboxFailures() || actions.includes("cleared-stuck-drag");

      if (nativeUnifiedDock && shouldResetPointer) {
        try {
          await invoke("reset_dock_input");
          actions.push("restored-pointer");
        } catch {
          await setDockInputEnabled(true);
        }
      } else if (!nativeUnifiedDock && !clickThroughRef.current) {
        await setDockInputEnabled(true);
        actions.push("restored-pointer");
      }

      if (dockCollapsed || clickThroughRef.current) {
        clearAllHitboxes();
      } else {
        refreshChromeHitboxes();
      }
      actions.push("refreshed-hitboxes");

      const report = createHealReport(actions);
      const meaningfulActions = actions.filter((action) => action !== "refreshed-hitboxes");

      if (options?.panic || meaningfulActions.length > 0) {
        setHealReport(report);
      }

      return report;
    },
    [
      clearAllHitboxes,
      dockCollapsed,
      draggingAgentId,
      hasHitboxFailures,
      nativeUnifiedDock,
      recallBuddiesToBorder,
      refreshChromeHitboxes,
    ],
  );

  function setClickThroughMode(enabled: boolean) {
    if (nativeUnifiedDock && enabled) {
      // Emergency "Force Pass" for border mode debugging/recovery.
      // When the user cannot click buddies at all, this clears the per-buddy
      // hitboxes (heads + any panels) so the entire overlay becomes input-transparent
      // (clicks go through to the desktop). The "Catch" / Heal path will restore them.
      // This is intentionally allowed now because "Pass not working" was blocking recovery.
      clickThroughRef.current = true;
      setClickThrough(true);
      setFullInputCapture(false);
      clearAllHitboxes();
      void bbLog("warn", "emergency pass-through (hitboxes cleared) in native border mode — clicks should now reach the desktop", {
        reason: "user-forced-for-recovery",
      });
      // Do not auto-heal here; let the user explicitly Heal or Catch to bring buddies back.
      return;
    }

    clickThroughRef.current = enabled;
    setClickThrough(enabled);
    if (enabled) {
      setFullInputCapture(false);
    }
    void bbLog(enabled ? "warn" : "info", "click-through", { enabled });

    if (nativeUnifiedDock) {
      void invoke("reset_dock_input").catch(() => setDockInputEnabled(true));
      if (enabled) {
        clearAllHitboxes();
      } else {
        refreshChromeHitboxes();
      }
      return;
    }

    setBuddyWindowInteractive(!enabled);
  }

  useEffect(() => {
    if (!nativeUnifiedDock) {
      return;
    }

    let cancelled = false;

    void (async () => {
      await recallBuddiesToBorder();
      if (!cancelled) {
        void bbLog("info", "startup test-ready", {
          renderMode: "head+bubble",
          collapsed: false,
          clickThrough: false,
        });
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [nativeUnifiedDock, recallBuddiesToBorder]);

  // In native border mode, periodically force all visible head hitboxes to be
  // re-measured and re-reported. This is the most reliable way to keep the
  // minimal clickable regions for the buddies on the edge alive when no chat
  // surface is open. The individual hotspots depend on headForceKey in their
  // measurement effect.
  useEffect(() => {
    if (!nativeUnifiedDock || dockCollapsed) {
      return;
    }

    const id = window.setInterval(() => {
      setHeadForceKey((k) => k + 1);
      // Also nudge a flush in case only chrome changed.
      refreshChromeHitboxes();
    }, 650);

    // Initial force shortly after mount.
    const initial = window.setTimeout(() => setHeadForceKey((k) => k + 1), 120);

    return () => {
      clearInterval(id);
      clearTimeout(initial);
    };
  }, [nativeUnifiedDock, dockCollapsed, refreshChromeHitboxes]);

  useEffect(() => {
    async function registerDockShortcuts() {
      // Pass-through shortcut disabled during first-connection testing — Ctrl+Alt+B
      // was turning click-through on and making Hermes feel "crashed".

      if (!unifiedDock) {
        return;
      }

      try {
        await register(DOCK_COLLAPSE_SHORTCUT, (event) => {
          if (event.state === "Pressed") {
            setDockSettings((current) => ({
              ...current,
              collapsed: !current.collapsed,
            }));
          }
        });
      } catch {
        // The visible Hide button still works when the global shortcut is unavailable.
      }

      try {
        await register(DOCK_RECOVER_SHORTCUT, (event) => {
          if (event.state === "Pressed") {
            performSelfHeal({ panic: true });
          }
        });
      } catch {
        // Routine self-heal still runs on the maintenance interval.
      }
    }

    registerDockShortcuts();

    return () => {
      unregister(DOCK_COLLAPSE_SHORTCUT).catch(() => {});
      unregister(DOCK_RECOVER_SHORTCUT).catch(() => {});
      setDockInputEnabled(true);
    };
  }, [performSelfHeal, unifiedDock]);

  useEffect(() => {
    dragStartedAtRef.current = draggingAgentId ? Date.now() : null;
  }, [draggingAgentId]);

  useEffect(() => {
    if (!nativeUnifiedDock) {
      return;
    }

    let unlistenMoved: (() => void) | null = null;

    getCurrentWindow()
      .onMoved(() => {
        markOverlayDragActive();
        markOverlayDragEnded();
      })
      .then((unlisten) => {
        unlistenMoved = unlisten;
      })
      .catch(() => {});

    const intervalId = window.setInterval(() => {
      performSelfHeal({ routine: true });
    }, SELF_HEAL_INTERVAL_MS);

    function handleVisibilityChange() {
      if (document.visibilityState === "visible") {
        performSelfHeal({ routine: true });
      }
    }

    window.addEventListener("visibilitychange", handleVisibilityChange);

    return () => {
      unlistenMoved?.();

      if (overlayDragEndTimerRef.current) {
        window.clearTimeout(overlayDragEndTimerRef.current);
      }

      overlayDragActiveRef.current = false;
      window.clearInterval(intervalId);
      window.removeEventListener("visibilitychange", handleVisibilityChange);
    };
  }, [markOverlayDragActive, markOverlayDragEnded, nativeUnifiedDock, performSelfHeal]);

  useEffect(() => {
    if (!unifiedDock || dockCollapsed) {
      return;
    }

    function handleKeyDown(event: KeyboardEvent) {
      if (event.key !== "Escape") {
        return;
      }

      setDockSettings((current) => ({
        ...current,
        collapsed: true,
      }));
    }

    window.addEventListener("keydown", handleKeyDown);

    return () => {
      window.removeEventListener("keydown", handleKeyDown);
    };
  }, [dockCollapsed, unifiedDock]);

  useEffect(() => {
    function handlePointerMove(event: PointerEvent) {
      markActivity();
      moveBuddyFromPoint(event.clientX, event.clientY);
    }

    function handlePointerEnd() {
      finishBuddyDrag();
    }

    if (draggingAgentId) {
      window.addEventListener("pointermove", handlePointerMove);
      window.addEventListener("pointerup", handlePointerEnd);
      window.addEventListener("pointercancel", handlePointerEnd);
    }

    return () => {
      window.removeEventListener("pointermove", handlePointerMove);
      window.removeEventListener("pointerup", handlePointerEnd);
      window.removeEventListener("pointercancel", handlePointerEnd);
    };
  }, [draggingAgentId]);

  useEffect(() => {
    if (!perBuddyWindow || !windowBuddy) {
      return;
    }

    let cleanupMovedListener: (() => void) | null = null;

    getCurrentWindow()
      .onMoved(() => {
        markActivity();

        const currentPlacement =
          placementsRef.current[windowBuddy.id] ?? defaultPlacements[windowBuddy.id];

        if (currentPlacement.state !== "free") {
          return;
        }

        setDraggingAgentId(windowBuddy.id);

        if (nativeMoveSnapTimerRef.current) {
          window.clearTimeout(nativeMoveSnapTimerRef.current);
        }

        nativeMoveSnapTimerRef.current = window.setTimeout(async () => {
          try {
            const result = await invoke<BuddySnapResult>("snap_buddy_window", {
              request: {
                buddyId: windowBuddy.id,
                bubbleVisible: windowBuddyBubbleVisible,
              },
            });

            if (result.snapped && result.edge) {
              setPlacements((current) => ({
                ...current,
                [windowBuddy.id]: {
                  state: "tucked",
                  edge: result.edge as Edge,
                  slot: result.slot ?? windowBuddy.dockSlot,
                },
              }));
            }
          } catch {
            // Browser previews and unsupported window managers keep the free placement.
          } finally {
            setDraggingAgentId(null);
          }
        }, 240);
      })
      .then((unlisten) => {
        cleanupMovedListener = unlisten;
      })
      .catch(() => {});

    return () => {
      cleanupMovedListener?.();

      if (nativeMoveSnapTimerRef.current) {
        window.clearTimeout(nativeMoveSnapTimerRef.current);
      }
    };
  }, [perBuddyWindow, windowBuddy, windowBuddyBubbleVisible]);

  useEffect(() => {
    markActivity();

    return () => {
      if (idleTimerRef.current) {
        window.clearTimeout(idleTimerRef.current);
      }
      // Drop any armed (but not yet started) buddy press gesture on unmount.
      pendingDragCleanupRef.current?.();
    };
  }, []);


  useEffect(() => {
    if (!perBuddyWindow || draggingAgentId || !idle) {
      setBuddyWindowInteractive(true);
      return;
    }

    const canPulseClickThrough =
      windowBuddy?.visible === "faint" ||
      (windowBuddyPlacement?.state === "tucked" && !windowBuddyBubbleVisible);

    if (!canPulseClickThrough) {
      return;
    }

    setBuddyWindowInteractive(false);
    const restoreTimer = window.setTimeout(() => {
      setBuddyWindowInteractive(true);
    }, IDLE_CLICK_THROUGH_PULSE);

    return () => {
      window.clearTimeout(restoreTimer);
      setBuddyWindowInteractive(true);
    };
  }, [
    draggingAgentId,
    idle,
    perBuddyWindow,
    windowBuddy?.visible,
    windowBuddyBubbleVisible,
    windowBuddyPlacement?.state,
  ]);

  function markActivity() {
    setIdle(false);

    if (idleTimerRef.current) {
      window.clearTimeout(idleTimerRef.current);
    }

    idleTimerRef.current = window.setTimeout(() => {
      setIdle(true);
    }, IDLE_FADE_DELAY);
  }

  function startBuddyDrag(buddy: DockBuddy, event: ReactPointerEvent<HTMLElement>) {
    if (clickThrough) {
      return;
    }

    markActivity();

    const stageRect = stageRef.current?.getBoundingClientRect();

    if (!stageRect) {
      return;
    }

    // IMPORTANT: do NOT preventDefault here. A plain press/release must keep
    // emitting the button's `click` event so the buddy can be activated and the
    // chat/connection surface opened. We only promote the gesture to a drag once
    // the pointer travels past DRAG_ACTIVATION_THRESHOLD. This is the fix for
    // "I can't click my first border buddy to test the Hermes connection" — the
    // previous code dragged (and popped the buddy off the border) on every tap.
    const pointerId = event.pointerId;
    const startClientX = event.clientX;
    const startClientY = event.clientY;
    const captureTarget = event.currentTarget;
    const currentPlacement = placements[buddy.id] ?? defaultPlacements[buddy.id];
    let dragStarted = false;

    const cleanup = () => {
      window.removeEventListener("pointermove", handleMove);
      window.removeEventListener("pointerup", handleEnd);
      window.removeEventListener("pointercancel", handleEnd);
      if (pendingDragCleanupRef.current === cleanup) {
        pendingDragCleanupRef.current = null;
      }
    };

    const beginDrag = () => {
      dragStarted = true;
      markActivity();

      const pointerX = startClientX - stageRect.left;
      const pointerY = startClientY - stageRect.top;
      const freePlacement =
        currentPlacement.state === "free"
          ? currentPlacement
          : getFreePlacement(buddy.edge, pointerX, pointerY, stageRect);

      try {
        captureTarget.setPointerCapture(pointerId);
      } catch {
        // Window-level listeners keep dragging reliable on desktop webviews.
      }

      if (perBuddyWindow) {
        dragRef.current = null;
        setActiveAgentId(buddy.id);
        setDraggingAgentId(buddy.id);
        setPlacements((current) => ({
          ...current,
          [buddy.id]: centerFreePlacement(freePlacement.edge, stageRect),
        }));
        startNativeBuddyDrag(buddy.id, freePlacement.edge, windowBuddyBubbleVisible);
        return;
      }

      dragRef.current = {
        agentId: buddy.id,
        offsetX: pointerX - freePlacement.x,
        offsetY: pointerY - freePlacement.y,
      };

      setActiveAgentId(buddy.id);
      setDraggingAgentId(buddy.id);
      setPlacements((current) => ({
        ...current,
        [buddy.id]: freePlacement,
      }));
    };

    function handleMove(moveEvent: PointerEvent) {
      if (moveEvent.pointerId !== pointerId || dragStarted) {
        return;
      }

      const dx = moveEvent.clientX - startClientX;
      const dy = moveEvent.clientY - startClientY;

      if (Math.hypot(dx, dy) >= DRAG_ACTIVATION_THRESHOLD) {
        cleanup();
        beginDrag();
      }
    }

    function handleEnd(endEvent: PointerEvent) {
      if (endEvent.pointerId !== pointerId) {
        return;
      }

      cleanup();
      // If no drag was started this was a tap — the button's onClick handles activation.
    }

    // Replace any stale pending gesture before arming a new one.
    pendingDragCleanupRef.current?.();
    pendingDragCleanupRef.current = cleanup;
    window.addEventListener("pointermove", handleMove);
    window.addEventListener("pointerup", handleEnd);
    window.addEventListener("pointercancel", handleEnd);
  }


  function moveBuddy(event: ReactPointerEvent<HTMLElement>) {
    markActivity();
    moveBuddyFromPoint(event.clientX, event.clientY);
  }

  function moveBuddyFromPoint(clientX: number, clientY: number) {
    const activeDrag = dragRef.current;
    const stageRect = stageRef.current?.getBoundingClientRect();

    if (!activeDrag || !stageRect) {
      return;
    }

    const pointerX = clientX - stageRect.left;
    const pointerY = clientY - stageRect.top;

    setPlacements((current) => {
      const currentPlacement = current[activeDrag.agentId] ?? defaultPlacements[activeDrag.agentId];
      const edge = currentPlacement.edge;

      return {
        ...current,
        [activeDrag.agentId]: {
          state: "free",
          edge,
          x: clamp(
            pointerX - activeDrag.offsetX,
            FREE_AGENT_MARGIN,
            stageRect.width - FREE_AGENT_SIZE - FREE_AGENT_MARGIN,
          ),
          y: clamp(
            pointerY - activeDrag.offsetY,
            FREE_AGENT_MARGIN,
            stageRect.height - FREE_AGENT_SIZE - FREE_AGENT_MARGIN,
          ),
        },
      };
    });
  }

  function finishBuddyDrag() {
    markActivity();
    const activeDrag = dragRef.current;
    const stageRect = stageRef.current?.getBoundingClientRect();

    if (!activeDrag || !stageRect) {
      return;
    }

    dragRef.current = null;
    setDraggingAgentId(null);

    setPlacements((current) => {
      const currentPlacement = current[activeDrag.agentId] ?? defaultPlacements[activeDrag.agentId];

      if (currentPlacement.state !== "free") {
        return current;
      }

      const snappedEdge = getSnapEdge(currentPlacement, stageRect);

      if (!snappedEdge) {
        return current;
      }

      return {
        ...current,
        [activeDrag.agentId]: {
          state: "tucked",
          edge: snappedEdge,
          slot: getPreviewDockSlot(snappedEdge, currentPlacement, stageRect),
        },
      };
    });
  }

  function beginOverlayWindowDrag() {
    // Native move is handled by data-tauri-drag-region on the Move control.
    // We only pause hitbox/self-heal updates while the compositor is dragging.
    markOverlayDragActive();
  }

  function beginOverlayWindowResize(direction: ResizeDirection) {
    markOverlayDragActive();

    try {
      void getCurrentWindow().startResizeDragging(direction);
    } catch {
      markOverlayDragEnded();
    }
  }

  function tuckBuddy(buddyId: string) {
    markActivity();
    setPlacements((current) => {
      const currentPlacement = current[buddyId] ?? defaultPlacements[buddyId];

      return {
        ...current,
        [buddyId]: {
          state: "tucked",
          edge: currentPlacement.edge,
          slot:
            currentPlacement.state === "tucked"
              ? currentPlacement.slot
              : defaultDockSlot(buddyId),
        },
      };
    });
  }

  // Undock a buddy into the free interactive surface (composer + settings).
  // This is the non-drag path to "open the buddy to chat": clicking the ambient
  // speech bubble pops the buddy into the center as a free, fully interactive
  // floating surface. Dragging the head past the threshold does the same thing.
  function popBuddyOut(buddyId: string) {
    markActivity();
    setClickThroughMode(false);
    setActiveAgentId(buddyId);

    const stageRect = stageRef.current?.getBoundingClientRect();
    if (!stageRect) {
      return;
    }

    const currentPlacement = placements[buddyId] ?? defaultPlacements[buddyId];
    if (currentPlacement.state === "free") {
      return;
    }

    setPlacements((current) => ({
      ...current,
      [buddyId]: centerFreePlacement(currentPlacement.edge, stageRect),
    }));

    if (perBuddyWindow) {
      startNativeBuddyDrag(buddyId, currentPlacement.edge, windowBuddyBubbleVisible);
    }
  }


  if (resolvingWindowBuddy || hiddenNativeWindow) {
    return null;
  }

  return (
    <main
      className={[
        "border-dock",
        perBuddyWindow ? "border-dock--per-buddy" : "border-dock--unified",
        browserPreview ? "border-dock--preview" : "",
        unifiedDock ? `border-dock--render-${effectiveRenderMode.replace("+", "-plus-")}` : "",
        dockCollapsed ? "border-dock--collapsed" : "",
        idle ? "border-dock--idle" : "",
        clickThrough ? "border-dock--click-through" : "",
      ].join(" ")}
      data-window-buddy={windowBuddy?.id ?? "dock"}
      aria-label="Border Buddies dock"
      onPointerEnter={markActivity}
      onPointerMove={markActivity}
      onPointerDownCapture={(event) => {
        if (!nativeUnifiedDock) {
          return;
        }

        const target = event.target instanceof HTMLElement ? event.target : null;
        void bbLog("info", "dock pointer down", {
          x: Math.round(event.clientX),
          y: Math.round(event.clientY),
          target: target?.className || target?.tagName || "unknown",
          buddy: target?.closest("[data-buddy]")?.getAttribute("data-buddy") ?? null,
          fullInputCapture,
        });
      }}
    >
      {clickThrough ? (
        <div className="dock-pass-banner" ref={passBannerRef} role="status">
          <span>Pass-through on — Hermes is not clickable.</span>
          <button type="button" onClick={() => setClickThroughMode(false)}>
            Catch (resume clicks)
          </button>
        </div>
      ) : null}
      <div
        className="dock-stage"
        data-monitor-count={layout.activeMonitorIds.length}
        ref={stageRef}
      >
        {unifiedDock ? (
          <DockChrome
            clickThrough={clickThrough}
            collapsed={dockCollapsed}
            controlsRef={controlsRef}
            onCollapseToggle={() =>
              setDockSettings((current) => ({
                ...current,
                collapsed: !current.collapsed,
              }))
            }
            onCycleRenderMode={() =>
              setDockSettings((current) => ({
                ...current,
                renderMode: cycleDockRenderMode(current.renderMode),
              }))
            }
            onBeginOverlayDrag={beginOverlayWindowDrag}
            onBeginOverlayResize={() => beginOverlayWindowResize("SouthEast")}
            onRecall={() => void recallBuddiesToBorder()}
            onRecover={() => performSelfHeal({ panic: true })}
            passThroughAvailable={passThroughAvailable}
            onToggleClickThrough={() => setClickThroughMode(!clickThroughRef.current)}
            fullInputCapture={fullInputCapture}
            onToggleFullInputCapture={() => {
              setClickThroughMode(false);
              setFullInputCapture((enabled) => !enabled);
            }}
            healReport={healReport}
            renderMode={effectiveRenderMode}
            nativeUnifiedDock={nativeUnifiedDock}
          />
        ) : null}
        {!dockCollapsed
          ? visibleBuddies.map((buddy) => {
              const profile = BUDDY_PROFILES[buddy.id];
              const hasGateway = profile ? buddyHasGateway(profile) : false;

              return (
                <BuddyHotspot
                  buddy={{
                    ...buddy,
                    message: buddyMessages[buddy.id] ?? buddy.message,
                  }}
                  active={buddy.id === activeAgentId}
                  collapsed={dockCollapsed}
                  dragging={buddy.id === draggingAgentId}
                  gatewayAutoConnect={gatewaySettings.autoConnect}
                  gatewayBusy={hasGateway ? gateway.busy : false}
                  gatewayDetail={hasGateway ? gateway.detail : null}
                  gatewayState={hasGateway ? gateway.state : "idle"}
                  gatewayUrl={gatewaySettings.url}
                  hasGateway={hasGateway}
                  key={buddy.id}
                  onDragEnd={finishBuddyDrag}
                  onDragMove={moveBuddy}
                  onDragStart={(event) => startBuddyDrag(buddy, event)}
                  onActivate={() => {
                    setClickThroughMode(false);
                    setActiveAgentId(buddy.id);
                  }}
                  onDeactivate={() => {
                    if (activeAgentId === buddy.id) {
                      setActiveAgentId("");
                    }
                  }}
                  onGatewayConnect={gateway.connect}
                  onGatewayDisconnect={gateway.disconnect}
                  onGatewaySettingsChange={setGatewaySettings}
                  onManualTuck={() => tuckBuddy(buddy.id)}
                  onRequestInteract={() => popBuddyOut(buddy.id)}
                  onClearHitboxes={() => clearBuddyHitboxes(buddy.id)}
                  onReportHitboxes={(boxes) => reportHitboxes(buddy.id, boxes)}
                  headForceKey={headForceKey}

                  onSendChat={(text) => {
                    const sent = gateway.sendChat(buddy.id, text);
                    void bbLog(sent ? "info" : "warn", "gateway sendChat", {
                      buddyId: buddy.id,
                      sent,
                      gatewayState: gateway.state,
                    });
                    return sent;
                  }}
                  perBuddyWindow={perBuddyWindow}
                  placement={placements[buddy.id] ?? defaultPlacements[buddy.id]}
                  renderMode={effectiveRenderMode}
                  settings={buddySettings[buddy.id] ?? defaultBuddySettings[buddy.id]}
                  onSettingsChange={(settings) =>
                    setBuddySettings((current) => ({
                      ...current,
                      [buddy.id]: normalizeBuddySettings(BUDDY_PROFILES[buddy.id], settings),
                    }))
                  }
                />
              );
            })
          : null}
      </div>
    </main>
  );
}

async function setBuddyWindowInteractive(interactive: boolean) {
  try {
    await invoke("set_buddy_window_interactive", { interactive });
  } catch {
    await setDockInputEnabled(interactive);
  }
}

async function startNativeBuddyDrag(buddyId: string, edge: Edge, bubbleVisible: boolean) {
  try {
    await invoke("configure_buddy_window", {
      request: {
        buddyId,
        edge,
        state: "free",
        slot: defaultDockSlot(buddyId),
        bubbleVisible,
      },
    });
    await getCurrentWindow().startDragging();
  } catch {
    // Browser previews and unsupported window managers keep the local drag behavior.
  }
}

function DockChrome({
  clickThrough,
  collapsed,
  controlsRef,
  healReport,
  onCollapseToggle,
  onCycleRenderMode,
  onBeginOverlayDrag,
  onBeginOverlayResize,
  onRecall,
  onRecover,
  passThroughAvailable,
  onToggleClickThrough,
  fullInputCapture,
  onToggleFullInputCapture,
  renderMode,
  nativeUnifiedDock,
}: {
  clickThrough: boolean;
  collapsed: boolean;
  controlsRef: (node: HTMLDivElement | null) => void;
  healReport: DockHealReport | null;
  onCollapseToggle: () => void;
  onCycleRenderMode: () => void;
  onBeginOverlayDrag: () => void;
  onBeginOverlayResize: () => void;
  onRecall: () => void;
  onRecover: () => void;
  passThroughAvailable: boolean;
  onToggleClickThrough: () => void;
  fullInputCapture: boolean;
  onToggleFullInputCapture: () => void;
  renderMode: DockRenderMode;
  nativeUnifiedDock?: boolean;
}) {
  const recentHeal =
    healReport && Date.now() - healReport.at < 20_000 ? healReport.actions.join(", ") : null;
  return (
    <div className="dock-chrome" ref={controlsRef}>
      {collapsed ? (
        <button
          className="dock-peek"
          type="button"
          onClick={onCollapseToggle}
          title="Expand Border Buddies dock"
        >
          Buddies
        </button>
      ) : (
        <div className="dock-controls" aria-label="Border Buddies window controls">
          <span className="dock-controls__hint">
            {clickThrough
              ? "Click-through on"
              : recentHeal
                ? "Self-heal check complete"
                : "Hermes on right edge · Recall if escaped"}
          </span>
          <button
            className="dock-control dock-control--mode"
            type="button"
            onClick={onCycleRenderMode}
            title="Cycle dock render mode"
          >
            {DOCK_RENDER_MODE_LABELS[renderMode]}
          </button>
          <button
            className="dock-control dock-control--move"
            data-tauri-drag-region
            type="button"
            onPointerDown={onBeginOverlayDrag}
            title="Move overlay window"
          >
            Move
          </button>
          <button
            className="dock-control dock-control--resize"
            type="button"
            onPointerDown={(event) => {
              event.preventDefault();
              onBeginOverlayResize();
            }}
            title="Resize overlay window"
          >
            Resize
          </button>
          <button
            className="dock-control dock-control--pass"
            type="button"
            // We deliberately allow the Pass button in native border mode now.
            // It acts as "Force Pass (emergency)" — clears all buddy hitboxes so the
            // overlay no longer intercepts clicks (full passthrough to desktop).
            // This is the escape hatch when you cannot click any buddies.
            // "Catch" or the Heal button will restore the head/panel regions.
            disabled={false}
            onClick={onToggleClickThrough}
            title={
              nativeUnifiedDock
                ? clickThrough
                  ? "Catch — restore the buddy head and panel click regions"
                  : "Force Pass (emergency) — clear hitboxes. Clicks will go through to the desktop until you Heal or Catch."
                : "Toggle click-through mode"
            }
          >
            {nativeUnifiedDock
              ? clickThrough
                ? "Catch"
                : "Force Pass"
              : passThroughAvailable
              ? clickThrough
                ? "Catch"
                : "Pass"
              : "Pass off"}
          </button>
          {nativeUnifiedDock ? (
            <button
              className="dock-control dock-control--recover"
              type="button"
              onClick={onToggleFullInputCapture}
              title={
                fullInputCapture
                  ? "Return pointer ownership to the desktop"
                  : "Take pointer ownership to interact with buddies"
              }
            >
              {fullInputCapture ? "Desktop" : "Interact"}
            </button>
          ) : null}
          <button
            className="dock-control dock-control--collapse"
            type="button"
            onClick={onCollapseToggle}
            title="Collapse dock and reclaim screen (Ctrl+Alt+H)"
          >
            Hide
          </button>
          <button
            className="dock-control dock-control--recall"
            type="button"
            onClick={onRecall}
            title="Recall escaped buddies to their border slots and restore full-screen overlay"
          >
            Recall
          </button>
          <button
            className="dock-control dock-control--recover"
            type="button"
            onClick={onRecover}
            title="Recover dock controls if the overlay feels stuck (Ctrl+Alt+Shift+R)"
          >
            Heal
          </button>
        </div>
      )}
    </div>
  );
}

function BuddyHotspot({
  active,
  buddy,
  collapsed,
  dragging,
  gatewayAutoConnect,
  gatewayBusy,
  gatewayDetail,
  gatewayState,
  gatewayUrl,
  hasGateway,
  onClearHitboxes,
  onDragEnd,
  onDragMove,
  onDragStart,
  onActivate,
  onDeactivate,
  onGatewayConnect,
  onGatewayDisconnect,
  onGatewaySettingsChange,
  onReportHitboxes,
  onRequestInteract,
  onSendChat,
  perBuddyWindow,
  placement,
  onManualTuck,
  renderMode,
  settings,
  onSettingsChange,
  headForceKey,
}: {
  active: boolean;
  buddy: DockBuddy;
  collapsed: boolean;
  dragging: boolean;
  gatewayAutoConnect: boolean;
  gatewayBusy: boolean;
  gatewayDetail: string | null;
  gatewayState: GatewayConnectionState;
  gatewayUrl: string;
  hasGateway: boolean;
  onClearHitboxes: () => void;
  onDragEnd: () => void;
  onDragMove: (event: ReactPointerEvent<HTMLElement>) => void;
  onDragStart: (event: ReactPointerEvent<HTMLElement>) => void;
  onActivate: () => void;
  onDeactivate: () => void;
  onGatewayConnect: () => void;
  onGatewayDisconnect: () => void;
  onGatewaySettingsChange: (settings: GatewaySettings) => void;
  onReportHitboxes: (boxes: Hitbox[]) => void;
  onRequestInteract: () => void;
  onSendChat: (text: string) => boolean;
  perBuddyWindow: boolean;
  placement: AgentPlacement;
  onManualTuck: () => void;
  renderMode: DockRenderMode;
  settings: BuddySettings;
  onSettingsChange: (settings: BuddySettings) => void;
  headForceKey?: number;
}) {

  const edge = placement.edge;
  const style = {
    "--agent-color": buddy.color,
    "--agent-accent": buddy.accentColor ?? buddy.color,
    "--agent-slot": placement.state === "tucked" ? `${placement.slot * 100}%` : "50%",
  } as CSSProperties;

  if (placement.state === "free") {
    Object.assign(style, {
      "--agent-x": `${placement.x}px`,
      "--agent-y": `${placement.y}px`,
    });
  }

  const headRef = useRef<HTMLButtonElement>(null);
  const surfaceRef = useRef<BuddySurfaceHandle>(null);
  const { setHitboxes } = useBuddyHitbox();

  // Interaction model: the buddy is either docked on the border (ambient,
  // bubble-only) or undocked/free (interactive composer + settings). The full
  // chat surface — and its larger, more fragile hitboxes — only exists in the
  // free state, which keeps the native border overlay tiny and stable.
  const isFree = placement.state === "free";
  // When free we always show the body figure as the drag handle. When tucked we
  // honor the dock render mode (head / head+bubble / bubble).
  const showHead = isFree || renderMode !== "bubble";
  // Free => interactive surface always. Tucked on border => ambient bubble by default,
  // but when the user activates (clicks head / opens chat) we allow the full interactive
  // composer + panel. This is required to have a usable "Hermes on the border" test
  // without forcing an undock for every interaction.
  //
  // See docs/FIX_LIST.md — the "Hermes buddy chat input not clickable in native unified border mode"
  // entry and the Opus gateway-gated-input fix. Previously surfaceInteractive was strictly
  // isFree, which meant no composer hitboxes (and thus no clickable textarea) while tucked
  // on the native border-dock, even after the disabled={busy} change.
  const showSurface = isFree || (renderMode !== "head" && active);
  const surfaceInteractive = isFree || active;
  const isSurfaceVisible = showSurface;

  const freeX = placement.state === "free" ? placement.x : null;
  const freeY = placement.state === "free" ? placement.y : null;

  // Depend on headForceKey so the parent can periodically force re-measure of heads
  // in native border mode (even for non-active tucked buddies).
  const measureAndReportHitboxes = useCallback(() => {
    if (collapsed) {
      onClearHitboxes();
      return;
    }

    const boxes: Hitbox[] = [];

    if (showHead && headRef.current) {
      const r = headRef.current.getBoundingClientRect();
      // Add a bit of padding to the head hitbox to make it easier to click in border mode.
      // The visual head (SVG etc.) may be slightly smaller than the button bounds.
      const pad = 4;
      boxes.push({
        x: Math.round(r.left - pad),
        y: Math.round(r.top - pad),
        w: Math.round(r.width + pad * 2),
        h: Math.round(r.height + pad * 2),
      });

      if (!isSurfaceVisible) {
        // Diagnostic: log the head-only rect being reported for tucked border buddies.
        // This will appear in the bb-ui logs so we can see what is being sent for the heads.
        // The padded version is what actually goes into the boxes sent to set_input_hitboxes.
        const sentX = Math.round(r.left - pad);
        const sentY = Math.round(r.top - pad);
        const sentW = Math.round(r.width + pad * 2);
        const sentH = Math.round(r.height + pad * 2);
        void bbLog("info", "reporting head hitbox (border mode)", {
          buddy: buddy.id,
          edge,
          state: placement.state,
          raw: { x: Math.round(r.left), y: Math.round(r.top), w: Math.round(r.width), h: Math.round(r.height) },
          sent: { x: sentX, y: sentY, w: sentW, h: sentH },
        });
      }
    }

    if (isSurfaceVisible && surfaceRef.current) {
      boxes.push(...surfaceRef.current.measureHitboxes());
    }

    if (perBuddyWindow) {
      setHitboxes(boxes);
    } else {
      onReportHitboxes(boxes);
    }
  }, [
    collapsed,
    onClearHitboxes,
    onReportHitboxes,
    perBuddyWindow,
    setHitboxes,
  ]);

  useLayoutEffect(() => {
    measureAndReportHitboxes();
  }, [
    freeX,
    freeY,
    gatewayBusy,
    gatewayDetail,
    gatewayState,
    headForceKey,
    isSurfaceVisible,
    measureAndReportHitboxes,
    placement.state,
    showHead,
  ]);

  // Harden: while the chat surface is open, keep re-reporting hitboxes on a
  // short cadence. Layout effects can miss late paints (fonts, history growth,
  // settings open/close) and leave the native input shape stale so the composer
  // stops accepting clicks. This guarantees the clickable region tracks the DOM
  // for the whole "open chat → Set → type → send" first-connection sequence.
  useEffect(() => {
    if (!isSurfaceVisible) {
      return;
    }

    const intervalId = window.setInterval(() => {
      measureAndReportHitboxes();
    }, 600);

    return () => window.clearInterval(intervalId);
  }, [isSurfaceVisible, measureAndReportHitboxes]);

  return (
    <section

      className={[
        "agent-hotspot",
        `agent-hotspot--${placement.state}`,
        `agent-hotspot--${edge}`,
        active ? "agent-hotspot--active" : "",
        dragging ? "agent-hotspot--dragging" : "",
        buddy.visible === "faint" ? "agent-hotspot--faint" : "",
      ].join(" ")}
      data-buddy={buddy.id}
      style={style}
      aria-label={`${buddy.shortName}: ${buddy.ownerLabel} ${buddy.role}`}
      onPointerEnter={() => {
        if (perBuddyWindow) {
          setDockInputEnabled(true);
        }
      }}
      onPointerLeave={() => {
        if (perBuddyWindow) {
          setDockInputEnabled(true);
        }
      }}
      onPointerMove={onDragMove}
      onPointerUp={onDragEnd}
      onPointerCancel={onDragEnd}
      onDoubleClick={onManualTuck}
    >
      {isSurfaceVisible ? (
        <BuddySurface
          ref={surfaceRef}
          buddy={buddy}
          dockSlot={placement.state === "tucked" ? placement.slot : 0.5}
          edge={edge}
          gatewayAutoConnect={gatewayAutoConnect}
          gatewayBusy={gatewayBusy}
          gatewayDetail={gatewayDetail}
          gatewayState={gatewayState}
          gatewayUrl={gatewayUrl}
          hasGateway={hasGateway}
          interactive={surfaceInteractive}
          message={buddy.message ?? ""}
          onGatewayConnect={onGatewayConnect}
          onGatewayDisconnect={onGatewayDisconnect}
          onGatewaySettingsChange={onGatewaySettingsChange}
          onLayoutChange={measureAndReportHitboxes}
          onRequestInteract={onRequestInteract}
          onRequestDock={onManualTuck}
          onSendChat={onSendChat}
          onSettingsChange={onSettingsChange}
          settings={settings}
        />

      ) : null}
      {showHead ? (
        <button
          ref={headRef}
          className="agent-button"
          type="button"
          onClick={() => {
            if (active) {
              onDeactivate();
            } else {
              onActivate();
            }
          }}
          onPointerDown={onDragStart}
        >
          <BuddyFigure buddyId={buddy.id} state={placement.state} />
        </button>
      ) : null}
    </section>
  );
}

function BuddyFigure({ buddyId, state }: { buddyId: string; state: AgentPlacement["state"] }) {
  if (buddyId === "hermes") {
    return state === "free" ? <HermesBody /> : <HermesHead />;
  }

  if (buddyId === "crab") {
    return state === "free" ? <ClawBody /> : <ClawHead />;
  }

  if (buddyId === "owl") {
    return state === "free" ? <OwlBody /> : <OwlHead />;
  }

  return state === "free" ? <FoxBody /> : <FoxHead />;
}

function HermesHead() {
  return (
    <svg className="agent-svg agent-svg--hermes" viewBox="0 0 128 128" aria-hidden="true">
      <defs>
        <linearGradient id="hermesHeadShell" x1="30" y1="10" x2="110" y2="130" gradientUnits="userSpaceOnUse">
          <stop offset="0" stopColor="#2f7dff" />
          <stop offset="0.52" stopColor="#111b34" />
          <stop offset="1" stopColor="#050913" />
        </linearGradient>
      </defs>
      <circle className="hermes-halo" cx="66" cy="64" r="55" />
      <path className="hermes-accent hermes-crest" d="M55 20c10-18 28-18 39 0-14-4-27-4-39 0Z" />
      <path className="hermes-shell" d="M25 72c0-31 22-55 51-55 25 0 43 18 43 43 0 33-27 54-58 49-22-3-36-17-36-37Z" fill="url(#hermesHeadShell)" />
      <path className="hermes-line" d="M38 50c18-18 45-22 68-6" />
      <ellipse className="eye" cx="56" cy="61" rx="13" ry="16" />
      <ellipse className="eye" cx="88" cy="59" rx="13" ry="16" />
      <circle className="pupil hermes-pupil" cx="60" cy="62" r="5" />
      <circle className="pupil hermes-pupil" cx="92" cy="60" r="5" />
      <circle className="shine" cx="63" cy="57" r="2" />
      <circle className="shine" cx="95" cy="55" r="2" />
      <path className="hermes-line hermes-smile" d="M58 82c13 11 29 11 43-1" />
      <path className="hermes-cape" d="M108 61c13 6 18 15 17 27-10-4-18-10-24-19Z" />
      <path className="hermes-star" d="M31 30l3 7 7 3-7 3-3 7-3-7-7-3 7-3z" />
    </svg>
  );
}

function HermesBody() {
  return (
    <svg className="agent-svg agent-svg--hermes-body" viewBox="0 0 128 160" aria-hidden="true">
      <defs>
        <linearGradient id="hermesBodyShell" x1="30" y1="10" x2="110" y2="150" gradientUnits="userSpaceOnUse">
          <stop offset="0" stopColor="#2f7dff" />
          <stop offset="0.52" stopColor="#111b34" />
          <stop offset="1" stopColor="#050913" />
        </linearGradient>
      </defs>
      <ellipse className="hermes-halo" cx="64" cy="78" rx="60" ry="72" />
      <path className="hermes-accent hermes-crest" d="M54 22c10-18 29-18 40 0-15-4-28-4-40 0Z" />
      <path className="hermes-cape" d="M85 74c22 10 36 34 31 70-20-7-35-22-42-45Z" />
      <ellipse className="hermes-shell" cx="64" cy="64" rx="44" ry="43" fill="url(#hermesBodyShell)" />
      <ellipse className="hermes-shell" cx="66" cy="109" rx="29" ry="33" fill="url(#hermesBodyShell)" />
      <path className="hermes-line hermes-arm" d="M28 123c10-7 21-10 33-6" />
      <path className="hermes-line hermes-baton" d="M85 108c12-9 22-22 27-38" />
      <circle className="hermes-accent" cx="90" cy="66" r="4" />
      <ellipse className="eye" cx="49" cy="56" rx="12" ry="15" />
      <ellipse className="eye" cx="78" cy="54" rx="12" ry="15" />
      <circle className="pupil hermes-pupil" cx="53" cy="56" r="5" />
      <circle className="pupil hermes-pupil" cx="82" cy="54" r="5" />
      <circle className="shine" cx="56" cy="52" r="2" />
      <circle className="shine" cx="85" cy="50" r="2" />
      <path className="hermes-line hermes-smile" d="M50 75c13 12 29 12 43-1" />
      <circle className="hermes-accent hermes-core" cx="66" cy="100" r="10" />
      <circle cx="66" cy="100" r="4" fill="#f2fbff" />
      <path d="M48 142c6 4 13 4 20 0" fill="none" stroke="#071020" strokeWidth="6" strokeLinecap="round" />
      <path d="M73 142c6 4 13 4 20 0" fill="none" stroke="#071020" strokeWidth="6" strokeLinecap="round" />
      <path className="hermes-star" d="M23 30l3 7 7 3-7 3-3 7-3-7-7-3 7-3z" />
    </svg>
  );
}

function ClawHead() {
  return (
    <svg className="agent-svg agent-svg--crab" viewBox="0 0 220 220" aria-hidden="true">
      <path className="crab-claw crab-claw--top" d="M42 70c-34-22-40-58-16-78 22-18 52-2 48 25 18-17 48-8 53 18 7 36-38 62-85 35Z" />
      <path className="crab-claw crab-claw--bottom" d="M41 156c-36 18-45 53-23 76 21 21 53 9 52-18 17 20 48 15 56-10 11-35-33-66-85-48Z" />
      <ellipse className="crab-shell" cx="136" cy="112" rx="92" ry="86" />
      <ellipse className="eye" cx="98" cy="84" rx="29" ry="36" />
      <ellipse className="eye" cx="162" cy="80" rx="31" ry="38" />
      <circle className="pupil" cx="107" cy="88" r="13" />
      <circle className="pupil" cx="172" cy="84" r="14" />
      <circle className="shine" cx="113" cy="81" r="5" />
      <circle className="shine" cx="179" cy="76" r="5" />
      <path className="smile" d="M94 142c29 32 72 33 106 2" />
      <path className="smile-shine" d="M116 157c16 9 33 10 52 3" />
    </svg>
  );
}

function ClawBody() {
  return (
    <svg className="agent-svg agent-svg--crab-body" viewBox="0 0 180 220" aria-hidden="true">
      <path className="crab-leg" d="M59 150c-22 18-37 33-45 54" />
      <path className="crab-leg" d="M82 158c-10 24-12 38-8 55" />
      <path className="crab-leg" d="M120 158c10 24 12 38 8 55" />
      <path className="crab-leg" d="M141 150c22 18 37 33 45 54" />
      <path className="crab-claw crab-claw--top" d="M47 82c-36-18-49-54-29-77 19-21 51-9 52 19 15-21 47-18 57 7 13 35-28 67-80 51Z" />
      <path className="crab-claw crab-claw--bottom" d="M45 121c-39 4-63 32-51 60 12 27 46 27 57 1 8 26 40 34 58 14 24-28-8-74-64-75Z" />
      <ellipse className="crab-shell" cx="104" cy="108" rx="76" ry="74" />
      <ellipse className="crab-belly" cx="109" cy="155" rx="48" ry="38" />
      <ellipse className="eye" cx="80" cy="74" rx="22" ry="29" />
      <ellipse className="eye" cx="133" cy="70" rx="24" ry="31" />
      <circle className="pupil" cx="87" cy="78" r="10" />
      <circle className="pupil" cx="141" cy="74" r="11" />
      <circle className="shine" cx="91" cy="72" r="4" />
      <circle className="shine" cx="145" cy="67" r="4" />
      <path className="smile" d="M80 123c25 27 62 28 91 2" />
      <path className="smile-shine" d="M98 136c14 8 29 9 45 2" />
    </svg>
  );
}

function OwlHead() {
  return (
    <svg className="agent-svg" viewBox="0 0 180 140" aria-hidden="true">
      <path className="owl-ear" d="M32 45 4 0l54 23Z" />
      <path className="owl-ear" d="M148 45 176 0l-54 23Z" />
      <ellipse className="owl-face" cx="90" cy="67" rx="82" ry="66" />
      <circle className="owl-eye" cx="62" cy="58" r="26" />
      <circle className="owl-eye" cx="118" cy="58" r="26" />
      <circle className="owl-pupil" cx="62" cy="58" r="9" />
      <circle className="owl-pupil" cx="118" cy="58" r="9" />
      <path className="owl-beak" d="M90 72 75 100h30Z" />
    </svg>
  );
}

function OwlBody() {
  return (
    <svg className="agent-svg agent-svg--body" viewBox="0 0 180 220" aria-hidden="true">
      <path className="owl-ear" d="M35 64 9 18l54 22Z" />
      <path className="owl-ear" d="M145 64 171 18l-54 22Z" />
      <ellipse className="owl-face" cx="90" cy="92" rx="78" ry="70" />
      <ellipse className="owl-body" cx="90" cy="151" rx="57" ry="54" />
      <circle className="owl-eye" cx="63" cy="83" r="24" />
      <circle className="owl-eye" cx="117" cy="83" r="24" />
      <circle className="owl-pupil" cx="63" cy="83" r="8" />
      <circle className="owl-pupil" cx="117" cy="83" r="8" />
      <path className="owl-beak" d="M90 98 76 124h28Z" />
      <path className="owl-wing" d="M47 136c-22 19-28 40-19 61" />
      <path className="owl-wing" d="M133 136c22 19 28 40 19 61" />
    </svg>
  );
}

function FoxHead() {
  return (
    <svg className="agent-svg" viewBox="0 0 180 150" aria-hidden="true">
      <path className="fox-ear" d="M38 48 6 0l62 24Z" />
      <path className="fox-ear" d="M142 48 174 0l-62 24Z" />
      <ellipse className="fox-face" cx="90" cy="78" rx="82" ry="66" />
      <path className="fox-muzzle" d="M52 98c20 34 56 42 76 0 18 52-94 52-76 0Z" />
      <circle className="fox-eye" cx="62" cy="68" r="9" />
      <circle className="fox-eye" cx="118" cy="68" r="9" />
      <path className="fox-smile" d="M72 103c13 13 24 13 37 0" />
    </svg>
  );
}

function FoxBody() {
  return (
    <svg className="agent-svg agent-svg--body" viewBox="0 0 180 220" aria-hidden="true">
      <path className="fox-ear" d="M38 68 8 22l62 25Z" />
      <path className="fox-ear" d="M142 68 172 22l-62 25Z" />
      <ellipse className="fox-face" cx="90" cy="96" rx="78" ry="65" />
      <ellipse className="fox-body" cx="88" cy="158" rx="52" ry="49" />
      <path className="fox-tail" d="M126 154c38 2 55 22 47 56-30-3-50-17-60-42" />
      <path className="fox-muzzle" d="M54 115c19 32 53 39 72 0 17 49-89 49-72 0Z" />
      <circle className="fox-eye" cx="63" cy="87" r="8" />
      <circle className="fox-eye" cx="117" cy="87" r="8" />
      <path className="fox-smile" d="M74 121c12 12 23 12 35 0" />
    </svg>
  );
}

function loadStoredPlacements(fallback: AgentPlacements): AgentPlacements {
  try {
    const storedValue = localStorage.getItem(PLACEMENT_STORAGE_KEY);

    if (!storedValue) {
      return fallback;
    }

    const stored = JSON.parse(storedValue) as Partial<AgentPlacements>;

    return buddies.reduce<AgentPlacements>((placements, buddy) => {
      const placement = stored[buddy.id];

      placements[buddy.id] = isValidPlacement(placement)
        ? placement
        : fallback[buddy.id];

      return placements;
    }, {});
  } catch {
    return fallback;
  }
}

function loadStoredBuddySettings(fallback: BuddySettingsMap): BuddySettingsMap {
  try {
    const storedValue = localStorage.getItem(SETTINGS_STORAGE_KEY);

    if (!storedValue) {
      return fallback;
    }

    const stored = JSON.parse(storedValue) as Partial<BuddySettingsMap>;

    return buddies.reduce<BuddySettingsMap>((settings, buddy) => {
      settings[buddy.id] = normalizeBuddySettings(
        BUDDY_PROFILES[buddy.id],
        stored[buddy.id],
      );
      return settings;
    }, {});
  } catch {
    return fallback;
  }
}

function isValidPlacement(placement: unknown): placement is AgentPlacement {
  if (!placement || typeof placement !== "object") {
    return false;
  }

  const candidate = placement as Partial<AgentPlacement>;

  if (!candidate.edge || !["top", "right", "bottom", "left"].includes(candidate.edge)) {
    return false;
  }

  if (candidate.state === "tucked") {
    return typeof candidate.slot === "number";
  }

  return (
    candidate.state === "free" &&
    typeof candidate.x === "number" &&
    typeof candidate.y === "number"
  );
}

function getFreePlacement(
  edge: Edge,
  pointerX: number,
  pointerY: number,
  stageRect: DOMRect,
): FreeAgentPlacement {
  const x = clamp(
    pointerX - FREE_AGENT_SIZE / 2,
    FREE_AGENT_MARGIN,
    stageRect.width - FREE_AGENT_SIZE - FREE_AGENT_MARGIN,
  );
  const y = clamp(
    pointerY - FREE_AGENT_SIZE / 2,
    FREE_AGENT_MARGIN,
    stageRect.height - FREE_AGENT_SIZE - FREE_AGENT_MARGIN,
  );

  return {
    state: "free",
    edge,
    x,
    y,
  };
}

function centerFreePlacement(edge: Edge, stageRect: DOMRect): FreeAgentPlacement {
  return {
    state: "free",
    edge,
    x: clamp(
      (stageRect.width - FREE_AGENT_SIZE) / 2,
      FREE_AGENT_MARGIN,
      stageRect.width - FREE_AGENT_SIZE - FREE_AGENT_MARGIN,
    ),
    y: clamp(
      (stageRect.height - FREE_AGENT_SIZE) / 2,
      FREE_AGENT_MARGIN,
      stageRect.height - FREE_AGENT_SIZE - FREE_AGENT_MARGIN,
    ),
  };
}

function fallbackHitboxForPlacement(placement: AgentPlacement, viewportWidth: number, viewportHeight: number): Hitbox {
  if (placement.state === "free") {
    return {
      x: Math.round(placement.x),
      y: Math.round(placement.y),
      w: FREE_AGENT_SIZE,
      h: FREE_AGENT_SIZE,
    };
  }

  const size = DOCKED_HEAD_HITBOX_SIZE;
  const half = size / 2;
  const slotX = placement.slot * viewportWidth;
  const slotY = placement.slot * viewportHeight;

  switch (placement.edge) {
    case "left":
      return {
        x: -DOCKED_HEAD_EDGE_OVERLAP,
        y: Math.round(slotY - half),
        w: size,
        h: size,
      };
    case "right":
      return {
        x: Math.round(viewportWidth - size + DOCKED_HEAD_EDGE_OVERLAP),
        y: Math.round(slotY - half),
        w: size,
        h: size,
      };
    case "top":
      return {
        x: Math.round(slotX - half),
        y: -DOCKED_HEAD_EDGE_OVERLAP,
        w: size,
        h: size,
      };
    case "bottom":
      return {
        x: Math.round(slotX - half),
        y: Math.round(viewportHeight - size + DOCKED_HEAD_EDGE_OVERLAP),
        w: size,
        h: size,
      };
  }
}

function getSnapEdge(placement: FreeAgentPlacement, stageRect: DOMRect) {
  const distances: Array<[Edge, number]> = [
    ["left", placement.x],
    ["right", stageRect.width - (placement.x + FREE_AGENT_SIZE)],
    ["top", placement.y],
    ["bottom", stageRect.height - (placement.y + FREE_AGENT_SIZE)],
  ];
  const [edge, distance] = distances.sort((a, b) => a[1] - b[1])[0];

  return distance <= SNAP_DISTANCE ? edge : null;
}

function getPreviewDockSlot(edge: Edge, placement: FreeAgentPlacement, stageRect: DOMRect) {
  const position =
    edge === "left" || edge === "right"
      ? (placement.y + FREE_AGENT_SIZE / 2) / stageRect.height
      : (placement.x + FREE_AGENT_SIZE / 2) / stageRect.width;

  return nearestDockSlot(edge, position);
}

function defaultDockSlot(buddyId: string) {
  return buddies.find((buddy) => buddy.id === buddyId)?.dockSlot ?? 0.5;
}

function getCurrentBuddyIdFromUrl() {
  const buddyId = new URLSearchParams(window.location.search).get("buddy");
  return buddyId || null;
}

async function getCurrentBuddyId() {
  const buddyId = getCurrentBuddyIdFromUrl();

  if (buddyId) {
    return buddyId;
  }

  try {
    const label = getCurrentWebviewWindow().label;

    if (label === "border-dock") {
      return null;
    }

    const labelBuddyId = getBuddyIdFromLabel(label);

    if (labelBuddyId) {
      return labelBuddyId;
    }
  } catch {
    // Browser previews do not expose a Tauri webview label.
  }

  try {
    const windowLabel = getCurrentWindow().label;

    if (windowLabel === "border-dock") {
      return null;
    }

    const labelBuddyId = getBuddyIdFromLabel(windowLabel);

    if (labelBuddyId) {
      return labelBuddyId;
    }
  } catch {
    // Browser previews do not expose a Tauri window label.
  }

  try {
    return await invoke<string | null>("current_buddy_id");
  } catch {
    return null;
  }
}

function getBuddyIdFromLabel(label: string) {
  return label.startsWith("buddy-") ? label.slice("buddy-".length) : null;
}

function nearestDockSlot(edge: Edge, position: number) {
  return DOCK_SLOTS_BY_EDGE[edge].reduce((nearest, slot) =>
    Math.abs(slot - position) < Math.abs(nearest - position) ? slot : nearest,
  );
}

function clamp(value: number, min: number, max: number) {
  return Math.min(Math.max(value, min), max);
}
