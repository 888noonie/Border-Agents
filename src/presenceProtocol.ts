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
 *   to-soul   (body → soul): interaction events the body reports about itself.
 *             The soul reasons over these.  clicked | grabbed | dragged | dropped
 *             | summoned | dismissed
 *
 * Discriminator note: presence messages carry `protocol: "presence"`, which keeps
 * them cleanly separable from the legacy gateway messages (which carry `type`) on
 * the shared WebSocket. A relay can forward both; a client switches on the field.
 */

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
 * Body-agnostic position. `anchored` tucks the buddy against a screen edge with a
 * pixel offset (maps to layer-shell anchor+margin); `free` floats it at a point in
 * a named coordinate space.
 */
export type PresencePosition =
  | { mode: "anchored"; edge: PresenceEdge; offset: { x: number; y: number } }
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

export type PresenceToBodyMessage =
  | PresenceMoveTo
  | PresenceExpress
  | PresenceSay
  | PresenceAttention
  | PresenceHydrate;

// --- to-soul: the body reporting what happened to it ---------------------------

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

export type PresenceToSoulMessage =
  | PresenceClicked
  | PresenceGrabbed
  | PresenceDragged
  | PresenceDropped
  | PresenceSummoned
  | PresenceDismissed;

export type PresenceMessage = PresenceToBodyMessage | PresenceToSoulMessage;

export type PresenceKind = PresenceMessage["kind"];

export const PRESENCE_TO_BODY_KINDS: readonly PresenceToBodyMessage["kind"][] = [
  "move_to",
  "express",
  "say",
  "attention",
  "hydrate",
];

export const PRESENCE_TO_SOUL_KINDS: readonly PresenceToSoulMessage["kind"][] = [
  "clicked",
  "grabbed",
  "dragged",
  "dropped",
  "summoned",
  "dismissed",
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

  if (value.mode === "anchored") {
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
};
