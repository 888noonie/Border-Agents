/**
 * Presence Protocol v0 — browser-body mirror.
 *
 * Plain-JS counterpart of src/presenceProtocol.ts, loaded into the content script
 * so the browser-extension body can speak the protocol. Keep the two files in sync:
 * the TypeScript module is canonical, this is the hand-mirrored runtime copy (same
 * pattern as hermes.js / profiles.js).
 *
 * Exposes globalThis.BorderBuddiesPresence with:
 *   - factory helpers for the events a body emits and consumes
 *   - parse(): validate an inbound message (null when malformed or not presence)
 *   - isEnvelope(): cheap "is this presence vs a gateway message" discriminator
 *   - position mapping between the body's placement model and PresencePosition
 */
(function attachPresenceProtocol(global) {
  "use strict";

  const PROTOCOL = "presence";
  const VERSION = 0;

  const TO_BODY_KINDS = ["move_to", "express", "say", "attention", "hydrate"];
  const TO_SOUL_KINDS = ["clicked", "grabbed", "dragged", "dropped", "summoned", "dismissed"];
  const ALL_KINDS = [...TO_BODY_KINDS, ...TO_SOUL_KINDS];
  const EMOTIONS = ["neutral", "happy", "thinking", "curious", "alert", "sleepy"];

  function isObject(value) {
    return typeof value === "object" && value !== null;
  }

  function isFiniteNumber(value) {
    return typeof value === "number" && Number.isFinite(value);
  }

  function isEdge(value) {
    return value === "top" || value === "right" || value === "bottom" || value === "left";
  }

  function isSpace(value) {
    return value === "viewport" || value === "screen" || value === "normalized";
  }

  function isPosition(value) {
    if (!isObject(value)) {
      return false;
    }
    if (value.mode === "anchored") {
      return (
        isEdge(value.edge) &&
        isObject(value.offset) &&
        isFiniteNumber(value.offset.x) &&
        isFiniteNumber(value.offset.y)
      );
    }
    if (value.mode === "free") {
      return isSpace(value.space) && isFiniteNumber(value.x) && isFiniteNumber(value.y);
    }
    return false;
  }

  function isFocus(value) {
    if (value === "user" || value === "screen" || value === "away") {
      return true;
    }
    return (
      isObject(value) &&
      isObject(value.point) &&
      isFiniteNumber(value.point.x) &&
      isFiniteNumber(value.point.y) &&
      isSpace(value.space)
    );
  }

  function isValidForKind(kind, raw) {
    switch (kind) {
      case "move_to":
        return isPosition(raw.position) && (raw.transitionMs === undefined || isFiniteNumber(raw.transitionMs));
      case "express":
        return EMOTIONS.includes(raw.emotion) && (raw.intensity === undefined || isFiniteNumber(raw.intensity)) && (raw.pose === undefined || typeof raw.pose === "string");
      case "say":
        return typeof raw.text === "string" && (raw.ttlMs === undefined || isFiniteNumber(raw.ttlMs)) && (raw.replyTo === undefined || typeof raw.replyTo === "string");
      case "attention":
        return isFocus(raw.focus);
      case "hydrate":
        return (raw.position === undefined || isPosition(raw.position)) && (raw.emotion === undefined || EMOTIONS.includes(raw.emotion)) && (raw.speech === undefined || typeof raw.speech === "string");
      case "clicked":
        return (raw.button === undefined || raw.button === "primary" || raw.button === "secondary") && (raw.at === undefined || isPosition(raw.at));
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

  function isEnvelope(raw) {
    return isObject(raw) && raw.protocol === PROTOCOL;
  }

  function parse(raw) {
    if (!isObject(raw) || raw.protocol !== PROTOCOL || raw.v !== VERSION) {
      return null;
    }
    if (typeof raw.kind !== "string" || !ALL_KINDS.includes(raw.kind)) {
      return null;
    }
    if (typeof raw.buddy !== "string" || raw.buddy.length === 0 || !isFiniteNumber(raw.ts)) {
      return null;
    }
    if (!isValidForKind(raw.kind, raw)) {
      return null;
    }
    return raw;
  }

  function envelope(kind, buddy, payload) {
    return Object.assign({ protocol: PROTOCOL, v: VERSION, kind, buddy, ts: Date.now() }, payload);
  }

  // --- placement <-> presence position mapping (browser body) ---
  // The body's placement model is { state: "tucked"|"free", edge, x?, y? }.
  function placementToPosition(placement) {
    if (placement && placement.state === "free" && isFiniteNumber(placement.x) && isFiniteNumber(placement.y)) {
      return { mode: "free", space: "viewport", x: placement.x, y: placement.y };
    }
    return { mode: "anchored", edge: isEdge(placement && placement.edge) ? placement.edge : "right", offset: { x: 0, y: 0 } };
  }

  function positionToPlacement(position, fallbackEdge) {
    const edge = isEdge(fallbackEdge) ? fallbackEdge : "right";
    if (position && position.mode === "free") {
      return { state: "free", edge, x: position.x, y: position.y };
    }
    if (position && position.mode === "anchored") {
      return { state: "tucked", edge: isEdge(position.edge) ? position.edge : edge };
    }
    return null;
  }

  global.BorderBuddiesPresence = {
    PROTOCOL,
    VERSION,
    TO_BODY_KINDS,
    TO_SOUL_KINDS,
    EMOTIONS,
    isEnvelope,
    parse,
    placementToPosition,
    positionToPlacement,
    // factory helpers — the events the body emits
    clicked(buddy, opts) {
      const o = opts || {};
      return envelope("clicked", buddy, { button: o.button, at: o.at });
    },
    grabbed(buddy, at) {
      return envelope("grabbed", buddy, { at });
    },
    dragged(buddy, at) {
      return envelope("dragged", buddy, { at });
    },
    dropped(buddy, at, onTarget) {
      return envelope("dropped", buddy, { at, onTarget });
    },
    summoned(buddy) {
      return envelope("summoned", buddy, {});
    },
    dismissed(buddy) {
      return envelope("dismissed", buddy, {});
    },
  };
})(globalThis);
