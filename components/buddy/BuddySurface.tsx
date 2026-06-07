import {
  type CSSProperties,
  forwardRef,
  useEffect,
  useImperativeHandle,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import {
  BUDDY_PROFILES,
  type BuddySettings,
} from "../../src/buddyProfiles";
import type { GatewayConnectionState } from "../../src/gatewayProtocol";
import type { GatewaySettings } from "../../src/gatewaySettings";
import { connectionLabelForState } from "../../src/useBuddyGateway";
import { BuddyActionMenu } from "./BuddyActionMenu";
import { BuddyPanel, type BuddyChatLine } from "./BuddyPanel";
import { BuddySettingsDialog } from "./BuddySettingsDialog";
import { BuddyUiBubble } from "./BuddyUiBubble";
import { useUiBubble } from "./useUiBubble";
import { TrustWorkbenchPanel } from "../trust-workbench/TrustWorkbenchPanel";
import "./buddy-surface.css";

export type BuddyDisplayMode = "compact" | "full";

type Edge = "top" | "right" | "bottom" | "left";

type Hitbox = {
  x: number;
  y: number;
  w: number;
  h: number;
};

export type BuddySurfaceHandle = {
  measureHitboxes: () => Hitbox[];
};

type DockBuddy = {
  id: string;
  shortName: string;
  message?: string;
};

type BuddySurfaceProps = {
  buddy: DockBuddy;
  edge: Edge;
  dockSlot: number;
  /**
   * Interaction model (robust + scalable for future custom buddies):
   *  - `interactive: false` → buddy is docked/tucked on the border. We show ONLY
   *    the ambient speech bubble. No text input, no settings — so the native
   *    border overlay's clickable region stays tiny and stable (this is what
   *    fixes the "nothing clickable / crashing" border-mode failure).
   *  - `interactive: true`  → buddy has been undocked / dragged out into a free
   *    floating surface. Now we mount the full chat composer + settings, where a
   *    large stable hitbox makes interaction reliable.
   */
  interactive: boolean;
  hasGateway: boolean;
  gatewayState: GatewayConnectionState;
  gatewayDetail: string | null;
  gatewayBusy: boolean;
  gatewayUrl: string;
  gatewayAutoConnect: boolean;
  message: string;
  settings: BuddySettings;
  onGatewayConnect: () => void;
  onGatewayDisconnect: () => void;
  onGatewaySettingsChange: (settings: GatewaySettings) => void;
  onLayoutChange?: () => void;
  /** Ask the host to undock this buddy into the interactive free surface. */
  onRequestInteract?: () => void;
  /** Ask the host to re-dock (tuck) this buddy back onto the border. */
  onRequestDock?: () => void;
  onSendChat: (text: string) => boolean;
  onSettingsChange: (settings: BuddySettings) => void;
};

function rectToHitbox(element: HTMLElement | null, padding = 8): Hitbox | null {
  if (!element) {
    return null;
  }

  const rect = element.getBoundingClientRect();
  if (rect.width <= 0 || rect.height <= 0) {
    return null;
  }

  return {
    x: Math.max(0, Math.round(rect.left) - padding),
    y: Math.max(0, Math.round(rect.top) - padding),
    w: Math.min(window.innerWidth, Math.round(rect.width) + padding * 2),
    h: Math.min(window.innerHeight, Math.round(rect.height) + padding * 2),
  };
}

function createLineId() {
  return `line-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
}

/**
 * Bounding box that encloses every provided hitbox. Used so the head, the gap,
 * and the panel become one continuous clickable region in the interactive
 * (undocked) surface — no sub-pixel gap can become an unclickable dead zone.
 */
function unionHitboxes(boxes: Hitbox[]): Hitbox | null {
  if (boxes.length === 0) {
    return null;
  }

  let minX = Infinity;
  let minY = Infinity;
  let maxX = -Infinity;
  let maxY = -Infinity;

  for (const box of boxes) {
    minX = Math.min(minX, box.x);
    minY = Math.min(minY, box.y);
    maxX = Math.max(maxX, box.x + box.w);
    maxY = Math.max(maxY, box.y + box.h);
  }

  return { x: minX, y: minY, w: maxX - minX, h: maxY - minY };
}

export const BuddySurface = forwardRef<BuddySurfaceHandle, BuddySurfaceProps>(function BuddySurface(
  {
    buddy,
    edge,
    dockSlot,
    interactive,
    hasGateway,
    gatewayState,
    gatewayDetail,
    gatewayBusy,
    gatewayUrl,
    gatewayAutoConnect,
    message,
    settings,
    onGatewayConnect,
    onGatewayDisconnect,
    onGatewaySettingsChange,
    onLayoutChange,
    onRequestInteract,
    onRequestDock,
    onSendChat,
    onSettingsChange,
  },
  ref,
) {
  const clusterRef = useRef<HTMLDivElement>(null);
  const panelRef = useRef<HTMLDivElement>(null);
  const settingsDialogRef = useRef<HTMLDivElement>(null);
  const actionMenuRef = useRef<HTMLDivElement>(null);
  const [draft, setDraft] = useState("");
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [menuOpen, setMenuOpen] = useState(false);
  const [menuAnchor, setMenuAnchor] = useState<HTMLElement | null>(null);
  const [history, setHistory] = useState<BuddyChatLine[]>([]);
  const [panelShift, setPanelShift] = useState({ x: 0, y: 0 });
  const lastMessageRef = useRef("");
  // A message typed while the gateway is still offline is parked here so it can
  // auto-send the moment the connection comes online (seamless first reply).
  const pendingSendRef = useRef<string | null>(null);
  const autoConnectAttemptedRef = useRef(false);


  // The display mode is now derived purely from the dock state. Docked = the
  // compact ambient bubble; undocked = the full interactive panel.
  const displayMode: BuddyDisplayMode = interactive ? "full" : "compact";

  const profile = BUDDY_PROFILES[buddy.id];
  const gatewayOnline = hasGateway && gatewayState === "connected";
  const bubbleSide = edge === "right" ? "left" : "right";
  const bubbleVertical = edge === "top" || dockSlot < 0.22 ? "below" : "above";
  const trustWorkbenchMode = buddy.id === "fox" ? "nexus" : buddy.id === "owl" ? "veritas" : null;

  // Close any transient overlays automatically when the buddy re-docks so they
  // can never leave a stale hitbox behind on the border.
  useEffect(() => {
    if (!interactive) {
      setSettingsOpen(false);
      setMenuOpen(false);
    }
  }, [interactive]);

  const statusBubbleText = useMemo(() => {
    if (gatewayBusy) {
      return "Hermes is thinking…";
    }

    if (hasGateway) {
      if (gatewayOnline) {
        return message.trim() || gatewayDetail || "Hermes is live on the border. Drag me out to chat.";
      }

      if (gatewayState === "connecting") {
        return "Connecting to Hermes gateway…";
      }

      if (gatewayState === "error" || gatewayState === "disconnected") {
        return gatewayDetail || "Drag me out to open settings and connect.";
      }

      return "Drag me off the border to chat and connect.";
    }

    return message.trim() || `${buddy.shortName} is ready — drag me out to chat.`;
  }, [
    buddy.shortName,
    gatewayBusy,
    gatewayDetail,
    gatewayOnline,
    gatewayState,
    hasGateway,
    message,
  ]);

  // The bubble is only visible (and animated) while docked/ambient.
  const uiBubble = useUiBubble(statusBubbleText, !interactive, true);

  useEffect(() => {
    const trimmed = message.trim();
    if (!trimmed || trimmed === lastMessageRef.current) {
      return;
    }

    lastMessageRef.current = trimmed;
    setHistory((current) => [
      ...current,
      { id: createLineId(), role: "assistant", text: trimmed },
    ]);
  }, [message]);

  useEffect(() => {
    if (gatewayDetail && gatewayState === "connected") {
      setHistory((current) => {
        if (current.some((line) => line.role === "status" && line.text === gatewayDetail)) {
          return current;
        }
        return [...current, { id: createLineId(), role: "status", text: gatewayDetail }];
      });
    }
  }, [gatewayDetail, gatewayState]);

  // Seamless connection: as soon as the buddy is undocked into the interactive
  // surface, bring its gateway online (once) so the user can chat without first
  // digging through settings. Re-armed each time it re-docks.
  useEffect(() => {
    if (!interactive) {
      autoConnectAttemptedRef.current = false;
      return;
    }

    if (hasGateway && !gatewayOnline && !autoConnectAttemptedRef.current && gatewayState !== "connecting") {
      autoConnectAttemptedRef.current = true;
      onGatewayConnect();
    }
  }, [interactive, hasGateway, gatewayOnline, gatewayState, onGatewayConnect]);

  // Flush a message that was typed while offline the instant we connect.
  useEffect(() => {
    if (!gatewayOnline) {
      return;
    }

    const pending = pendingSendRef.current;
    if (!pending) {
      return;
    }

    pendingSendRef.current = null;
    onSendChat(pending);
  }, [gatewayOnline, onSendChat]);


  useImperativeHandle(
    ref,
    () => ({
      measureHitboxes: () => {
        const boxes: Hitbox[] = [];
        const clusterBox = rectToHitbox(clusterRef.current, 4);
        const bubbleNode = clusterRef.current?.querySelector(".buddy-ui-bubble") as HTMLElement | null;
        const bubbleBox = rectToHitbox(bubbleNode, 4);

        if (clusterBox) {
          boxes.push(clusterBox);
        }
        if (bubbleBox) {
          boxes.push(bubbleBox);
        }

        // Ambient (docked) mode: only the cluster + speech bubble are clickable.
        // No composer/settings exist here, so the border overlay stays minimal.
        if (!interactive) {
          return boxes;
        }

        const panelBox = rectToHitbox(panelRef.current);
        const dialogBox = settingsOpen ? rectToHitbox(settingsDialogRef.current) : null;
        const menuBox = menuOpen ? rectToHitbox(actionMenuRef.current, 4) : null;

        if (panelBox) {
          boxes.push(panelBox);
        }
        if (dialogBox) {
          boxes.push(dialogBox);
        }
        if (menuBox) {
          boxes.push(menuBox);
        }

        // Union the figure + panel into one continuous region so the gap
        // between them can never create a dead zone over the composer.
        const unionBox = unionHitboxes(
          [clusterBox, bubbleBox, panelBox].filter(
            (box): box is Hitbox => box !== null,
          ),
        );
        if (unionBox) {
          boxes.push(unionBox);
        }

        return boxes;
      },
    }),
    [interactive, menuOpen, settingsOpen],
  );

  useLayoutEffect(() => {
    if (!settingsOpen && !menuOpen) {
      return;
    }

    const frame = window.requestAnimationFrame(() => {
      void clusterRef.current?.getBoundingClientRect();
    });
    return () => window.cancelAnimationFrame(frame);
  }, [menuOpen, settingsOpen]);

  useLayoutEffect(() => {
    if (!interactive || !panelRef.current) {
      setPanelShift({ x: 0, y: 0 });
      return;
    }

    let innerFrame = 0;
    const outerFrame = window.requestAnimationFrame(() => {
      innerFrame = window.requestAnimationFrame(() => {
        const rect = panelRef.current?.getBoundingClientRect();
        if (!rect) {
          return;
        }

        const margin = 12;
        let x = 0;
        let y = 0;

        if (rect.left < margin) {
          x = margin - rect.left;
        } else if (rect.right > window.innerWidth - margin) {
          x = window.innerWidth - margin - rect.right;
        }

        if (rect.top < margin) {
          y = margin - rect.top;
        } else if (rect.bottom > window.innerHeight - margin) {
          y = window.innerHeight - margin - rect.bottom;
        }

        setPanelShift((current) =>
          current.x === x && current.y === y ? current : { x, y },
        );
      });
    });

    return () => {
      window.cancelAnimationFrame(outerFrame);
      if (innerFrame) {
        window.cancelAnimationFrame(innerFrame);
      }
    };
  }, [displayMode, history.length, interactive, menuOpen, settingsOpen]);

  useLayoutEffect(() => {
    if (!onLayoutChange) {
      return;
    }

    let innerFrame = 0;
    const outerFrame = window.requestAnimationFrame(() => {
      innerFrame = window.requestAnimationFrame(() => {
        onLayoutChange();
      });
    });

    return () => {
      window.cancelAnimationFrame(outerFrame);
      if (innerFrame) {
        window.cancelAnimationFrame(innerFrame);
      }
    };
  }, [
    draft,
    gatewayBusy,
    gatewayDetail,
    gatewayState,
    history.length,
    interactive,
    menuOpen,
    onLayoutChange,
    settingsOpen,
    uiBubble.displayText,
    uiBubble.mounted,
    uiBubble.phase,
  ]);

  function forceLayoutRefresh() {
    if (onLayoutChange) {
      // Two rAFs to let DOM settle before native hitbox measurement.
      requestAnimationFrame(() => requestAnimationFrame(() => onLayoutChange()));
    }
  }

  function handleSubmit() {
    const trimmed = draft.trim();
    if (!trimmed) {
      return;
    }

    // Always show the user's message immediately so the chat feels responsive.
    setHistory((current) => [
      ...current,
      { id: createLineId(), role: "user", text: trimmed },
    ]);
    setDraft("");

    // Offline: park the message and bring the gateway up. The auto-flush effect
    // below sends it the moment the connection is live — no second click needed.
    if (hasGateway && !gatewayOnline) {
      pendingSendRef.current = trimmed;
      setHistory((current) => [
        ...current,
        { id: createLineId(), role: "status", text: "Connecting to gateway…" },
      ]);
      onGatewayConnect();
      return;
    }

    const sent = onSendChat(trimmed);
    if (!sent) {
      // Couldn't deliver right now — keep it queued and (re)connect.
      pendingSendRef.current = trimmed;
      if (hasGateway) {
        onGatewayConnect();
      }
    }
  }


  const menuActions = [
    {
      id: "settings",
      label: "Model & gateway settings",
      icon: "⚙",
    },
    ...(hasGateway
      ? [
          {
            id: gatewayOnline ? "disconnect" : "connect",
            label: gatewayOnline ? "Disconnect gateway" : "Connect gateway",
            icon: "◎",
            disabled: gatewayState === "connecting",
          },
        ]
      : []),
    {
      id: "dock",
      label: "Dock to border",
      icon: "⤓",
    },
  ];

  return (
    <div
      ref={clusterRef}
      className={[
        "buddy-cluster",
        `buddy-cluster--${edge}`,
        interactive ? "buddy-cluster--full" : "buddy-cluster--compact",
      ].join(" ")}
    >
      {!interactive ? (
        <BuddyUiBubble
          text={uiBubble.displayText}
          phase={uiBubble.phase}
          mounted={uiBubble.mounted}
          clickable={uiBubble.isClickable}
          bubbleSide={bubbleSide}
          bubbleVertical={bubbleVertical}
          title="Open chat — undock buddy"
          onActivate={onRequestInteract}
        />
      ) : (
        <>
          <div
            ref={panelRef}
            className="buddy-cluster__panel-wrap"
            style={{
              "--buddy-panel-shift-x": `${panelShift.x}px`,
              "--buddy-panel-shift-y": `${panelShift.y}px`,
            } as CSSProperties}
          >
            <BuddyPanel
              buddyName={buddy.shortName}
              modelLabel={settings.modelLabel}
              displayMode={displayMode}
              draft={draft}
              busy={gatewayBusy}
              hasGateway={hasGateway}
              gatewayState={gatewayState}
              history={history}
              panelContent={
                trustWorkbenchMode ? (
                  <TrustWorkbenchPanel
                    mode={trustWorkbenchMode}
                    title={trustWorkbenchMode === "nexus" ? "Nexus context grades" : "Veritas receipt checks"}
                    compact
                  />
                ) : null
              }
              onDraftChange={setDraft}
              onSubmit={handleSubmit}
              onPrimaryAction={() => undefined}
              onComposerInteract={forceLayoutRefresh}
              onOpenMenu={(anchor) => {
                setMenuAnchor(anchor);
                setMenuOpen(true);
              }}
              onOpenSettings={() => {
                setSettingsOpen(true);
                forceLayoutRefresh();
              }}
              onCollapse={() => onRequestDock?.()}
            />
          </div>

          <BuddyActionMenu
            open={menuOpen}
            anchor={menuAnchor}
            actions={menuActions}
            boundaryRef={panelRef}
            menuRef={actionMenuRef}
            onClose={() => setMenuOpen(false)}
            onAction={(actionId) => {
              if (actionId === "settings") {
                setSettingsOpen(true);
                forceLayoutRefresh();
                return;
              }

              if (actionId === "connect") {
                onGatewayConnect();
                return;
              }

              if (actionId === "disconnect") {
                onGatewayDisconnect();
                return;
              }

              if (actionId === "dock") {
                onRequestDock?.();
              }
            }}
          />

          <BuddySettingsDialog
            open={settingsOpen}
            buddyName={buddy.shortName}
            ownerLabel={profile?.identity.ownerLabel ?? buddy.shortName}
            settings={settings}
            hasGateway={hasGateway}
            gatewayState={gatewayState}
            gatewayDetail={gatewayDetail}
            gatewayUrl={gatewayUrl}
            gatewayAutoConnect={gatewayAutoConnect}
            dialogRef={settingsDialogRef}
            onClose={() => {
              setSettingsOpen(false);
              forceLayoutRefresh();
            }}
            onSave={(nextSettings, nextGatewaySettings) => {
              onSettingsChange(nextSettings);
              onGatewaySettingsChange(nextGatewaySettings);
            }}
            onGatewayConnect={onGatewayConnect}
            onGatewayDisconnect={onGatewayDisconnect}
          />
        </>
      )}
    </div>
  );
});

export function buddySurfaceStatusLabel(
  hasGateway: boolean,
  gatewayState: GatewayConnectionState,
) {
  if (!hasGateway) {
    return "";
  }

  return connectionLabelForState(gatewayState);
}
