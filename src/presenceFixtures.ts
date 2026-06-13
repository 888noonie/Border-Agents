/**
 * Cross-language golden fixtures for Presence Protocol v0 — pure data, no Node.
 *
 * One canonical message per kind, built from the canonical TypeScript factories.
 * The committed `fixtures/presence-v0.json` is generated from these (see
 * `scripts/gen-presence-fixtures.ts`) and read by BOTH test suites:
 *   - vitest (`src/__tests__/presenceFixtures.test.ts`) — drift guard + parsing.
 *   - the Rust body (`desktop-body`, Step 4) — parser/serializer parity in cargo test.
 *
 * Keeping this module Node-free lets it sit inside the typechecked `src` graph; the
 * file-writing lives in the script, which tsc never compiles.
 */
import { presence, type PresencePosition } from "./presenceProtocol";

const BUDDY = "hermes";
const ANCHORED: PresencePosition = { mode: "anchored", edge: "right", offset: { x: 24, y: 48 } };
const TUCKED: PresencePosition = { mode: "tucked", edge: "left", offset: { x: 0, y: 400 } };
const FREE: PresencePosition = { mode: "free", space: "screen", x: 800, y: 120 };
const BOUNDS = { x: 320, y: 180, w: 1280, h: 720, scaleFactor: 2 };
const PLATFORM = { canTrackGeometry: true, canInjectInput: false, canClickThrough: true };

/** Fixed timestamps keep the emitted fixture byte-stable across regenerations. */
export const PRESENCE_FIXTURES = {
  move_to: presence.moveTo(BUDDY, ANCHORED, { transitionMs: 180, ts: 1000 }),
  express: presence.express(BUDDY, "happy", { intensity: 0.8, ts: 1001 }),
  say: presence.say(BUDDY, "Hi — I'm your setup host.", { ts: 1002 }),
  attention: presence.attention(BUDDY, "user", { ts: 1003 }),
  hydrate: presence.hydrate(BUDDY, { position: ANCHORED, emotion: "neutral", speech: "ready", platform: PLATFORM }, { ts: 1004 }),
  attached: presence.attached(BUDDY, { at: ANCHORED, capabilities: ["drag", "menu", "say"], ts: 1005 }),
  clicked: presence.clicked(BUDDY, { button: "primary", at: FREE, ts: 1006 }),
  grabbed: presence.grabbed(BUDDY, FREE, { ts: 1007 }),
  dragged: presence.dragged(BUDDY, FREE, { ts: 1008 }),
  dropped: presence.dropped(BUDDY, TUCKED, { onTarget: "dock", ts: 1009 }),
  summoned: presence.summoned(BUDDY, { ts: 1010 }),
  dismissed: presence.dismissed(BUDDY, { ts: 1011 }),
  said: presence.said(BUDDY, "what's on my calendar today?", { ts: 1012 }),
  output: presence.output(
    BUDDY,
    { surface: "image", mediaType: "image/png", caption: "a red bicycle", dataBase64: "iVBORw0KGgo=" },
    { ts: 1013 },
  ),
  target_acquired: presence.targetAcquired(
    BUDDY,
    { targetId: "win-42", title: "Firefox", appId: "org.mozilla.firefox", bounds: BOUNDS },
    { ts: 1014 },
  ),
  target_moved: presence.targetMoved(BUDDY, { targetId: "win-42", bounds: BOUNDS }, { ts: 1015 }),
  target_lost: presence.targetLost(BUDDY, { targetId: "win-42", reason: "closed" }, { ts: 1016 }),
  target_drag_requested: presence.targetDragRequested(
    BUDDY,
    { targetId: "win-42", delta: { x: 12, y: -4 } },
    { ts: 1017 },
  ),
  action_request: presence.actionRequest(BUDDY, "receipt_review", { requestId: "req-1", ts: 1018 }),
  action_result: presence.actionResult(
    BUDDY,
    { effector: "receipt_review", decision: "allow", receiptId: "action:hermes:receipt_review:t0", requestId: "req-1", summary: "Opening the receipt ledger." },
    { ts: 1019 },
  ),
} as const;

/** Canonical serialization (drops `undefined`, stable 2-space indent, trailing nl). */
export function serializeFixtures(): string {
  return `${JSON.stringify(PRESENCE_FIXTURES, null, 2)}\n`;
}
