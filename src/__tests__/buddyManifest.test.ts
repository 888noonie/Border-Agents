import { describe, expect, it } from "vitest";
import {
  BUDDY_MANIFEST,
  BUDDY_MANIFEST_ORDER,
  CAPABILITY_LABELS,
  CAPABILITY_ORDER,
  EFFECTOR_SPECS,
  GATED_WIRED_EFFECTORS,
  ROUTE_PROVIDER_LABELS,
  currentRouteLabel,
  effectorsFor,
  isWired,
  manifestEntry,
  resolveManifestId,
  validateBuddyManifest,
  type CapabilityGroup,
  type EffectorId,
  type RouteProvider,
} from "../buddyManifest";

describe("buddy manifest", () => {
  it("passes its own static invariants", () => {
    expect(() => validateBuddyManifest()).not.toThrow();
  });

  it("ships every effector stubbed except gated read-only effectors", () => {
    for (const spec of Object.values(EFFECTOR_SPECS)) {
      if (GATED_WIRED_EFFECTORS.has(spec.id)) {
        continue;
      }
      expect(spec.wired, `effector ${spec.id} must stay unwired`).toBe(false);
      expect(isWired(spec.id)).toBe(false);
    }
  });

  it("only receipt_review is gated live, and only read-only effectors may be gated", () => {
    expect(EFFECTOR_SPECS.receipt_review.wired).toBe(true);
    expect(isWired("receipt_review")).toBe(true);
    expect([...GATED_WIRED_EFFECTORS]).toEqual(["receipt_review"]);
    for (const id of GATED_WIRED_EFFECTORS) {
      expect(EFFECTOR_SPECS[id].kind, `gated effector ${id} must be reach`).toBe("reach");
    }
  });

  it("resolves persona ids to governance ids, and leaves governance/unknown ids untouched", () => {
    // Persona aliases declared on the entries (the dock/body speak these).
    expect(resolveManifestId("owl")).toBe("veritas");
    expect(resolveManifestId("crab")).toBe("forge");
    expect(resolveManifestId("fox")).toBe("nexus");
    // Every declared persona resolves to the entry that declared it.
    for (const entry of Object.values(BUDDY_MANIFEST)) {
      if (entry.persona) {
        expect(resolveManifestId(entry.persona)).toBe(entry.id);
      }
    }
    // Governance ids and unknown ids resolve to themselves (callers may pass either form).
    expect(resolveManifestId("veritas")).toBe("veritas");
    expect(resolveManifestId("not-a-buddy")).toBe("not-a-buddy");
  });

  it("every act effector requires an explicit grant", () => {
    for (const spec of Object.values(EFFECTOR_SPECS)) {
      if (spec.kind === "act") {
        expect(spec.requiresGrant, `act effector ${spec.id} must require a grant`).toBe(true);
      }
    }
  });

  it("every granted effector resolves to a known spec", () => {
    for (const entry of Object.values(BUDDY_MANIFEST)) {
      for (const spec of effectorsFor(entry)) {
        expect(spec).toBeDefined();
        expect(spec.id).toBeTruthy();
      }
    }
  });

  it("routes only name known providers", () => {
    const known = new Set<RouteProvider>(Object.keys(ROUTE_PROVIDER_LABELS) as RouteProvider[]);
    for (const entry of Object.values(BUDDY_MANIFEST)) {
      const tiers = [entry.routes.primary, entry.routes.fallback ?? [], entry.routes.local ?? []];
      for (const tier of tiers) {
        for (const provider of tier) {
          expect(known.has(provider), `${entry.id} routes to unknown provider ${provider}`).toBe(true);
        }
      }
    }
  });

  it("reachFirst buddies expose a reach effector (reachable, not replace)", () => {
    for (const entry of Object.values(BUDDY_MANIFEST)) {
      if (entry.reachFirst) {
        expect(effectorsFor(entry).some((s) => s.kind === "reach")).toBe(true);
      }
    }
  });

  it("currentRouteLabel exposes only the role and top route, never effectors", () => {
    const label = currentRouteLabel(BUDDY_MANIFEST.forge);
    expect(label).toBe("Forge → Claude");
    expect(label).not.toMatch(/github|terminal|repo/i);
  });

  it("manifest order and capability order reference real entries", () => {
    for (const id of BUDDY_MANIFEST_ORDER) {
      expect(manifestEntry(id), `order lists unknown buddy ${id}`).toBeDefined();
    }
    for (const group of CAPABILITY_ORDER) {
      expect(CAPABILITY_LABELS[group as CapabilityGroup]).toBeTruthy();
    }
    // every capability group is ordered exactly once
    expect(new Set(CAPABILITY_ORDER).size).toBe(CAPABILITY_ORDER.length);
    expect(CAPABILITY_ORDER.length).toBe(Object.keys(CAPABILITY_LABELS).length);
  });

  it("connector buddy reaches the platforms the user already pays for", () => {
    const nexus = BUDDY_MANIFEST.nexus;
    const ids = new Set<EffectorId>(nexus.effectors);
    expect(ids.has("open_chatgpt")).toBe(true);
    expect(ids.has("open_claude")).toBe(true);
    expect(effectorsFor(nexus).every((s) => s.kind === "reach")).toBe(true);
  });
});
