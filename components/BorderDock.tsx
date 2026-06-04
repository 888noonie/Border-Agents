import {
  type CSSProperties,
  type PointerEvent as ReactPointerEvent,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import clawManifest from "../characters/crab/manifest.json";
import owlManifest from "../characters/owl/manifest.json";
import "./BorderDock.css";

type Edge = "top" | "right" | "bottom" | "left";

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
  color: string;
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

type AgentPlacement =
  | {
      state: "tucked";
      edge: Edge;
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
const PLACEMENT_STORAGE_KEY = "border-buddies:placements:v2";

const buddies: DockBuddy[] = [
  {
    id: clawManifest.id,
    name: clawManifest.name,
    shortName: "Claw",
    ownerKind: "model",
    ownerLabel: "Codex",
    role: clawManifest.role,
    personality: clawManifest.personality,
    speechStyle: "Short, celebratory, a little cheeky",
    edge: clawManifest.border_position as Edge,
    color: clawManifest.color,
    message: "Memory graded! Trusted pieces ready?",
    visible: "primary",
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
    color: "#ef6a3a",
    message: "5 memories found. 1 trusted.",
    visible: "faint",
  },
];

const defaultPlacements = buddies.reduce<AgentPlacements>((placements, buddy) => {
  placements[buddy.id] = {
    state: "tucked",
    edge: buddy.edge,
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
  const [layout, setLayout] = useState<DockLayout>(INITIAL_LAYOUT);
  const [activeAgentId, setActiveAgentId] = useState("crab");
  const [draggingAgentId, setDraggingAgentId] = useState<string | null>(null);
  const [placements, setPlacements] = useState<AgentPlacements>(() =>
    loadStoredPlacements(defaultPlacements),
  );

  const multiMonitor = useMemo(
    () => new URLSearchParams(window.location.search).get("multiMonitor") === "true",
    [],
  );

  useEffect(() => {
    let mounted = true;

    async function configureWindow() {
      await setDockInputEnabled(true);

      try {
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
  }, [multiMonitor]);

  useEffect(() => {
    localStorage.setItem(PLACEMENT_STORAGE_KEY, JSON.stringify(placements));
  }, [placements]);

  function startBuddyDrag(buddy: DockBuddy, event: ReactPointerEvent<HTMLElement>) {
    const stageRect = stageRef.current?.getBoundingClientRect();

    if (!stageRect) {
      return;
    }

    event.preventDefault();
    event.currentTarget.setPointerCapture(event.pointerId);

    const pointerX = event.clientX - stageRect.left;
    const pointerY = event.clientY - stageRect.top;
    const currentPlacement = placements[buddy.id] ?? defaultPlacements[buddy.id];
    const freePlacement =
      currentPlacement.state === "free"
        ? currentPlacement
        : getFreePlacement(buddy.edge, pointerX, pointerY, stageRect);

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
    const activeDrag = dragRef.current;
    const stageRect = stageRef.current?.getBoundingClientRect();

    if (!activeDrag || !stageRect) {
      return;
    }

    const pointerX = event.clientX - stageRect.left;
    const pointerY = event.clientY - stageRect.top;

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
        },
      };
    });
  }

  function tuckBuddy(buddyId: string) {
    setPlacements((current) => {
      const currentPlacement = current[buddyId] ?? defaultPlacements[buddyId];

      return {
        ...current,
        [buddyId]: {
          state: "tucked",
          edge: currentPlacement.edge,
        },
      };
    });
  }

  return (
    <main className="border-dock" aria-label="Border Buddies dock">
      <div
        className="dock-stage"
        data-monitor-count={layout.activeMonitorIds.length}
        ref={stageRef}
      >
        {buddies.map((buddy) => (
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
  } as CSSProperties;

  if (placement.state === "free") {
    Object.assign(style, {
      "--agent-x": `${placement.x}px`,
      "--agent-y": `${placement.y}px`,
    });
  }

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
      style={style}
      aria-label={`${buddy.shortName}: ${buddy.ownerLabel} ${buddy.role}`}
      onPointerEnter={() => setDockInputEnabled(true)}
      onPointerLeave={() => setDockInputEnabled(true)}
      onPointerMove={onDragMove}
      onPointerUp={onDragEnd}
      onPointerCancel={onDragEnd}
      onDoubleClick={onManualTuck}
    >
      {active && buddy.message && placement.state === "tucked" ? (
        <SpeechBubble buddy={buddy} edge={edge} />
      ) : null}
      <button
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

function SpeechBubble({ buddy, edge }: { buddy: DockBuddy; edge: Edge }) {
  return (
    <div className={`speech-bubble speech-bubble--${edge}`} role="status">
      <strong>{buddy.shortName}</strong>
      <span className="speech-bubble__owner">{buddy.ownerLabel}</span>
      <span>{buddy.message}</span>
      {buddy.id === "crab" ? <span aria-hidden="true">🦀</span> : null}
    </div>
  );
}

function BuddyFigure({ buddyId, state }: { buddyId: string; state: AgentPlacement["state"] }) {
  if (buddyId === "crab") {
    return state === "free" ? <ClawBody /> : <ClawHead />;
  }

  if (buddyId === "owl") {
    return state === "free" ? <OwlBody /> : <OwlHead />;
  }

  return state === "free" ? <FoxBody /> : <FoxHead />;
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
    return true;
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

function clamp(value: number, min: number, max: number) {
  return Math.min(Math.max(value, min), max);
}
