import { describe, expect, it } from "vitest";
import {
  BUDDY_MANIFEST,
  BUDDY_MANIFEST_ORDER,
  CAPABILITY_LABELS,
  CAPABILITY_ORDER,
  EFFECTOR_SPECS,
  ROUTE_PROVIDER_LABELS,
  currentRouteLabel,
  effectorsFor,
  isWired,
  manifestEntry,
  validateBuddyManifest,
  type CapabilityGroup,
  type EffectorId,
  type RouteProvider,
} from "../buddyManifest";

describe("buddy manifest", () => {
  it("passes its own static invariants", () => {
    expect(() => validateBuddyManifest()).not.toThrow();
  });

  it("ships every effector stubbed — nothing live in the body layer", () => {
    for (const spec of Object.values(EFFECTOR_SPECS)) {
      expect(spec.wired, `effector ${spec.id} must stay unwired`).toBe(false);
      expect(isWired(spec.id)).toBe(false);
    }
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
