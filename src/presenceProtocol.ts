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
 *             where to look.  move_to | express | say | attention | hydrate
 *
 *   to-soul   (body → soul): lifecycle + interaction events the body reports about
 *             itself. The soul reasons over these (and they are tomorrow's receipt
 *             inputs, so each kind means exactly one thing).  attached | clicked |
 *             grabbed | dragged | dropped | summoned | dismissed
 *
 * Discriminator note: presence messages carry `protocol: "presence"`, which keeps
 * them cleanly separable from the legacy gateway messages (which carry `type`) on
 * the shared WebSocket. A relay can forward both; a client switches on the field.
 */

import { isOutputSurfaceKind, type OutputSurfaceKind } from "./buddyCapabilities";

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

/** Full snapshot so a late-joining or reconnecting body can hydrate at once. */
export type PresenceHydrate = PresenceEnvelope<
  "hydrate",
  {
    position?: PresencePosition;
    emotion?: PresenceEmotion;
    speech?: string;
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

export type PresenceToBodyMessage =
  | PresenceMoveTo
  | PresenceExpress
  | PresenceSay
  | PresenceAttention
  | PresenceHydrate
  | PresenceOutput;

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

export type PresenceToSoulMessage =
  | PresenceAttached
  | PresenceClicked
  | PresenceGrabbed
  | PresenceDragged
  | PresenceDropped
  | PresenceSummoned
  | PresenceDismissed
  | PresenceSaid;

export type PresenceMessage = PresenceToBodyMessage | PresenceToSoulMessage;

export type PresenceKind = PresenceMessage["kind"];

export const PRESENCE_TO_BODY_KINDS: readonly PresenceToBodyMessage["kind"][] = [
  "move_to",
  "express",
  "say",
  "attention",
  "hydrate",
  "output",
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

function isEdge(value: unknown): value is PresenceEdge {
  return value === "top" || value === "right" || value === "bottom" || value === "left";
}

function isSpace(value: unknown): value is PresenceSpace {
  return value === "viewport" || value === "screen" || value === "normalized";
}

function isEmotion(value: unknown): value is PresenceEmotion {
  return (PRESENCE_EMOTIONS as readonly unknown[]).includes(value);
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
        (raw.speech === undefined || typeof raw.speech === "string")
      );
    case "output":
      return isValidOutputPayload(raw);
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
    snapshot: { position?: PresencePosition; emotion?: PresenceEmotion; speech?: string },
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
};
