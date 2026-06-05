import {
  type CSSProperties,
  type PointerEvent as ReactPointerEvent,
  useEffect,
  forwardRef,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  useCallback,
} from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { register, unregister } from "@tauri-apps/plugin-global-shortcut";
import clawManifest from "../characters/crab/manifest.json";
import hermesManifest from "../characters/hermes/manifest.json";
import owlManifest from "../characters/owl/manifest.json";
import "./BorderDock.css";

export interface Hitbox {
  x: number;
  y: number;
  w: number;
  h: number;
}

export function useBuddyHitbox() {
  const pending = useRef<Hitbox[]>([]);
  const raf = useRef<number | null>(null);

  const flush = useCallback(() => {
    if (raf.current) cancelAnimationFrame(raf.current);
    raf.current = requestAnimationFrame(() => {
      invoke("set_input_hitboxes", { boxes: pending.current }).catch(() => {});
    });
  }, []);

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

  return { setHitboxes };
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
const PLACEMENT_STORAGE_KEY = "border-buddies:placements:v3";
const PASS_THROUGH_SHORTCUT = "CommandOrControl+Alt+B";
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
    message: "Signal caught. Want the sharp version?",
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
  const clickThroughRef = useRef(false);
  const idleTimerRef = useRef<number | null>(null);
  const nativeMoveSnapTimerRef = useRef<number | null>(null);
  const [layout, setLayout] = useState<DockLayout>(INITIAL_LAYOUT);
  const [activeAgentId, setActiveAgentId] = useState("hermes");
  const [draggingAgentId, setDraggingAgentId] = useState<string | null>(null);
  const [clickThrough, setClickThrough] = useState(false);
  const [idle, setIdle] = useState(false);
  const [placements, setPlacements] = useState<AgentPlacements>(() =>
    loadStoredPlacements(defaultPlacements),
  );
  const placementsRef = useRef<AgentPlacements>(placements);

  const windowBuddyId = useMemo(() => new URLSearchParams(window.location.search).get("buddy"), []);
  const windowBuddy = useMemo(
    () => buddies.find((buddy) => buddy.id === windowBuddyId) ?? null,
    [windowBuddyId],
  );
  const perBuddyWindow = Boolean(windowBuddy);
  const visibleBuddies = windowBuddy ? [windowBuddy] : buddies;
  const multiMonitor = useMemo(
    () => new URLSearchParams(window.location.search).get("multiMonitor") === "true",
    [],
  );
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
    let mounted = true;

    async function configureWindow() {
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
    windowBuddy,
    windowBuddyPlacement?.edge,
    windowBuddyPlacement?.state,
    windowBuddyBubbleVisible,
  ]);

  useEffect(() => {
    localStorage.setItem(PLACEMENT_STORAGE_KEY, JSON.stringify(placements));
  }, [placements]);

  useEffect(() => {
    async function registerPassThroughShortcut() {
      try {
        await register(PASS_THROUGH_SHORTCUT, (event) => {
          if (event.state === "Pressed") {
            setClickThroughMode(!clickThroughRef.current);
          }
        });
      } catch {
        // The visible Pass button still works when the global shortcut is unavailable.
      }
    }

    registerPassThroughShortcut();

    return () => {
      unregister(PASS_THROUGH_SHORTCUT).catch(() => {});
      setDockInputEnabled(true);
    };
  }, []);

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

  function setClickThroughMode(enabled: boolean) {
    clickThroughRef.current = enabled;
    setClickThrough(enabled);
    setBuddyWindowInteractive(!enabled);
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

    event.preventDefault();
    event.stopPropagation();

    try {
      event.currentTarget.setPointerCapture(event.pointerId);
    } catch {
      // Window-level listeners keep dragging reliable on desktop webviews.
    }

    const pointerX = event.clientX - stageRect.left;
    const pointerY = event.clientY - stageRect.top;
    const currentPlacement = placements[buddy.id] ?? defaultPlacements[buddy.id];
    const freePlacement =
      currentPlacement.state === "free"
        ? currentPlacement
        : getFreePlacement(buddy.edge, pointerX, pointerY, stageRect);

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

  async function dragOverlayWindow() {
    try {
      await getCurrentWindow().startDragging();
    } catch {
      // Browser previews do not have a native window to drag.
    }
  }

  async function resizeOverlayWindow(direction: ResizeDirection) {
    try {
      await getCurrentWindow().startResizeDragging(direction);
    } catch {
      // Browser previews do not have a native window to resize.
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

  return (
    <main
      className={[
        "border-dock",
        perBuddyWindow ? "border-dock--per-buddy" : "",
        idle ? "border-dock--idle" : "",
        clickThrough ? "border-dock--click-through" : "",
      ].join(" ")}
      data-window-buddy={windowBuddy?.id ?? "preview"}
      aria-label="Border Buddies dock"
      onPointerEnter={markActivity}
      onPointerMove={markActivity}
    >
      <div
        className="dock-stage"
        data-monitor-count={layout.activeMonitorIds.length}
        ref={stageRef}
      >
        {!perBuddyWindow ? (
          <DockControls
            clickThrough={clickThrough}
            onDragWindow={dragOverlayWindow}
            onResizeWindow={() => resizeOverlayWindow("SouthEast")}
            onToggleClickThrough={() => setClickThroughMode(!clickThroughRef.current)}
          />
        ) : null}
        {visibleBuddies.map((buddy) => (
          <BuddyHotspot
            buddy={buddy}
            active={buddy.id === activeAgentId}
            dragging={buddy.id === draggingAgentId}
            key={buddy.id}
            onDragEnd={finishBuddyDrag}
            onDragMove={moveBuddy}
            onDragStart={(event) => startBuddyDrag(buddy, event)}
            onActivate={() => setActiveAgentId(buddy.id)}
            onManualTuck={() => tuckBuddy(buddy.id)}
            placement={placements[buddy.id] ?? defaultPlacements[buddy.id]}
          />
        ))}
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

function DockControls({
  clickThrough,
  onDragWindow,
  onResizeWindow,
  onToggleClickThrough,
}: {
  clickThrough: boolean;
  onDragWindow: () => void;
  onResizeWindow: () => void;
  onToggleClickThrough: () => void;
}) {
  return (
    <div className="dock-controls" aria-label="Border Buddies window controls">
      <span className="dock-controls__hint">
        {clickThrough ? "Click-through on" : "Ctrl+Alt+B passes clicks through"}
      </span>
      <button
        className="dock-control dock-control--move"
        type="button"
        onPointerDown={(event) => {
          event.preventDefault();
          onDragWindow();
        }}
        title="Move overlay window"
      >
        Move
      </button>
      <button
        className="dock-control dock-control--resize"
        type="button"
        onPointerDown={(event) => {
          event.preventDefault();
          onResizeWindow();
        }}
        title="Resize overlay window"
      >
        Resize
      </button>
      <button
        className="dock-control dock-control--pass"
        type="button"
        onClick={onToggleClickThrough}
        title="Toggle click-through mode"
      >
        {clickThrough ? "Catch" : "Pass"}
      </button>
    </div>
  );
}

function BuddyHotspot({
  active,
  buddy,
  dragging,
  onDragEnd,
  onDragMove,
  onDragStart,
  onActivate,
  placement,
  onManualTuck,
}: {
  active: boolean;
  buddy: DockBuddy;
  dragging: boolean;
  onDragEnd: () => void;
  onDragMove: (event: ReactPointerEvent<HTMLElement>) => void;
  onDragStart: (event: ReactPointerEvent<HTMLElement>) => void;
  onActivate: () => void;
  placement: AgentPlacement;
  onManualTuck: () => void;
}) {
  const edge = placement.edge;
  const style = {
    "--agent-color": buddy.color,
    "--agent-accent": buddy.accentColor ?? buddy.color,
  } as CSSProperties;

  if (placement.state === "free") {
    Object.assign(style, {
      "--agent-x": `${placement.x}px`,
      "--agent-y": `${placement.y}px`,
    });
  }

  const headRef = useRef<HTMLButtonElement>(null);
  const bubbleRef = useRef<HTMLDivElement>(null);
  const { setHitboxes } = useBuddyHitbox();

  const isBubbleVisible = active && buddy.message && placement.state === "tucked";
  const freeX = placement.state === "free" ? placement.x : null;
  const freeY = placement.state === "free" ? placement.y : null;

  useLayoutEffect(() => {
    const boxes: Hitbox[] = [];

    if (headRef.current) {
      const r = headRef.current.getBoundingClientRect();
      boxes.push({
        x: Math.round(r.left),
        y: Math.round(r.top),
        w: Math.round(r.width),
        h: Math.round(r.height),
      });
    }

    if (isBubbleVisible && bubbleRef.current) {
      const r = bubbleRef.current.getBoundingClientRect();
      boxes.push({
        x: Math.round(r.left),
        y: Math.round(r.top),
        w: Math.round(r.width),
        h: Math.round(r.height),
      });
    }

    setHitboxes(boxes);
  }, [placement.state, freeX, freeY, isBubbleVisible, setHitboxes]);

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
      onPointerEnter={() => setDockInputEnabled(true)}
      onPointerLeave={() => setDockInputEnabled(true)}
      onPointerMove={onDragMove}
      onPointerUp={onDragEnd}
      onPointerCancel={onDragEnd}
      onDoubleClick={onManualTuck}
    >
      {isBubbleVisible ? (
        <SpeechBubble buddy={buddy} edge={edge} ref={bubbleRef} />
      ) : null}
      <button
        ref={headRef}
        className="agent-button"
        type="button"
        onClick={onActivate}
        onPointerDown={onDragStart}
      >
        <BuddyFigure buddyId={buddy.id} state={placement.state} />
      </button>
    </section>
  );
}

const SpeechBubble = forwardRef<HTMLDivElement, { buddy: DockBuddy; edge: Edge }>(
  ({ buddy, edge }, ref) => {
    return (
      <div ref={ref} className={`speech-bubble speech-bubble--${edge}`} role="status">
        <strong>{buddy.shortName}</strong>
        <span className="speech-bubble__owner">{buddy.ownerLabel}</span>
        <span>{buddy.message}</span>
        {buddy.id === "crab" ? <span aria-hidden="true">🦀</span> : null}
        {buddy.id === "hermes" ? <span aria-hidden="true">✦</span> : null}
      </div>
    );
  }
);

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

function nearestDockSlot(edge: Edge, position: number) {
  return DOCK_SLOTS_BY_EDGE[edge].reduce((nearest, slot) =>
    Math.abs(slot - position) < Math.abs(nearest - position) ? slot : nearest,
  );
}

function clamp(value: number, min: number, max: number) {
  return Math.min(Math.max(value, min), max);
}
