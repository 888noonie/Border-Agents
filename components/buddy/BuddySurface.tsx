import {
  type CSSProperties,
  forwardRef,
  useEffect,
  useImperativeHandle,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import {
  BUDDY_PROFILES,
  type BuddySettings,
} from "../../src/buddyProfiles";
import type { GatewayConnectionState, GatewayMedia } from "../../src/gatewayProtocol";
import type { GatewaySettings } from "../../src/gatewaySettings";
import { buildBuddyGovernanceSnapshot, type BuddyGovernanceSnapshot } from "../../src/liveGovernance";
import { buildOnboardingPanelModel, type OnboardingPanelSection, type HermesProviderPresetId, HERMES_PROVIDER_PRESETS } from "../../src/onboardingPanelModel";
import {
  loadStoredOnboardingSurfaceState,
  saveStoredOnboardingSurfaceState,
  type OnboardingSurfaceDraft,
} from "../../src/onboardingSurfaceState";
import { advanceOnboarding, currentAct, type OnboardingAct, type OnboardingEvent } from "../../src/wizardOnboarding";
import { lifecycleReceiptKinds, recordLifecycleReceipt } from "../../src/lifecycleReceipts";
import { type UserPosture } from "../../src/core/userPosture";
import { type ActionReceipt } from "../../src/core";
import { handleActionRequest } from "../../src/soulActions";
import { connectionLabelForState } from "../../src/useBuddyGateway";
import { ActionReceiptCard } from "./ActionReceiptCard";
import { BuddyActionMenu } from "./BuddyActionMenu";
import { BuddyPanel, type BuddyChatLine } from "./BuddyPanel";
import { BuddySettingsDialog } from "./BuddySettingsDialog";
import { BuddyUiBubble } from "./BuddyUiBubble";
import { OnboardingWizardPanel } from "./OnboardingWizardPanel";
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

type ChatPayload = {
  text: string;
  purpose?: string;
  context?: string;
};

type DockBuddy = {
  id: string;
  shortName: string;
  message?: string;
};

type BubbleTabId = "message" | "setup" | "settings" | "gateway" | "dock";

type BuddySurfaceUiState = {
  activeTab: BubbleTabId;
  expandedSections: Partial<Record<BubbleTabId, boolean>>;
  alwaysCenterFit: boolean;
  preventSettingsOverflow: boolean;
};

const BUDDY_SURFACE_UI_STORAGE_KEY = "border-agents:buddy-surface-ui:v1";

const DEFAULT_BUDDY_SURFACE_UI: BuddySurfaceUiState = {
  activeTab: "message",
  expandedSections: {},
  alwaysCenterFit: false,
  preventSettingsOverflow: true,
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
  preferCenterFit?: boolean;
  hasGateway: boolean;
  gatewayState: GatewayConnectionState;
  gatewayDetail: string | null;
  gatewayBusy: boolean;
  gatewayUrl: string;
  gatewayAutoConnect: boolean;
  message: string;
  /** Rich media (image/file) attached to the latest reply, rendered in the panel. */
  messageMedia?: GatewayMedia | null;
  settings: BuddySettings;
  /** The active Work/Play/Private posture, for routing buddy actions through the gate. */
  posture?: UserPosture;
  onGatewayConnect: () => void;
  onGatewayDisconnect: () => void;
  onGatewaySettingsChange: (settings: GatewaySettings) => void;
  onLayoutChange?: () => void;
  /** Ask the host to undock this buddy into the interactive free surface. */
  onRequestInteract?: () => void;
  /** Ask the host to re-dock (tuck) this buddy back onto the border. */
  onRequestDock?: () => void;
  onSendChat: (payload: { text: string; purpose?: string; context?: string }) => boolean;
  onSettingsChange: (settings: BuddySettings) => void;
  onPostureChange?: (posture: UserPosture) => void;
  onPlacementChange?: (placement: { enabledBuddyIds: readonly string[]; buddyEdges: Readonly<Record<string, string>> }) => void;
  onGovernanceSnapshotChange?: (snapshot: BuddyGovernanceSnapshot | null) => void;
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
    preferCenterFit = false,
    hasGateway,
    gatewayState,
    gatewayDetail,
    gatewayBusy,
    gatewayUrl,
    gatewayAutoConnect,
    message,
    messageMedia,
    settings,
    posture = "work",
    onGatewayConnect,
    onGatewayDisconnect,
    onGatewaySettingsChange,
    onLayoutChange,
    onRequestInteract,
    onRequestDock,
    onSendChat,
    onSettingsChange,
    onPostureChange,
    onPlacementChange,
    onGovernanceSnapshotChange,
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
  // The latest governance action-gate outcome, rendered as a receipt card. `pendingEffector`
  // holds the effector awaiting confirmation so a Confirm re-invokes the gate with confirmed.
  const [actionReceipt, setActionReceipt] = useState<ActionReceipt | null>(null);
  const [pendingEffector, setPendingEffector] = useState<string | null>(null);
  const [panelShift, setPanelShift] = useState({ x: 0, y: 0 });
  const [surfaceUi, setSurfaceUi] = useState<BuddySurfaceUiState>(() =>
    loadStoredBuddySurfaceUi(buddy.id, preferCenterFit),
  );
  const [onboardingSurface, setOnboardingSurface] = useState(() =>
    loadStoredOnboardingSurfaceState(),
  );
  const [onboardingHubSection, setOnboardingHubSection] = useState<OnboardingPanelSection | null>(null);
  const lastMessageRef = useRef("");
  const governanceSnapshotChangeRef = useRef(onGovernanceSnapshotChange);
  // A message typed while the gateway is still offline is parked here so it can
  // auto-send the moment the connection comes online (seamless first reply).
  const pendingSendRef = useRef<ChatPayload | null>(null);
  const autoConnectAttemptedRef = useRef(false);


  // The display mode is now derived purely from the dock state. Docked = the
  // compact ambient bubble; undocked = the full interactive panel.
  const displayMode: BuddyDisplayMode = interactive ? "full" : "compact";

  const profile = BUDDY_PROFILES[buddy.id];
  const gatewayOnline = hasGateway && gatewayState === "connected";
  const wizardEnabled = buddy.id === "hermes";
  const bubbleSide = edge === "right" ? "left" : "right";
  const bubbleVertical = edge === "top" || dockSlot < 0.22 ? "below" : "above";
  const trustWorkbenchMode = buddy.id === "fox" ? "nexus" : buddy.id === "owl" ? "veritas" : null;
  const bubbleTab = surfaceUi.activeTab;
  const onboardingModel = useMemo(
    () =>
      wizardEnabled
        ? buildOnboardingPanelModel({
            state: onboardingSurface.progress,
            receiptKinds: onboardingSurface.receiptKinds,
            sectionOverride: onboardingHubSection,
            capabilities: BUDDY_PROFILES[buddy.id]?.capabilities ?? null,
          })
        : null,
    [buddy.id, onboardingHubSection, onboardingSurface.progress, onboardingSurface.receiptKinds, wizardEnabled],
  );
  const governanceSnapshot = useMemo(
    () =>
      buildBuddyGovernanceSnapshot({
        buddyId: buddy.id,
        history,
        settings,
      }),
    [buddy.id, history, settings],
  );

  // Close any transient overlays automatically when the buddy re-docks so they
  // can never leave a stale hitbox behind on the border.
  useEffect(() => {
    if (!interactive) {
      setSettingsOpen(false);
      setMenuOpen(false);
    }
  }, [interactive]);

  useEffect(() => {
    setSurfaceUi(loadStoredBuddySurfaceUi(buddy.id, preferCenterFit));
  }, [buddy.id, preferCenterFit]);

  useEffect(() => {
    saveStoredBuddySurfaceUi(buddy.id, surfaceUi);
  }, [buddy.id, surfaceUi]);

  useEffect(() => {
    saveStoredOnboardingSurfaceState(onboardingSurface);
  }, [onboardingSurface]);

  useEffect(() => {
    if (!wizardEnabled || !onboardingModel) {
      return;
    }
    if (onboardingModel.mode === "hub" && onboardingHubSection === null) {
      setOnboardingHubSection("summary");
      return;
    }
    if (onboardingModel.mode !== "hub" && onboardingHubSection !== null) {
      setOnboardingHubSection(null);
    }
  }, [onboardingHubSection, onboardingModel, wizardEnabled]);

  // The panel stands in for the Host soul's timer on timeout-advancing acts
  // (find_me): linger long enough to read the cue, then move the script along.
  // advanceWizard is intentionally outside the deps (see bubbleTabs note); the
  // functional update inside it keeps a late firing safe.
  const idleAutoAdvanceMs = onboardingModel?.idle?.autoAdvanceMs ?? null;
  useEffect(() => {
    if (idleAutoAdvanceMs === null) {
      return;
    }
    const timer = window.setTimeout(() => advanceWizard("timeout"), idleAutoAdvanceMs);
    return () => window.clearTimeout(timer);
  }, [idleAutoAdvanceMs, onboardingModel?.act.id]);

  useEffect(() => {
    governanceSnapshotChangeRef.current = onGovernanceSnapshotChange;
  }, [onGovernanceSnapshotChange]);

  const governanceSnapshotKey = useMemo(() => {
    if (!governanceSnapshot) {
      return "none";
    }

    const latestReceipt = governanceSnapshot.frame.receipts[governanceSnapshot.frame.receipts.length - 1];

    return [
      governanceSnapshot.purpose,
      latestReceipt?.receipt_id ?? "no-receipt",
      governanceSnapshot.prompt.included.length,
      governanceSnapshot.prompt.excluded.length,
    ].join(":");
  }, [governanceSnapshot]);

  useEffect(() => {
    governanceSnapshotChangeRef.current?.(governanceSnapshot);
  }, [governanceSnapshot, governanceSnapshotKey]);

  const statusBubbleText = useMemo(() => {
    const trimmedMessage = message.trim();

    if (gatewayBusy) {
      return "Hermes is thinking…";
    }

    if (hasGateway) {
      if (gatewayOnline) {
        return trimmedMessage || gatewayDetail || "Hermes gateway ready.";
      }

      if (gatewayState === "connecting") {
        return "Connecting to Hermes gateway…";
      }

      if (gatewayState === "error" || gatewayState === "disconnected") {
        return trimmedMessage || gatewayDetail || "Hermes gateway attention required.";
      }

      return trimmedMessage || "Hermes gateway offline.";
    }

    return trimmedMessage || `${buddy.shortName} ready.`;
  }, [
    buddy.shortName,
    gatewayBusy,
    gatewayDetail,
    gatewayOnline,
    gatewayState,
    hasGateway,
    message,
  ]);

  useEffect(() => {
    const trimmed = message.trim();
    if (!trimmed || trimmed === lastMessageRef.current) {
      return;
    }

    lastMessageRef.current = trimmed;
    setHistory((current) => [
      ...current,
      {
        id: createLineId(),
        role: "assistant",
        text: trimmed,
        media: messageMedia ?? undefined,
      },
    ]);
  }, [message, messageMedia]);

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

    if (
      hasGateway &&
      gatewayAutoConnect &&
      !gatewayOnline &&
      !autoConnectAttemptedRef.current &&
      gatewayState !== "connecting"
    ) {
      autoConnectAttemptedRef.current = true;
      onGatewayConnect();
    }
  }, [gatewayAutoConnect, interactive, hasGateway, gatewayOnline, gatewayState, onGatewayConnect]);

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

        const panelBox = settingsOpen ? null : rectToHitbox(panelRef.current);
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

        if (edge === "top" || edge === "bottom") {
          if (rect.left < margin) {
            x = margin - rect.left;
          } else if (rect.right > window.innerWidth - margin) {
            x = window.innerWidth - margin - rect.right;
          }
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
  }, [displayMode, edge, history.length, interactive, menuOpen, settingsOpen, surfaceUi.alwaysCenterFit]);

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
    surfaceUi.alwaysCenterFit,
  ]);

  function forceLayoutRefresh() {
    if (onLayoutChange) {
      // Two rAFs to let DOM settle before native hitbox measurement.
      requestAnimationFrame(() => requestAnimationFrame(() => onLayoutChange()));
    }
  }

  // Run a buddy action through the soul-side governance gate. The body never authorizes
  // (AGENTS.md law 7) — it hands the request to the gate, which emits the ActionReceipt and
  // persists it. Confirmation re-runs the same effector with `confirmed`, which can clear the
  // risk floor but never a hard block.
  function runActionReview(effectorId: string, confirmed: boolean) {
    const { receipt } = handleActionRequest({
      buddy: buddy.id,
      effectorId,
      settings,
      posture,
      history: history.map((line) => ({ role: line.role, text: line.text })),
      confirmed,
    });
    setActionReceipt(receipt);
    setPendingEffector(receipt.decision === "needs_confirmation" ? effectorId : null);
    updateSurfaceUi({ activeTab: "message" });
    setHistory((current) => [
      ...current,
      { id: createLineId(), role: "status", text: `${receipt.effector}: ${receipt.decision}` },
    ]);
  }

  function handleSubmit() {
    const trimmed = draft.trim();
    if (!trimmed) {
      return;
    }

    // Slash commands that drive the governance action gate, intercepted before any
    // text reaches the provider. `/review [effector]` requests an effector (default the
    // read-only receipt_review); `/confirm` confirms a pending needs_confirmation action.
    if (trimmed.startsWith("/review") || trimmed === "/confirm") {
      setDraft("");
      if (trimmed === "/confirm") {
        if (pendingEffector) {
          runActionReview(pendingEffector, true);
        }
        return;
      }
      const effectorId = trimmed.slice("/review".length).trim() || "receipt_review";
      runActionReview(effectorId, false);
      return;
    }

    const payload: ChatPayload = {
      text: trimmed,
      purpose: governanceSnapshot?.purpose,
      context: governanceSnapshot?.prompt.context,
    };

    // Always show the user's message immediately so the chat feels responsive.
    setHistory((current) => [
      ...current,
      { id: createLineId(), role: "user", text: trimmed },
    ]);
    setDraft("");

    // Offline: park the message and bring the gateway up. The auto-flush effect
    // below sends it the moment the connection is live — no second click needed.
    if (hasGateway && !gatewayOnline) {
      pendingSendRef.current = payload;
      setHistory((current) => [
        ...current,
        { id: createLineId(), role: "status", text: "Connecting to gateway…" },
      ]);
      onGatewayConnect();
      return;
    }

    const sent = onSendChat(payload);
    if (!sent) {
      // Couldn't deliver right now — keep it queued and (re)connect.
      pendingSendRef.current = payload;
      if (hasGateway) {
        onGatewayConnect();
      }
    }
  }

  function updateSurfaceUi(patch: Partial<BuddySurfaceUiState>) {
    setSurfaceUi((current) => normalizeBuddySurfaceUi({ ...current, ...patch }));
  }

  function setActiveBubbleTab(tabId: string) {
    if (isBubbleTabId(tabId)) {
      updateSurfaceUi({ activeTab: tabId });
    }
  }

  function toggleBubbleSection(tabId: BubbleTabId) {
    setSurfaceUi((current) => normalizeBuddySurfaceUi({
      ...current,
      expandedSections: {
        ...current.expandedSections,
        [tabId]: !current.expandedSections[tabId],
      },
    }));
  }

  function updateOnboardingDraft(patch: Partial<OnboardingSurfaceDraft>) {
    setOnboardingSurface((current) => ({
      ...current,
      draft: { ...current.draft, ...patch },
    }));
  }

  function advanceWizard(event: string) {
    if (!onboardingModel) {
      return;
    }

    // Record a real, durable lifecycle milestone — but only when the act genuinely
    // advances AND carries a receipt (first_contact/find_me carry none). The ledger
    // is append-only, so re-running a step in the hub honestly logs a fresh dated
    // milestone; `lifecycleReceiptKinds` collapses it for the linear-vs-hub decision.
    const advancingAct = currentAct(onboardingSurface.progress);
    const advances = advancingAct.advanceOn.includes(event as OnboardingEvent);
    const recordedKinds =
      advancingAct.receipt && advances
        ? lifecycleReceiptKinds(
            recordLifecycleReceipt({
              kind: advancingAct.receipt,
              detail: receiptDetail(advancingAct, onboardingSurface.draft),
            }),
          )
        : null;

    // Progress is advanced inside the functional update so a delayed caller (the
    // find_me auto-advance timer) can never clobber state written after its closure
    // was captured. Receipt kinds come from the durable ledger we just wrote.
    setOnboardingSurface((current) => ({
      ...current,
      progress: advanceOnboarding(current.progress, event as OnboardingEvent),
      receiptKinds: recordedKinds ?? current.receiptKinds,
    }));

    // Settings writes are keyed on the panel event itself — each event is only ever
    // emitted by its own section, in both linear and hub mode.
    if (event === "panel:connection_ok") {
      const draft = onboardingSurface.draft;
      // Layering: the browser only ever connects to the LOCAL gateway over its
      // WebSocket URL — it never talks to the provider directly. The provider
      // endpoint + API key the user entered here belong gateway-side in `.env`
      // (the gateway holds the secret and makes the HTTPS call). So we keep the
      // existing gateway URL and only enable/trigger autoConnect; writing
      // draft.apiBase into GatewaySettings.url would point the socket at the
      // provider's REST endpoint and break the connection.
      onGatewaySettingsChange({ url: gatewayUrl, autoConnect: true });
      const presetBuddyProvider = presetProvider(draft.provider);
      onSettingsChange({ ...settings, provider: presetBuddyProvider, modelLabel: draft.model });
      if (hasGateway && !gatewayOnline) {
        onGatewayConnect();
      }
    }
    if (event === "panel:posture_set") {
      onPostureChange?.(onboardingSurface.draft.posture);
    }
    if (event === "panel:next") {
      onPlacementChange?.({
        enabledBuddyIds: onboardingSurface.draft.enabledBuddyIds,
        buddyEdges: onboardingSurface.draft.buddyEdges,
      });
    }

    // Handoff fires only on the linear flow's first completion — hub-mode saves keep
    // the user on the section they were editing instead of yanking them away.
    const nextState = advanceOnboarding(onboardingSurface.progress, event as OnboardingEvent);
    if (!onboardingSurface.progress.completed && nextState.completed) {
      setOnboardingHubSection("summary");
      updateSurfaceUi({ activeTab: "message" });
    }
  }

  function presetProvider(id: HermesProviderPresetId) {
    return HERMES_PROVIDER_PRESETS.find((p) => p.id === id)?.buddyProvider ?? "custom";
  }

  // Honest, minimal detail captured alongside each milestone — what was actually set,
  // never the secret itself (the API key is recorded as present/absent, not its value).
  function receiptDetail(
    act: OnboardingAct,
    draft: OnboardingSurfaceDraft,
  ): Record<string, string | number | boolean> | undefined {
    switch (act.receipt) {
      case "credential.stored":
        return {
          provider: presetProvider(draft.provider),
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

  function selectWizardSection(section: OnboardingPanelSection) {
    if (onboardingModel?.mode === "hub") {
      setOnboardingHubSection(section);
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

  const bubbleTabs = useMemo(() => {
    const gatewayLabel = hasGateway
      ? gatewayOnline
        ? "Gateway connected"
        : gatewayState === "connecting"
          ? "Gateway connecting"
          : "Gateway offline"
      : "No gateway for this buddy";

    return [
      {
        id: "message",
        label: "Message",
        icon: "●",
        tone: "message" as const,
        content: (
          <BubbleSection
            expanded={surfaceUi.expandedSections.message === true}
            summary={statusBubbleText}
            title="Latest output"
            onToggle={() => toggleBubbleSection("message")}
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
                actionReceipt ? (
                  <ActionReceiptCard
                    receipt={actionReceipt}
                    onConfirm={pendingEffector ? () => runActionReview(pendingEffector, true) : undefined}
                  />
                ) : bubbleTab === "setup" && wizardEnabled && onboardingModel ? (
                  <OnboardingWizardPanel
                    model={onboardingModel}
                    draft={onboardingSurface.draft}
                    onAdvance={advanceWizard}
                    onDraftChange={updateOnboardingDraft}
                    onSectionSelect={selectWizardSection}
                  />
                ) : trustWorkbenchMode ? (
                  <TrustWorkbenchPanel
                    mode={trustWorkbenchMode}
                    title={trustWorkbenchMode === "nexus" ? "Nexus context grades" : "Veritas receipt checks"}
                    compact
                    snapshot={governanceSnapshot}
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
          </BubbleSection>
        ),
      },
      ...(wizardEnabled && onboardingModel
        ? [
            {
              id: "setup",
              label: "Setup",
              icon: "✦",
              tone: "setup" as const,
              content: (
                <BubbleSection
                  expanded={surfaceUi.expandedSections.setup === true}
                  summary={
                    onboardingModel.mode === "hub"
                      ? "Setup hub is ready."
                      : `Next: ${onboardingModel.act.title}`
                  }
                  title="Onboarding wizard"
                  onToggle={() => toggleBubbleSection("setup")}
                >
                  <BubbleAction
                    title={onboardingModel.mode === "hub" ? "Setup hub" : onboardingModel.act.title}
                    detail={
                      onboardingModel.mode === "hub"
                        ? "Jump back into connection, posture, placement, or receipts."
                        : "Open the guided setup panel and keep moving through the current act."
                    }
                    actionLabel="Open setup"
                    onAction={() => updateSurfaceUi({ activeTab: "setup" })}
                  />
                </BubbleSection>
              ),
            },
          ]
        : []),
      {
        id: "settings",
        label: "Settings",
        icon: "⚙",
        tone: "settings" as const,
        content: (
          <BubbleSection
            expanded={surfaceUi.expandedSections.settings === true}
            summary={`${settings.modelLabel} · ${settings.memoryMode.replace("_", " ")}`}
            title="Model & gateway settings"
            onToggle={() => toggleBubbleSection("settings")}
          >
            <BubbleAction
              title="Settings"
              detail={`${settings.modelLabel} · ${settings.memoryMode.replace("_", " ")}`}
              actionLabel="Open settings"
              onAction={() => {
                setSettingsOpen(true);
                forceLayoutRefresh();
              }}
            />
            <BubbleToggle
              checked={surfaceUi.alwaysCenterFit}
              label="Always centre and fit full height"
              onChange={(checked) => updateSurfaceUi({ alwaysCenterFit: checked })}
            />
            <BubbleToggle
              checked={surfaceUi.preventSettingsOverflow}
              label="Keep settings inside border"
              onChange={(checked) => updateSurfaceUi({ preventSettingsOverflow: checked })}
            />
          </BubbleSection>
        ),
      },
      {
        id: "gateway",
        label: "Gateway",
        icon: "◎",
        tone: "gateway" as const,
        content: (
          <BubbleSection
            expanded={surfaceUi.expandedSections.gateway === true}
            summary={gatewayDetail ?? gatewayUrl}
            title={gatewayLabel}
            onToggle={() => toggleBubbleSection("gateway")}
          >
            <BubbleAction
              title={gatewayLabel}
              detail={gatewayDetail ?? gatewayUrl}
              actionLabel={gatewayOnline ? "Disconnect gateway" : "Connect gateway"}
              disabled={!hasGateway || gatewayState === "connecting"}
              onAction={gatewayOnline ? onGatewayDisconnect : onGatewayConnect}
            />
          </BubbleSection>
        ),
      },
      {
        id: "dock",
        label: "Dock",
        icon: "⤓",
        tone: "dock" as const,
        content: (
          <BubbleSection
            expanded={surfaceUi.expandedSections.dock === true}
            summary="Use this when the panel should return to the border."
            title="Dock control"
            onToggle={() => toggleBubbleSection("dock")}
          >
            <BubbleAction
              title="Dock to border"
              detail="Collapse this buddy back to a small speech output."
              actionLabel="Dock to border"
              onAction={onRequestDock}
            />
          </BubbleSection>
        ),
      },
    ];
  }, [
    gatewayDetail,
    gatewayOnline,
    gatewayState,
    gatewayUrl,
    hasGateway,
    onGatewayConnect,
    onGatewayDisconnect,
    onRequestDock,
    buddy.shortName,
    displayMode,
    draft,
    gatewayBusy,
    history,
    trustWorkbenchMode,
    governanceSnapshot,
    onboardingSurface.draft,
    settings.memoryMode,
    settings.modelLabel,
    statusBubbleText,
    surfaceUi,
    bubbleTab,
    // advanceWizard / selectWizardSection / updateOnboardingDraft are recreated each
    // render; like the other inline handlers here they're intentionally omitted so the
    // memo only rebuilds when the state they close over changes.
    onboardingModel,
    wizardEnabled,
  ]);

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
          text={statusBubbleText}
          phase="visible"
          mounted
          clickable
          bubbleSide={bubbleSide}
          bubbleVertical={bubbleVertical}
          activationLabel="Open chat"
          title="Open chat — undock buddy"
          onActivate={onRequestInteract}
        />
      ) : (
        <>
          <div
            ref={panelRef}
            className={[
              "buddy-cluster__panel-wrap",
              surfaceUi.alwaysCenterFit ? "buddy-cluster__panel-wrap--center-fit" : "",
              settingsOpen ? "buddy-cluster__panel-wrap--settings-open" : "",
            ].join(" ")}
            style={{
              "--buddy-panel-shift-x": `${panelShift.x}px`,
              "--buddy-panel-shift-y": `${panelShift.y}px`,
            } as CSSProperties}
          >
            <BuddyUiBubble
              text={statusBubbleText}
              phase="visible"
              mounted
              clickable={false}
              bubbleSide={bubbleSide}
              bubbleVertical={bubbleVertical}
              inline
              tabs={bubbleTabs}
              activeTab={bubbleTab}
              onTabChange={setActiveBubbleTab}
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
            preventOverflow={surfaceUi.preventSettingsOverflow}
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

function BubbleAction({
  title,
  detail,
  actionLabel,
  disabled = false,
  onAction,
}: {
  title: string;
  detail: string;
  actionLabel: string;
  disabled?: boolean;
  onAction?: () => void;
}) {
  return (
    <div className="buddy-ui-bubble__action">
      <strong>{title}</strong>
      <span>{detail}</span>
      <button
        type="button"
        disabled={disabled}
        onClick={(event) => {
          event.preventDefault();
          event.stopPropagation();
          onAction?.();
        }}
      >
        {actionLabel}
      </button>
    </div>
  );
}

function BubbleSection({
  children,
  expanded,
  summary,
  title,
  onToggle,
}: {
  children: ReactNode;
  expanded: boolean;
  summary: string;
  title: string;
  onToggle: () => void;
}) {
  return (
    <section className="buddy-ui-bubble__section">
      <button
        className="buddy-ui-bubble__section-toggle"
        type="button"
        aria-expanded={expanded}
        onClick={(event) => {
          event.preventDefault();
          event.stopPropagation();
          onToggle();
        }}
      >
        <span>{title}</span>
        <small>{summary}</small>
      </button>
      {expanded ? <div className="buddy-ui-bubble__section-body">{children}</div> : null}
    </section>
  );
}

function BubbleToggle({
  checked,
  label,
  onChange,
}: {
  checked: boolean;
  label: string;
  onChange: (checked: boolean) => void;
}) {
  return (
    <label className="buddy-ui-bubble__toggle">
      <span>{label}</span>
      <input
        type="checkbox"
        checked={checked}
        onChange={(event) => onChange(event.target.checked)}
        onClick={(event) => event.stopPropagation()}
      />
    </label>
  );
}

function loadStoredBuddySurfaceUi(buddyId: string, preferCenterFit = false): BuddySurfaceUiState {
  try {
    const raw = localStorage.getItem(BUDDY_SURFACE_UI_STORAGE_KEY);
    if (!raw) {
      return preferCenterFit
        ? { ...DEFAULT_BUDDY_SURFACE_UI, alwaysCenterFit: true }
        : DEFAULT_BUDDY_SURFACE_UI;
    }

    const stored = JSON.parse(raw) as Partial<Record<string, Partial<BuddySurfaceUiState>>>;
    const parsed = normalizeBuddySurfaceUi(stored[buddyId]);
    if (preferCenterFit && !parsed.alwaysCenterFit) {
      return { ...parsed, alwaysCenterFit: true };
    }
    return parsed;
  } catch {
    return preferCenterFit
      ? { ...DEFAULT_BUDDY_SURFACE_UI, alwaysCenterFit: true }
      : DEFAULT_BUDDY_SURFACE_UI;
  }
}

function saveStoredBuddySurfaceUi(buddyId: string, state: BuddySurfaceUiState) {
  try {
    const raw = localStorage.getItem(BUDDY_SURFACE_UI_STORAGE_KEY);
    const stored = raw ? JSON.parse(raw) as Record<string, BuddySurfaceUiState> : {};
    stored[buddyId] = normalizeBuddySurfaceUi(state);
    localStorage.setItem(BUDDY_SURFACE_UI_STORAGE_KEY, JSON.stringify(stored));
  } catch {
    // Persisting UI posture is best-effort. The visual defaults remain safe.
  }
}

function normalizeBuddySurfaceUi(candidate: Partial<BuddySurfaceUiState> | null | undefined): BuddySurfaceUiState {
  const activeTab = isBubbleTabId(candidate?.activeTab) ? candidate.activeTab : DEFAULT_BUDDY_SURFACE_UI.activeTab;
  const expandedCandidate = candidate?.expandedSections && typeof candidate.expandedSections === "object"
    ? candidate.expandedSections
    : {};

  return {
    activeTab,
    expandedSections: {
      message: expandedCandidate.message === true,
      setup: expandedCandidate.setup === true,
      settings: expandedCandidate.settings === true,
      gateway: expandedCandidate.gateway === true,
      dock: expandedCandidate.dock === true,
    },
    alwaysCenterFit: candidate?.alwaysCenterFit === true,
    preventSettingsOverflow: candidate?.preventSettingsOverflow !== false,
  };
}

function isBubbleTabId(value: unknown): value is BubbleTabId {
  return value === "message" || value === "setup" || value === "settings" || value === "gateway" || value === "dock";
}

export function buddySurfaceStatusLabel(
  hasGateway: boolean,
  gatewayState: GatewayConnectionState,
) {
  if (!hasGateway) {
    return "";
  }

  return connectionLabelForState(gatewayState);
}
