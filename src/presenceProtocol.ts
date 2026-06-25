/**
 * Presence Protocol v0
 * ====================
 *
 * The small typed event schema that lets one buddy "soul" (the agent runtime)
 * possess many "bodies" (presence surfaces) over the existing buddy WebSocket.
 *
 * This protocol — not any one renderer — is the product. Once a body speaks it,
 * the same soul drives a browser-extension body, a desktop wlr-layer-shell body,
 * a phone body, and eventually a kinetic one, by swapping only the renderer.
 *
 * Two event families ride the same socket:
 *
 *   to-body   (soul → body): the LLM's presence "tool calls". The body is a dumb
 *             puppet that renders these — where to sit, how to feel, what to say,
 *             where to look, what governance outcome to show.  move_to | express |
 *             say | attention | hydrate | output | action_result
 *
 *   to-soul   (body → soul): lifecycle + interaction events the body reports about
 *             itself. The soul reasons over these (and they are tomorrow's receipt
 *             inputs, so each kind means exactly one thing).  attached | clicked |
 *             grabbed | dragged | dropped | summoned | dismissed | said |
 *             action_request
 *
 * Discriminator note: presence messages carry `protocol: "presence"`, which keeps
 * them cleanly separable from the legacy gateway messages (which carry `type`) on
 * the shared WebSocket. A relay can forward both; a client switches on the field.
 */

import { isOutputSurfaceKind, type OutputSurfaceKind } from "./buddyCapabilities";
import type { ActionDecision } from "./core";
import { isUserPosture, type UserPosture } from "./core/userPosture";

export const PRESENCE_PROTOCOL = "presence" as const;
export const PRESENCE_PROTOCOL_VERSION = 0 as const;

/** Which way an event travels. Informational — both ride the same socket. */
export type PresenceDirection = "to-body" | "to-soul";

export type PresenceEdge = "top" | "right" | "bottom" | "left";

/**
 * Coordinate space a `free` position is expressed in. A body maps the abstract
 * position onto its own primitive: the browser body → viewport pixels; a
 * wlr-layer-shell body → anchor + pixel margins; an NSPanel → screen frame.
 */
export type PresenceSpace = "viewport" | "screen" | "normalized";

export type PresenceEmotion =
  | "neutral"
  | "happy"
  | "thinking"
  | "curious"
  | "alert"
  | "sleepy";

export const PRESENCE_EMOTIONS: readonly PresenceEmotion[] = [
  "neutral",
  "happy",
  "thinking",
  "curious",
  "alert",
  "sleepy",
];

/**
 * Glanceable governance alert tier — the chrome (passport tint, later the route ring)
 * the body paints alongside the face. It rides on `action_result`, NOT `express`, so the
 * face (`decision` → `Emotion::for_decision`) and the chrome (`alertLevel`) derive from one
 * event and one truth: the body never infers policy status from a facial-expression string
 * (law 7), and there is no express/action_result ordering race. `quiet` is the resting tier;
 * unknown decisions fail loud at `critical`, never a reassuring `quiet`.
 */
export type PresenceAlertLevel = "quiet" | "ready" | "confirm" | "blocked" | "critical";

export const PRESENCE_ALERT_LEVELS: readonly PresenceAlertLevel[] = [
  "quiet",
  "ready",
  "confirm",
  "blocked",
  "critical",
];

/**
 * Body-agnostic position. `anchored` floats the *whole* buddy near a screen edge
 * with a pixel offset (maps to layer-shell anchor+margin); `tucked` parks it flush
 * against an edge in a minimized form (the body shows only a tucked bump/peek, not
 * the full figure) with the offset giving its position *along* that edge — the
 * along-edge axis is `y` for left/right edges and `x` for top/bottom; the flush axis
 * is ignored. `free` floats it at a point in a named coordinate space.
 *
 * `tucked` is a distinct mode, not anchored-with-zero-offset, because it changes how
 * the body *presents* (minimized bump vs full figure) and what summon/dismiss mean —
 * keeping it explicit means a persisted drop position round-trips through `hydrate`
 * with its tucked-ness intact, instead of springing back to the full figure.
 */
export type PresencePosition =
  | { mode: "anchored"; edge: PresenceEdge; offset: { x: number; y: number } }
  | { mode: "tucked"; edge: PresenceEdge; offset: { x: number; y: number } }
  | { mode: "free"; space: PresenceSpace; x: number; y: number };

/** Where the buddy is attending. */
export type PresenceFocus =
  | "user"
  | "screen"
  | "away"
  | { point: { x: number; y: number }; space: PresenceSpace };

export type PresencePointer = "primary" | "secondary";

/**
 * Bounds of a tracked native OS window, in **logical pixels** in a single global
 * coordinate space, with the `scaleFactor` that produced them carried alongside.
 *
 * Every platform driver (the COSMIC `cosmic-toplevel-info` body, a future Win32 hook)
 * MUST convert into this one canonical space *before* emitting, so the body never has
 * to know which OS produced a rectangle. The body does the final device-pixel math
 * with `scaleFactor` — this is what stops the frame from drifting off the edge of a
 * window dragged onto a HiDPI/4K external monitor.
 */
export type TargetBounds = {
  x: number;
  y: number;
  w: number;
  h: number;
  scaleFactor: number;
};

/**
 * Why the body lost its grip on a target. A closed union, not a free string, because
 * the body pattern-matches on it to pick an animation (release gracefully on `closed`
 * vs. look around confused on `trackingFailed`) — a typo'd reason must fail typecheck,
 * not silently skip the animation.
 */
export type TargetLostReason = "closed" | "workspaceSwitched" | "minimized" | "trackingFailed";

/**
 * Host-platform capabilities, reported once in `hydrate`. These describe the *body's
 * window shell and its driver*, not the buddy — so the soul/UI can degrade gracefully
 * on a platform where a mechanism is missing instead of assuming it everywhere.
 *
 * `canClickThrough` is a property of the window shell (XShape / layer-shell / Win32
 * styles), not the geometry driver, but it rides here because `hydrate` is the one
 * body-level snapshot and the body owns both.
 */
export type PresencePlatform = {
  canTrackGeometry: boolean;
  canInjectInput: boolean;
  canClickThrough: boolean;
};

type PresenceEnvelope<Kind extends string, Payload> = {
  protocol: typeof PRESENCE_PROTOCOL;
  v: typeof PRESENCE_PROTOCOL_VERSION;
  kind: Kind;
  /** Which buddy this concerns (e.g. "hermes"). */
  buddy: string;
  /** Epoch milliseconds the event was minted. */
  ts: number;
} & Payload;

// --- to-body: the soul possessing the body -------------------------------------

export type PresenceMoveTo = PresenceEnvelope<
  "move_to",
  { position: PresencePosition; transitionMs?: number }
>;

export type PresenceExpress = PresenceEnvelope<
  "express",
  { emotion: PresenceEmotion; intensity?: number; pose?: string }
>;

export type PresenceSay = PresenceEnvelope<
  "say",
  { text: string; ttlMs?: number; replyTo?: string }
>;

export type PresenceAttention = PresenceEnvelope<
  "attention",
  { focus: PresenceFocus }
>;

/**
 * How a surface stands relative to its backing effector, as the body should present it:
 * `available` (no effector needed) and `gated` (wired, authorized at act time) render
 * normally; `unwired` (names an effector not yet wired) renders dimmed. This is the wire
 * twin of the soul-side `SurfaceAvailability` (src/surfaceManifest.ts) — the native body
 * NEVER imports that manifest, so availability must arrive soul-pushed here (Slice 2a).
 */
export type PresenceSurfaceAvailability = "available" | "unwired" | "gated";

export const PRESENCE_SURFACE_AVAILABILITIES: readonly PresenceSurfaceAvailability[] = [
  "available",
  "unwired",
  "gated",
];

/**
 * One entry in the ordered surface list shipped on `hydrate`. The body cycles this list
 * (so it no longer needs a hardcoded surface order) and dims entries by `availability`.
 */
export interface PresenceSurfaceDescriptor {
  id: string;
  label: string;
  availability: PresenceSurfaceAvailability;
  /**
   * `surface` (default) cycles/switches the active surface. `launcher` opens an external tool
   * via a reach `action_request` for `effector`, instead of becoming the active surface. Additive
   * (Slice 0): an absent `kind` means `surface`, so older snapshots and fixtures remain valid.
   */
  kind?: PresenceSurfaceDescriptorKind;
  /** For a `launcher`, the reach effector id the body requests (e.g. "open_cursor"). */
  effector?: string;
}

export type PresenceSurfaceDescriptorKind = "surface" | "launcher";

/**
 * Full snapshot so a late-joining or reconnecting body can hydrate at once. `surfaces` is
 * the ordered, soul-pushed surface list (canonical SURFACE_ORDER) with per-surface
 * availability — the body cycles and dims from this instead of any local manifest.
 */
export type PresenceHydrate = PresenceEnvelope<
  "hydrate",
  {
    position?: PresencePosition;
    emotion?: PresenceEmotion;
    speech?: string;
    platform?: PresencePlatform;
    surfaces?: PresenceSurfaceDescriptor[];
  }
>;

/**
 * Rich result content the body renders in its output surface (the desktop torso, the
 * browser content panel) — distinct from `say`, which is the ephemeral speech bubble.
 *
 * `surface` selects the renderer: `text` (a text card), `image`/`file` (rendered from
 * inline bytes), or `session` (clear back to the idle status card). Bytes are always
 * **inlined as base64** by the soul/gateway, never a URL — the native wlr-layer-shell
 * body speaks `ws://` only and has no HTTP/TLS client, so it must receive the bytes,
 * not fetch them. `mediaType` + `dataBase64` are required for image/file; `text`
 * carries the text-card body; `session` needs neither.
 */
export type PresenceOutput = PresenceEnvelope<
  "output",
  {
    surface: OutputSurfaceKind;
    text?: string;
    caption?: string;
    mediaType?: string;
    dataBase64?: string;
  }
>;

/**
 * Border-target tracking (the "Morph Frame" seam) — soul/driver → body. A platform
 * driver tells the body where a native OS window is, so the body can wrap its hollow
 * torso around it. The body never learns whether a rectangle came from
 * `cosmic-toplevel-info`, an XWayland poll, or a Win32 hook: a driver is simply "a
 * process that speaks presence protocol", which makes the seam wire-level, not a
 * shared-language Rust trait.
 *
 * Split into three kinds so the body can bind distinct CSS to the lifecycle, and so a
 * `targetMoved` is never ambiguous: every event carries `targetId`, and `targetAcquired`
 * carries the initial `bounds` so there is no empty-handed gap before the first move.
 */
export type PresenceTargetAcquired = PresenceEnvelope<
  "target_acquired",
  { targetId: string; title: string; appId: string; bounds: TargetBounds }
>;

export type PresenceTargetMoved = PresenceEnvelope<
  "target_moved",
  { targetId: string; bounds: TargetBounds }
>;

export type PresenceTargetLost = PresenceEnvelope<
  "target_lost",
  { targetId: string; reason: TargetLostReason }
>;

/**
 * The world-facing execution outcome on an `action_result` (additive, v0). `executed` is
 * the load-bearing bit: it lets a dumb body truthfully show "blocked before run" vs
 * "executed and receipted" without any policy reasoning. The full `ExecutionReceipt` stays
 * soul-side (law 7); only this thin summary + its id cross the wire. `route` records which
 * provider carried the effect, and whether it was a downgrade ("buddies persist, providers
 * rotate"). Present only on `allow` paths.
 */
export interface PresenceActionOutcome {
  executed: boolean;
  executionReceiptId?: string;
  route?: { provider: string; locality: "local" | "cloud"; downgraded: boolean; fallbackOf?: string };
}

/**
 * Outcome of an `action_request` the soul ran through the governance action gate
 * (src/core/actionGate.ts) — soul → body. Distinct from `output` (rendered artifact
 * bytes) and `say` (ephemeral speech): an authorization outcome is a governance result
 * the body renders as a badge/affordance, so it can key on `decision` without sniffing
 * prose. The full `ActionReceipt` stays soul-side in the ledger (law 7 — the body never
 * holds the authorization, only a cue about it); only `decision` + `receiptId` + an
 * optional human `summary` (+ optional execution `outcome`) cross the wire. `requestId`
 * correlates back to the request.
 */
export type PresenceActionResult = PresenceEnvelope<
  "action_result",
  {
    effector: string;
    decision: ActionDecision;
    receiptId: string;
    requestId?: string;
    summary?: string;
    outcome?: PresenceActionOutcome;
    /**
     * Chrome twin of `decision` — the soul derives it (src/soulActions.ts
     * `decisionAlertLevel`) so the body tints the passport/ring from this one cue
     * instead of re-deriving policy state locally. Optional/additive (v0).
     */
    alertLevel?: PresenceAlertLevel;
  }
>;

/**
 * The route the active surface is riding — provider label, where it runs (`locality`), and
 * its `health`. Carried as one nested object (not a flat `locality`) so the Slice 1 passport
 * row and the Slice 3 outer route ring read the same shape and no field has to migrate later.
 * `health` is optional: the soul omits it until route health derivation lands (Slice 3).
 */
export type SurfaceRoute = {
  label: string;
  locality: "local" | "cloud";
  health?: "ready" | "degraded" | "unavailable";
};

export type PresenceSurfaceActive = PresenceEnvelope<
  "surface_active",
  {
    surface: string;
    posture: UserPosture;
    label?: string;
    providerLabel?: string;
    route?: SurfaceRoute;
  }
>;

export type PresenceToBodyMessage =
  | PresenceMoveTo
  | PresenceExpress
  | PresenceSay
  | PresenceAttention
  | PresenceHydrate
  | PresenceOutput
  | PresenceActionResult
  | PresenceSurfaceActive
  | PresenceTargetAcquired
  | PresenceTargetMoved
  | PresenceTargetLost;

// --- to-soul: the body reporting what happened to it ---------------------------

/**
 * Lifecycle handshake: the body announces itself when it comes online or reconnects,
 * so the soul knows to push a `hydrate`. This is deliberately NOT `summoned` —
 * `summoned` means the *user* opened the buddy's surface; `attached` means the *body*
 * came online. Conflating them would poison the soul's reasoning and any audit trail.
 * `at` is the body's current position if it has one; `capabilities` hints what this
 * body can render/do (e.g. `["drag", "menu", "say"]`).
 */
export type PresenceAttached = PresenceEnvelope<
  "attached",
  { at?: PresencePosition; capabilities?: readonly string[] }
>;

export type PresenceClicked = PresenceEnvelope<
  "clicked",
  { button?: PresencePointer; at?: PresencePosition }
>;

export type PresenceGrabbed = PresenceEnvelope<"grabbed", { at: PresencePosition }>;

export type PresenceDragged = PresenceEnvelope<"dragged", { at: PresencePosition }>;

export type PresenceDropped = PresenceEnvelope<
  "dropped",
  { at: PresencePosition; onTarget?: string }
>;

/** User opened the buddy's chat/menu surface. */
export type PresenceSummoned = PresenceEnvelope<"summoned", Record<never, never>>;

/** User dismissed the buddy's chat/menu surface. */
export type PresenceDismissed = PresenceEnvelope<"dismissed", Record<never, never>>;

/** User typed a message to the buddy through its on-body input box. */
export type PresenceSaid = PresenceEnvelope<"said", { text: string }>;

/**
 * User dragged the buddy's visible frame head while it was wrapped around a native
 * target. This is only a request: OS window movement is an effector routed through the
 * soul/driver, never a body capability.
 */
export type PresenceTargetDragRequested = PresenceEnvelope<
  "target_drag_requested",
  { targetId: string; delta: { x: number; y: number } }
>;

/**
 * Wire-side typed intent on an `action_request` (additive, v0). The protocol stays
 * manifest-free: `operation` and `target.kind` are validated as strings here and checked
 * against the manifest + gate in the soul handler. This is the membrane on the wire — only
 * a typed `intent` may authorize an `act` effector; `context` is legacy and NEVER
 * authoritative (it exists for old `/review` convenience). The body fills these fields; it
 * does not interpret them.
 */
export interface PresenceActionIntent {
  operation: string;
  target?: { kind: "repo_path" | "file_path" | "url" | "command" | "none"; value?: string };
  payloadDigest?: string;
  summary?: string;
}

/** Advisory hint about the route the body believes it is on. The soul resolves the
 * authoritative route and records it on the ExecutionReceipt — this never decides anything. */
export interface PresenceRouteHint {
  provider?: string;
  locality?: "local" | "cloud";
}

/**
 * User asked the buddy to run one of its granted effectors (e.g. a `/review` affordance
 * → `receipt_review`, or `/review repo_edit scratch.md` → a typed `repo_edit` intent).
 * This is only a *request*: authorization happens in the soul's action gate, never in the
 * body (law 7). `effector` is a free string on the wire and is validated against the
 * manifest in the soul handler, so the protocol stays manifest-free. `confirmed: true` is
 * the follow-up after a `needs_confirmation` result — it can only clear the confirmation
 * floor, never widen a hard block. `requestId` correlates the resulting `action_result`.
 */
export type PresenceActionRequest = PresenceEnvelope<
  "action_request",
  {
    effector: string;
    context?: string;
    confirmed?: boolean;
    requestId?: string;
    intent?: PresenceActionIntent;
    routeHint?: PresenceRouteHint;
  }
>;

export type PresenceSurfaceRequest = PresenceEnvelope<"surface_request", { surface: string }>;

export type PresenceToSoulMessage =
  | PresenceAttached
  | PresenceClicked
  | PresenceGrabbed
  | PresenceDragged
  | PresenceDropped
  | PresenceSummoned
  | PresenceDismissed
  | PresenceSaid
  | PresenceActionRequest
  | PresenceSurfaceRequest
  | PresenceTargetDragRequested;

export type PresenceMessage = PresenceToBodyMessage | PresenceToSoulMessage;

export type PresenceKind = PresenceMessage["kind"];

export const PRESENCE_TO_BODY_KINDS: readonly PresenceToBodyMessage["kind"][] = [
  "move_to",
  "express",
  "say",
  "attention",
  "hydrate",
  "output",
  "action_result",
  "surface_active",
  "target_acquired",
  "target_moved",
  "target_lost",
];

export const PRESENCE_TO_SOUL_KINDS: readonly PresenceToSoulMessage["kind"][] = [
  "attached",
  "clicked",
  "grabbed",
  "dragged",
  "dropped",
  "summoned",
  "dismissed",
  "said",
  "action_request",
  "surface_request",
  "target_drag_requested",
];

export function presenceDirection(kind: PresenceKind): PresenceDirection {
  return (PRESENCE_TO_BODY_KINDS as readonly string[]).includes(kind)
    ? "to-body"
    : "to-soul";
}

// --- parsing / validation ------------------------------------------------------

function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function isFiniteNumber(value: unknown): value is number {
  return typeof value === "number" && Number.isFinite(value);
}

function isNonEmptyString(value: unknown): value is string {
  return typeof value === "string" && value.length > 0;
}

function isStringArray(value: unknown): value is string[] {
  return Array.isArray(value) && value.every((item) => typeof item === "string");
}

const ACTION_TARGET_KINDS: readonly string[] = ["repo_path", "file_path", "url", "command", "none"];

function isActionIntent(value: unknown): value is PresenceActionIntent {
  if (!isObject(value)) return false;
  if (!isNonEmptyString(value.operation)) return false;
  if (value.target !== undefined) {
    if (!isObject(value.target)) return false;
    if (typeof value.target.kind !== "string" || !ACTION_TARGET_KINDS.includes(value.target.kind)) return false;
    if (value.target.value !== undefined && typeof value.target.value !== "string") return false;
  }
  if (value.payloadDigest !== undefined && typeof value.payloadDigest !== "string") return false;
  if (value.summary !== undefined && typeof value.summary !== "string") return false;
  return true;
}

function isRouteHint(value: unknown): value is PresenceRouteHint {
  if (!isObject(value)) return false;
  if (value.provider !== undefined && typeof value.provider !== "string") return false;
  if (value.locality !== undefined && value.locality !== "local" && value.locality !== "cloud") return false;
  return true;
}

function isActionOutcome(value: unknown): value is PresenceActionOutcome {
  if (!isObject(value)) return false;
  if (typeof value.executed !== "boolean") return false;
  if (value.executionReceiptId !== undefined && typeof value.executionReceiptId !== "string") return false;
  if (value.route !== undefined) {
    const route = value.route;
    if (!isObject(route)) return false;
    if (!isNonEmptyString(route.provider)) return false;
    if (route.locality !== "local" && route.locality !== "cloud") return false;
    if (typeof route.downgraded !== "boolean") return false;
    if (route.fallbackOf !== undefined && typeof route.fallbackOf !== "string") return false;
  }
  return true;
}

function isEdge(value: unknown): value is PresenceEdge {
  return value === "top" || value === "right" || value === "bottom" || value === "left";
}

function isSpace(value: unknown): value is PresenceSpace {
  return value === "viewport" || value === "screen" || value === "normalized";
}

function isEmotion(value: unknown): value is PresenceEmotion {
  return (PRESENCE_EMOTIONS as readonly unknown[]).includes(value);
}

function isAlertLevel(value: unknown): value is PresenceAlertLevel {
  return (PRESENCE_ALERT_LEVELS as readonly unknown[]).includes(value);
}

function isSurfaceRoute(value: unknown): value is SurfaceRoute {
  if (!isObject(value)) return false;
  if (!isNonEmptyString(value.label)) return false;
  if (value.locality !== "local" && value.locality !== "cloud") return false;
  if (
    value.health !== undefined &&
    value.health !== "ready" &&
    value.health !== "degraded" &&
    value.health !== "unavailable"
  ) {
    return false;
  }
  return true;
}

function isPosition(value: unknown): value is PresencePosition {
  if (!isObject(value)) {
    return false;
  }

  if (value.mode === "anchored" || value.mode === "tucked") {
    const offset = value.offset;
    return (
      isEdge(value.edge) &&
      isObject(offset) &&
      isFiniteNumber(offset.x) &&
      isFiniteNumber(offset.y)
    );
  }

  if (value.mode === "free") {
    return isSpace(value.space) && isFiniteNumber(value.x) && isFiniteNumber(value.y);
  }

  return false;
}

function isTargetBounds(value: unknown): value is TargetBounds {
  return (
    isObject(value) &&
    isFiniteNumber(value.x) &&
    isFiniteNumber(value.y) &&
    isFiniteNumber(value.w) &&
    isFiniteNumber(value.h) &&
    isFiniteNumber(value.scaleFactor)
  );
}

function isActionDecision(value: unknown): value is ActionDecision {
  return value === "allow" || value === "needs_confirmation" || value === "blocked";
}

function isTargetLostReason(value: unknown): value is TargetLostReason {
  return (
    value === "closed" ||
    value === "workspaceSwitched" ||
    value === "minimized" ||
    value === "trackingFailed"
  );
}

function isSurfaceAvailability(value: unknown): value is PresenceSurfaceAvailability {
  return (PRESENCE_SURFACE_AVAILABILITIES as readonly unknown[]).includes(value);
}

function isSurfaceDescriptorArray(value: unknown): value is PresenceSurfaceDescriptor[] {
  return (
    Array.isArray(value) &&
    value.every(
      (item) =>
        isObject(item) &&
        isNonEmptyString(item.id) &&
        typeof item.label === "string" &&
        isSurfaceAvailability(item.availability) &&
        // Additive (Slice 0 launchers): both fields optional. A present-but-bad `kind`/`effector`
        // drops the whole cue, mirroring the Rust `parse_surfaces` guard.
        (item.kind === undefined || item.kind === "surface" || item.kind === "launcher") &&
        (item.effector === undefined || typeof item.effector === "string"),
    )
  );
}

function isPlatform(value: unknown): value is PresencePlatform {
  return (
    isObject(value) &&
    typeof value.canTrackGeometry === "boolean" &&
    typeof value.canInjectInput === "boolean" &&
    typeof value.canClickThrough === "boolean"
  );
}

function isFocus(value: unknown): value is PresenceFocus {
  if (value === "user" || value === "screen" || value === "away") {
    return true;
  }

  if (!isObject(value)) {
    return false;
  }

  const point = value.point;
  return (
    isObject(point) &&
    isFiniteNumber(point.x) &&
    isFiniteNumber(point.y) &&
    isSpace(value.space)
  );
}

/**
 * Validate an `output` payload. image/file must carry inline bytes (`dataBase64`) and a
 * `mediaType`; `text` must carry a string body; `session` (clear) needs neither. Any
 * present optional field must still be the right type.
 */
function isValidOutputPayload(raw: Record<string, unknown>): boolean {
  if (!isOutputSurfaceKind(raw.surface)) {
    return false;
  }
  if (raw.text !== undefined && typeof raw.text !== "string") {
    return false;
  }
  if (raw.caption !== undefined && typeof raw.caption !== "string") {
    return false;
  }
  if (raw.mediaType !== undefined && typeof raw.mediaType !== "string") {
    return false;
  }
  if (raw.dataBase64 !== undefined && typeof raw.dataBase64 !== "string") {
    return false;
  }
  if (raw.surface === "image" || raw.surface === "file") {
    return isNonEmptyString(raw.dataBase64) && isNonEmptyString(raw.mediaType);
  }
  if (raw.surface === "text") {
    return typeof raw.text === "string";
  }
  return true; // session: a clear/reset signal, no payload required
}

/** Validate a kind-specific payload. Returns false for anything malformed. */
function isValidForKind(kind: PresenceKind, raw: Record<string, unknown>): boolean {
  switch (kind) {
    case "move_to":
      return (
        isPosition(raw.position) &&
        (raw.transitionMs === undefined || isFiniteNumber(raw.transitionMs))
      );
    case "express":
      return (
        isEmotion(raw.emotion) &&
        (raw.intensity === undefined || isFiniteNumber(raw.intensity)) &&
        (raw.pose === undefined || typeof raw.pose === "string")
      );
    case "say":
      return (
        typeof raw.text === "string" &&
        (raw.ttlMs === undefined || isFiniteNumber(raw.ttlMs)) &&
        (raw.replyTo === undefined || typeof raw.replyTo === "string")
      );
    case "attention":
      return isFocus(raw.focus);
    case "hydrate":
      return (
        (raw.position === undefined || isPosition(raw.position)) &&
        (raw.emotion === undefined || isEmotion(raw.emotion)) &&
        (raw.speech === undefined || typeof raw.speech === "string") &&
        (raw.platform === undefined || isPlatform(raw.platform)) &&
        (raw.surfaces === undefined || isSurfaceDescriptorArray(raw.surfaces))
      );
    case "output":
      return isValidOutputPayload(raw);
    case "action_result":
      return (
        isNonEmptyString(raw.effector) &&
        isActionDecision(raw.decision) &&
        isNonEmptyString(raw.receiptId) &&
        (raw.requestId === undefined || typeof raw.requestId === "string") &&
        (raw.summary === undefined || typeof raw.summary === "string") &&
        (raw.outcome === undefined || isActionOutcome(raw.outcome)) &&
        (raw.alertLevel === undefined || isAlertLevel(raw.alertLevel))
      );
    case "surface_active":
      return (
        isNonEmptyString(raw.surface) &&
        isUserPosture(raw.posture) &&
        (raw.label === undefined || typeof raw.label === "string") &&
        (raw.providerLabel === undefined || typeof raw.providerLabel === "string") &&
        (raw.route === undefined || isSurfaceRoute(raw.route))
      );
    case "target_acquired":
      return (
        isNonEmptyString(raw.targetId) &&
        typeof raw.title === "string" &&
        typeof raw.appId === "string" &&
        isTargetBounds(raw.bounds)
      );
    case "target_moved":
      return isNonEmptyString(raw.targetId) && isTargetBounds(raw.bounds);
    case "target_lost":
      return isNonEmptyString(raw.targetId) && isTargetLostReason(raw.reason);
    case "attached":
      return (
        (raw.at === undefined || isPosition(raw.at)) &&
        (raw.capabilities === undefined || isStringArray(raw.capabilities))
      );
    case "clicked":
      return (
        (raw.button === undefined || raw.button === "primary" || raw.button === "secondary") &&
        (raw.at === undefined || isPosition(raw.at))
      );
    case "grabbed":
    case "dragged":
      return isPosition(raw.at);
    case "dropped":
      return isPosition(raw.at) && (raw.onTarget === undefined || typeof raw.onTarget === "string");
    case "summoned":
    case "dismissed":
      return true;
    case "said":
      return typeof raw.text === "string" && raw.text.length > 0;
    case "action_request":
      return (
        isNonEmptyString(raw.effector) &&
        (raw.context === undefined || typeof raw.context === "string") &&
        (raw.confirmed === undefined || typeof raw.confirmed === "boolean") &&
        (raw.requestId === undefined || typeof raw.requestId === "string") &&
        (raw.intent === undefined || isActionIntent(raw.intent)) &&
        (raw.routeHint === undefined || isRouteHint(raw.routeHint))
      );
    case "surface_request":
      return isNonEmptyString(raw.surface);
    case "target_drag_requested":
      return (
        isNonEmptyString(raw.targetId) &&
        isObject(raw.delta) &&
        isFiniteNumber(raw.delta.x) &&
        isFiniteNumber(raw.delta.y)
      );
    default:
      return false;
  }
}

const ALL_KINDS: readonly string[] = [...PRESENCE_TO_BODY_KINDS, ...PRESENCE_TO_SOUL_KINDS];

/** Cheap discriminator: is this a presence envelope at all (vs a gateway message)? */
export function isPresenceEnvelope(raw: unknown): raw is { protocol: typeof PRESENCE_PROTOCOL } {
  return isObject(raw) && raw.protocol === PRESENCE_PROTOCOL;
}

/**
 * Parse and validate an inbound presence message. Returns null for anything that
 * is not a well-formed, version-0 presence message — a malformed event must never
 * crash a body or a soul.
 */
export function parsePresenceMessage(raw: unknown): PresenceMessage | null {
  if (!isObject(raw)) {
    return null;
  }

  if (raw.protocol !== PRESENCE_PROTOCOL || raw.v !== PRESENCE_PROTOCOL_VERSION) {
    return null;
  }

  if (typeof raw.kind !== "string" || !ALL_KINDS.includes(raw.kind)) {
    return null;
  }

  if (!isNonEmptyString(raw.buddy) || !isFiniteNumber(raw.ts)) {
    return null;
  }

  if (!isValidForKind(raw.kind as PresenceKind, raw)) {
    return null;
  }

  return raw as unknown as PresenceMessage;
}

// --- factory helpers -----------------------------------------------------------

type EnvelopeOptions = {
  /** Override the timestamp (defaults to Date.now()). Handy for tests. */
  ts?: number;
};

function envelope<Kind extends PresenceKind>(
  kind: Kind,
  buddy: string,
  payload: Record<string, unknown>,
  options?: EnvelopeOptions,
) {
  return {
    protocol: PRESENCE_PROTOCOL,
    v: PRESENCE_PROTOCOL_VERSION,
    kind,
    buddy,
    ts: options?.ts ?? Date.now(),
    ...payload,
  };
}

/**
 * Typed constructors for every presence event. Bodies and souls build messages
 * through these so the envelope and version are never hand-assembled at call sites.
 */
export const presence = {
  moveTo(
    buddy: string,
    position: PresencePosition,
    opts: { transitionMs?: number } & EnvelopeOptions = {},
  ): PresenceMoveTo {
    const { transitionMs, ts } = opts;
    return envelope("move_to", buddy, { position, transitionMs }, { ts }) as PresenceMoveTo;
  },
  express(
    buddy: string,
    emotion: PresenceEmotion,
    opts: { intensity?: number; pose?: string } & EnvelopeOptions = {},
  ): PresenceExpress {
    const { intensity, pose, ts } = opts;
    return envelope("express", buddy, { emotion, intensity, pose }, { ts }) as PresenceExpress;
  },
  say(
    buddy: string,
    text: string,
    opts: { ttlMs?: number; replyTo?: string } & EnvelopeOptions = {},
  ): PresenceSay {
    const { ttlMs, replyTo, ts } = opts;
    return envelope("say", buddy, { text, ttlMs, replyTo }, { ts }) as PresenceSay;
  },
  attention(buddy: string, focus: PresenceFocus, opts: EnvelopeOptions = {}): PresenceAttention {
    return envelope("attention", buddy, { focus }, opts) as PresenceAttention;
  },
  hydrate(
    buddy: string,
    snapshot: {
      position?: PresencePosition;
      emotion?: PresenceEmotion;
      speech?: string;
      platform?: PresencePlatform;
      surfaces?: PresenceSurfaceDescriptor[];
    },
    opts: EnvelopeOptions = {},
  ): PresenceHydrate {
    return envelope("hydrate", buddy, { ...snapshot }, opts) as PresenceHydrate;
  },
  output(
    buddy: string,
    payload: {
      surface: OutputSurfaceKind;
      text?: string;
      caption?: string;
      mediaType?: string;
      dataBase64?: string;
    },
    opts: EnvelopeOptions = {},
  ): PresenceOutput {
    return envelope("output", buddy, { ...payload }, opts) as PresenceOutput;
  },
  actionResult(
    buddy: string,
    payload: {
      effector: string;
      decision: ActionDecision;
      receiptId: string;
      requestId?: string;
      summary?: string;
      outcome?: PresenceActionOutcome;
      alertLevel?: PresenceAlertLevel;
    },
    opts: EnvelopeOptions = {},
  ): PresenceActionResult {
    return envelope("action_result", buddy, { ...payload }, opts) as PresenceActionResult;
  },
  surfaceActive(
    buddy: string,
    payload: {
      surface: string;
      posture: UserPosture;
      label?: string;
      providerLabel?: string;
      route?: SurfaceRoute;
    },
    opts: EnvelopeOptions = {},
  ): PresenceSurfaceActive {
    return envelope("surface_active", buddy, { ...payload }, opts) as PresenceSurfaceActive;
  },
  targetAcquired(
    buddy: string,
    target: { targetId: string; title: string; appId: string; bounds: TargetBounds },
    opts: EnvelopeOptions = {},
  ): PresenceTargetAcquired {
    return envelope("target_acquired", buddy, { ...target }, opts) as PresenceTargetAcquired;
  },
  targetMoved(
    buddy: string,
    target: { targetId: string; bounds: TargetBounds },
    opts: EnvelopeOptions = {},
  ): PresenceTargetMoved {
    return envelope("target_moved", buddy, { ...target }, opts) as PresenceTargetMoved;
  },
  targetLost(
    buddy: string,
    target: { targetId: string; reason: TargetLostReason },
    opts: EnvelopeOptions = {},
  ): PresenceTargetLost {
    return envelope("target_lost", buddy, { ...target }, opts) as PresenceTargetLost;
  },
  attached(
    buddy: string,
    opts: { at?: PresencePosition; capabilities?: readonly string[] } & EnvelopeOptions = {},
  ): PresenceAttached {
    const { at, capabilities, ts } = opts;
    return envelope("attached", buddy, { at, capabilities }, { ts }) as PresenceAttached;
  },
  clicked(
    buddy: string,
    opts: { button?: PresencePointer; at?: PresencePosition } & EnvelopeOptions = {},
  ): PresenceClicked {
    const { button, at, ts } = opts;
    return envelope("clicked", buddy, { button, at }, { ts }) as PresenceClicked;
  },
  grabbed(buddy: string, at: PresencePosition, opts: EnvelopeOptions = {}): PresenceGrabbed {
    return envelope("grabbed", buddy, { at }, opts) as PresenceGrabbed;
  },
  dragged(buddy: string, at: PresencePosition, opts: EnvelopeOptions = {}): PresenceDragged {
    return envelope("dragged", buddy, { at }, opts) as PresenceDragged;
  },
  dropped(
    buddy: string,
    at: PresencePosition,
    opts: { onTarget?: string } & EnvelopeOptions = {},
  ): PresenceDropped {
    const { onTarget, ts } = opts;
    return envelope("dropped", buddy, { at, onTarget }, { ts }) as PresenceDropped;
  },
  summoned(buddy: string, opts: EnvelopeOptions = {}): PresenceSummoned {
    return envelope("summoned", buddy, {}, opts) as PresenceSummoned;
  },
  dismissed(buddy: string, opts: EnvelopeOptions = {}): PresenceDismissed {
    return envelope("dismissed", buddy, {}, opts) as PresenceDismissed;
  },
  said(buddy: string, text: string, opts: EnvelopeOptions = {}): PresenceSaid {
    return envelope("said", buddy, { text }, opts) as PresenceSaid;
  },
  actionRequest(
    buddy: string,
    effector: string,
    opts: {
      context?: string;
      confirmed?: boolean;
      requestId?: string;
      intent?: PresenceActionIntent;
      routeHint?: PresenceRouteHint;
    } & EnvelopeOptions = {},
  ): PresenceActionRequest {
    const { context, confirmed, requestId, intent, routeHint, ts } = opts;
    return envelope(
      "action_request",
      buddy,
      { effector, context, confirmed, requestId, intent, routeHint },
      { ts },
    ) as PresenceActionRequest;
  },
  surfaceRequest(buddy: string, surface: string, opts: EnvelopeOptions = {}): PresenceSurfaceRequest {
    return envelope("surface_request", buddy, { surface }, opts) as PresenceSurfaceRequest;
  },
  targetDragRequested(
    buddy: string,
    target: { targetId: string; delta: { x: number; y: number } },
    opts: EnvelopeOptions = {},
  ): PresenceTargetDragRequested {
    return envelope("target_drag_requested", buddy, { ...target }, opts) as PresenceTargetDragRequested;
  },
};
