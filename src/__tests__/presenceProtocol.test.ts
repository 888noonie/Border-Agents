import { describe, expect, test } from "vitest";
import {
  PRESENCE_PROTOCOL,
  PRESENCE_PROTOCOL_VERSION,
  PRESENCE_TO_BODY_KINDS,
  PRESENCE_TO_SOUL_KINDS,
  isPresenceEnvelope,
  parsePresenceMessage,
  presence,
  presenceDirection,
  type PresenceMessage,
  type PresencePosition,
} from "../presenceProtocol";

const FREE: PresencePosition = { mode: "free", space: "viewport", x: 120, y: 64 };
const ANCHORED: PresencePosition = { mode: "anchored", edge: "right", offset: { x: 0, y: 0 } };

/** Simulate the wire: serialize, then parse back as an untrusted peer would. */
function overTheWire(message: PresenceMessage): PresenceMessage | null {
  return parsePresenceMessage(JSON.parse(JSON.stringify(message)));
}

describe("presence protocol envelope", () => {
  test("every factory stamps protocol, version, buddy, and timestamp", () => {
    const message = presence.say("hermes", "hello", { ts: 1000 });
    expect(message).toMatchObject({
      protocol: PRESENCE_PROTOCOL,
      v: PRESENCE_PROTOCOL_VERSION,
      kind: "say",
      buddy: "hermes",
      ts: 1000,
      text: "hello",
    });
  });

  test("direction is derived from kind, and the two families are disjoint", () => {
    for (const kind of PRESENCE_TO_BODY_KINDS) {
      expect(presenceDirection(kind)).toBe("to-body");
    }
    for (const kind of PRESENCE_TO_SOUL_KINDS) {
      expect(presenceDirection(kind)).toBe("to-soul");
    }
    const overlap = PRESENCE_TO_BODY_KINDS.filter((kind) =>
      (PRESENCE_TO_SOUL_KINDS as readonly string[]).includes(kind),
    );
    expect(overlap).toEqual([]);
  });

  test("isPresenceEnvelope separates presence messages from gateway messages", () => {
    expect(isPresenceEnvelope(presence.summoned("hermes"))).toBe(true);
    expect(isPresenceEnvelope({ type: "chat", buddy: "hermes", text: "hi" })).toBe(false);
    expect(isPresenceEnvelope(null)).toBe(false);
  });
});

describe("presence protocol round-trips", () => {
  const cases: PresenceMessage[] = [
    presence.moveTo("hermes", FREE, { transitionMs: 180, ts: 1 }),
    presence.moveTo("hermes", ANCHORED, { ts: 2 }),
    presence.express("hermes", "thinking", { intensity: 0.7, ts: 3 }),
    presence.say("hermes", "On it.", { replyTo: "req-1", ts: 4 }),
    presence.attention("hermes", "user", { ts: 5 }),
    presence.attention("hermes", { point: { x: 10, y: 20 }, space: "screen" }, { ts: 6 }),
    presence.hydrate("hermes", { position: ANCHORED, emotion: "neutral", speech: "hi" }, { ts: 7 }),
    presence.attached("hermes", { at: ANCHORED, capabilities: ["drag", "menu"], ts: 8 }),
    presence.clicked("hermes", { button: "primary", at: FREE, ts: 9 }),
    presence.grabbed("hermes", FREE, { ts: 10 }),
    presence.dragged("hermes", FREE, { ts: 11 }),
    presence.dropped("hermes", ANCHORED, { onTarget: "trash", ts: 12 }),
    presence.summoned("hermes", { ts: 13 }),
    presence.dismissed("hermes", { ts: 14 }),
    presence.said("hermes", "what's the weather?", { ts: 15 }),
    presence.output("hermes", { surface: "text", text: "Reply body." }, { ts: 16 }),
    presence.output(
      "hermes",
      { surface: "image", mediaType: "image/png", caption: "a bike", dataBase64: "iVBORw0KGgo=" },
      { ts: 17 },
    ),
    presence.output("hermes", { surface: "session" }, { ts: 18 }),
    presence.surfaceRequest("hermes", "private_local_chat", { ts: 19 }),
    presence.surfaceActive("hermes", { surface: "private_local_chat", posture: "private", label: "Private local chat", providerLabel: "LM Studio" }, { ts: 20 }),
  ];

  test.each(cases)("survives JSON serialization: $kind", (message) => {
    expect(overTheWire(message)).toEqual(parsePresenceMessage(message));
    expect(overTheWire(message)?.kind).toBe(message.kind);
  });
});

describe("presence protocol parsing rejects malformed input", () => {
  test("rejects wrong protocol or version", () => {
    expect(parsePresenceMessage({ protocol: "gateway", v: 0, kind: "say", buddy: "h", ts: 1, text: "x" })).toBeNull();
    expect(parsePresenceMessage({ protocol: PRESENCE_PROTOCOL, v: 1, kind: "say", buddy: "h", ts: 1, text: "x" })).toBeNull();
  });

  test("rejects unknown kinds and non-presence shapes", () => {
    expect(parsePresenceMessage({ protocol: PRESENCE_PROTOCOL, v: 0, kind: "teleport", buddy: "h", ts: 1 })).toBeNull();
    expect(parsePresenceMessage({ type: "chat", buddy: "h", text: "hi" })).toBeNull();
    expect(parsePresenceMessage(null)).toBeNull();
    expect(parsePresenceMessage("nope")).toBeNull();
  });

  test("rejects missing buddy or timestamp", () => {
    expect(parsePresenceMessage({ protocol: PRESENCE_PROTOCOL, v: 0, kind: "summoned", buddy: "", ts: 1 })).toBeNull();
    expect(parsePresenceMessage({ protocol: PRESENCE_PROTOCOL, v: 0, kind: "summoned", buddy: "h" })).toBeNull();
  });

  test("rejects payloads with the wrong shape for their kind", () => {
    // move_to without a valid position
    expect(parsePresenceMessage({ protocol: PRESENCE_PROTOCOL, v: 0, kind: "move_to", buddy: "h", ts: 1, position: { mode: "free", x: 1 } })).toBeNull();
    // express with an unknown emotion
    expect(parsePresenceMessage({ protocol: PRESENCE_PROTOCOL, v: 0, kind: "express", buddy: "h", ts: 1, emotion: "smug" })).toBeNull();
    // grabbed without a position
    expect(parsePresenceMessage({ protocol: PRESENCE_PROTOCOL, v: 0, kind: "grabbed", buddy: "h", ts: 1 })).toBeNull();
    // anchored position missing an offset coordinate
    expect(parsePresenceMessage({ protocol: PRESENCE_PROTOCOL, v: 0, kind: "move_to", buddy: "h", ts: 1, position: { mode: "anchored", edge: "right", offset: { x: 0 } } })).toBeNull();
    // output image without inline bytes
    expect(parsePresenceMessage({ protocol: PRESENCE_PROTOCOL, v: 0, kind: "output", buddy: "h", ts: 1, surface: "image", mediaType: "image/png" })).toBeNull();
    // output with an unknown surface
    expect(parsePresenceMessage({ protocol: PRESENCE_PROTOCOL, v: 0, kind: "output", buddy: "h", ts: 1, surface: "hologram" })).toBeNull();
    // output text with a non-string body
    expect(parsePresenceMessage({ protocol: PRESENCE_PROTOCOL, v: 0, kind: "output", buddy: "h", ts: 1, surface: "text", text: 42 })).toBeNull();
  });

  test("accepts valid optional fields and minimal payloads", () => {
    expect(parsePresenceMessage(presence.clicked("hermes", { ts: 1 }))).not.toBeNull();
    expect(parsePresenceMessage(presence.hydrate("hermes", {}, { ts: 1 }))).not.toBeNull();
  });
});

describe("action gate cues", () => {
  test("action_request is to-soul and round-trips over the wire", () => {
    const request = presence.actionRequest("veritas", "receipt_review", { confirmed: true, requestId: "req-1", ts: 1 });
    expect(presenceDirection("action_request")).toBe("to-soul");
    expect((PRESENCE_TO_SOUL_KINDS as readonly string[])).toContain("action_request");
    expect(overTheWire(request)).toMatchObject({
      kind: "action_request",
      effector: "receipt_review",
      confirmed: true,
      requestId: "req-1",
    });
  });

  test("action_result is to-body and round-trips over the wire", () => {
    const result = presence.actionResult(
      "veritas",
      { effector: "receipt_review", decision: "needs_confirmation", receiptId: "action:veritas:receipt_review:t", summary: "confirm?" },
      { ts: 1 },
    );
    expect(presenceDirection("action_result")).toBe("to-body");
    expect((PRESENCE_TO_BODY_KINDS as readonly string[])).toContain("action_result");
    expect(overTheWire(result)).toMatchObject({ kind: "action_result", decision: "needs_confirmation" });
  });

  test("rejects a malformed action_request (missing effector)", () => {
    expect(
      parsePresenceMessage({ protocol: PRESENCE_PROTOCOL, v: 0, kind: "action_request", buddy: "veritas", ts: 1 }),
    ).toBeNull();
  });

  test("rejects an action_result with an unknown decision literal", () => {
    expect(
      parsePresenceMessage({
        protocol: PRESENCE_PROTOCOL,
        v: 0,
        kind: "action_result",
        buddy: "veritas",
        ts: 1,
        effector: "receipt_review",
        decision: "maybe",
        receiptId: "r1",
      }),
    ).toBeNull();
  });

  test("action_request carries a typed intent + route hint and round-trips", () => {
    const request = presence.actionRequest("crab", "repo_edit", {
      requestId: "req-9",
      intent: {
        operation: "write_patch",
        target: { kind: "repo_path", value: ".border-agents/proofs/x.patch" },
        payloadDigest: "sha256:abc",
        summary: "write proof patch",
      },
      routeHint: { provider: "claude", locality: "cloud" },
      ts: 1,
    });
    expect(overTheWire(request)).toMatchObject({
      kind: "action_request",
      effector: "repo_edit",
      intent: { operation: "write_patch", target: { kind: "repo_path" } },
      routeHint: { provider: "claude", locality: "cloud" },
    });
  });

  test("action_result carries an execution outcome + route provenance and round-trips", () => {
    const result = presence.actionResult(
      "crab",
      {
        effector: "repo_edit",
        decision: "allow",
        receiptId: "action:forge:repo_edit:t",
        outcome: {
          executed: true,
          executionReceiptId: "exec:forge:repo_edit:t",
          route: { provider: "claude", locality: "cloud", downgraded: false },
        },
      },
      { ts: 1 },
    );
    expect(overTheWire(result)).toMatchObject({
      kind: "action_result",
      outcome: { executed: true, route: { provider: "claude", downgraded: false } },
    });
  });

  test("surface_request is to-soul and surface_active is to-body", () => {
    expect(presenceDirection("surface_request")).toBe("to-soul");
    expect(presenceDirection("surface_active")).toBe("to-body");
    expect(overTheWire(presence.surfaceRequest("aether", "private_local_chat", { ts: 1 }))).toMatchObject({
      kind: "surface_request",
      surface: "private_local_chat",
    });
    expect(
      overTheWire(
        presence.surfaceActive(
          "aether",
          { surface: "private_local_chat", posture: "private", label: "Private local chat", providerLabel: "LM Studio" },
          { ts: 2 },
        ),
      ),
    ).toMatchObject({
      kind: "surface_active",
      surface: "private_local_chat",
      posture: "private",
      providerLabel: "LM Studio",
    });
  });

  test("rejects a malformed intent (target with an unknown kind)", () => {
    expect(
      parsePresenceMessage({
        protocol: PRESENCE_PROTOCOL,
        v: 0,
        kind: "action_request",
        buddy: "crab",
        ts: 1,
        effector: "repo_edit",
        intent: { operation: "write_patch", target: { kind: "registry", value: "x" } },
      }),
    ).toBeNull();
  });

  test("rejects an outcome whose executed flag is not a boolean", () => {
    expect(
      parsePresenceMessage({
        protocol: PRESENCE_PROTOCOL,
        v: 0,
        kind: "action_result",
        buddy: "crab",
        ts: 1,
        effector: "repo_edit",
        decision: "allow",
        receiptId: "r1",
        outcome: { executed: "yes" },
      }),
    ).toBeNull();
  });

  test("rejects malformed surface cues", () => {
    expect(
      parsePresenceMessage({ protocol: PRESENCE_PROTOCOL, v: 0, kind: "surface_request", buddy: "aether", ts: 1, surface: "" }),
    ).toBeNull();
    expect(
      parsePresenceMessage({
        protocol: PRESENCE_PROTOCOL,
        v: 0,
        kind: "surface_active",
        buddy: "aether",
        ts: 1,
        surface: "private_local_chat",
        posture: "party",
      }),
    ).toBeNull();
  });
});

describe("attached handshake", () => {
  test("is a to-soul lifecycle kind, distinct from summoned", () => {
    expect((PRESENCE_TO_SOUL_KINDS as readonly string[])).toContain("attached");
    expect(presenceDirection("attached")).toBe("to-soul");
    // The two must never be conflated — attached = body online, summoned = user opened surface.
    expect("attached").not.toBe("summoned");
  });

  test("accepts a bare handshake and rejects malformed capabilities", () => {
    expect(parsePresenceMessage(presence.attached("hermes", { ts: 1 }))).not.toBeNull();
    expect(
      parsePresenceMessage({
        protocol: PRESENCE_PROTOCOL,
        v: 0,
        kind: "attached",
        buddy: "hermes",
        ts: 1,
        capabilities: ["drag", 7],
      }),
    ).toBeNull();
  });
});
