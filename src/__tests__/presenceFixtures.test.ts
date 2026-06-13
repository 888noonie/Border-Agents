import { describe, expect, test } from "vitest";
import {
  PRESENCE_TO_BODY_KINDS,
  PRESENCE_TO_SOUL_KINDS,
  parsePresenceMessage,
} from "../presenceProtocol";
import { PRESENCE_FIXTURES, serializeFixtures } from "../presenceFixtures";
import committedFixture from "../../fixtures/presence-v0.json";

/**
 * The golden fixture is the cross-language contract: the Rust body reads the same
 * `fixtures/presence-v0.json` in `cargo test`. These tests guard it from two sides —
 * (1) the committed file must match what the TS factories produce *right now* (drift
 * guard — regenerate with `npm run gen:fixtures`), and (2) every kind in the protocol
 * must be represented and must parse.
 */
describe("presence golden fixtures", () => {
  test("committed fixture matches the factories (regenerate with npm run gen:fixtures)", () => {
    expect(committedFixture).toEqual(JSON.parse(serializeFixtures()));
  });

  test("covers every protocol kind exactly once", () => {
    const fixtureKinds = Object.keys(PRESENCE_FIXTURES).sort();
    const allKinds = [...PRESENCE_TO_BODY_KINDS, ...PRESENCE_TO_SOUL_KINDS].sort();
    expect(fixtureKinds).toEqual(allKinds);
  });

  test("every fixture is a well-formed presence message of its keyed kind", () => {
    for (const [kind, message] of Object.entries(PRESENCE_FIXTURES)) {
      const parsed = parsePresenceMessage(message);
      expect(parsed, `fixture "${kind}" should parse`).not.toBeNull();
      expect(parsed?.kind).toBe(kind);
    }
  });

  test("the committed fixture on disk parses for every kind", () => {
    for (const [kind, message] of Object.entries(committedFixture)) {
      const parsed = parsePresenceMessage(message);
      expect(parsed, `committed fixture "${kind}" should parse`).not.toBeNull();
    }
  });
});
