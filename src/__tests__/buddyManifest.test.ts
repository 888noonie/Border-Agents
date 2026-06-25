import { describe, expect, it } from "vitest";
import {
  BUDDY_MANIFEST,
  BUDDY_MANIFEST_ORDER,
  CAPABILITY_LABELS,
  CAPABILITY_ORDER,
  EFFECTOR_SPECS,
  GATED_WIRED_EFFECTORS,
  GATED_WIRED_REACH_EFFECTORS,
  GATED_WIRED_ACT_EFFECTORS,
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

  it("ships every effector stubbed except gated live effectors", () => {
    for (const spec of Object.values(EFFECTOR_SPECS)) {
      if (GATED_WIRED_EFFECTORS.has(spec.id)) {
        continue;
      }
      expect(spec.wired, `effector ${spec.id} must stay unwired`).toBe(false);
      expect(isWired(spec.id)).toBe(false);
    }
  });

  it("gates receipt_review, local_chat, and the launchers on the reach lane", () => {
    expect(EFFECTOR_SPECS.receipt_review.wired).toBe(true);
    expect(isWired("receipt_review")).toBe(true);
    expect(EFFECTOR_SPECS.local_chat.wired).toBe(true);
    expect(EFFECTOR_SPECS.local_chat.kind).toBe("reach");
    expect(EFFECTOR_SPECS.local_chat.requiresGrant).toBe(true);
    expect(isWired("local_chat")).toBe(true);
    // Launchers — open a tool the user already has, detached. Reach lane only.
    for (const id of ["open_vscode", "open_cursor", "open_terminal"] as const) {
      expect(EFFECTOR_SPECS[id].wired, `${id} must be wired live`).toBe(true);
      expect(EFFECTOR_SPECS[id].kind, `${id} must be a reach effector`).toBe("reach");
      expect(isWired(id)).toBe(true);
    }
    expect([...GATED_WIRED_REACH_EFFECTORS]).toEqual([
      "receipt_review",
      "local_chat",
      "open_vscode",
      "open_cursor",
      "open_terminal",
    ]);
    // The reach lane never acts in place of the tool — an act effector can never enter it.
    for (const id of GATED_WIRED_REACH_EFFECTORS) {
      expect(EFFECTOR_SPECS[id].kind, `reach-lane effector ${id} must be reach`).toBe("reach");
    }
  });

  it("gates repo_edit on the stricter act lane, never the reach lane", () => {
    expect(EFFECTOR_SPECS.repo_edit.wired).toBe(true);
    expect(isWired("repo_edit")).toBe(true);
    expect(GATED_WIRED_ACT_EFFECTORS.map((e) => e.id)).toEqual(["repo_edit"]);
    // Every act-lane entry is an act effector declaring the stricter membrane guarantees,
    // and never also sits in the reach lane.
    for (const entry of GATED_WIRED_ACT_EFFECTORS) {
      expect(EFFECTOR_SPECS[entry.id].kind, `act-lane effector ${entry.id} must be act`).toBe("act");
      expect(entry.requiresIntentSchema).toBe(true);
      expect(entry.requiresOutcomeReceipt).toBe(true);
      expect(GATED_WIRED_REACH_EFFECTORS.has(entry.id)).toBe(false);
    }
  });

  it("the union of both lanes is exactly the wired effectors", () => {
    const wired = Object.values(EFFECTOR_SPECS).filter((s) => s.wired).map((s) => s.id).sort();
    expect([...GATED_WIRED_EFFECTORS].sort()).toEqual(wired);
    expect([...GATED_WIRED_EFFECTORS].sort()).toEqual(
      ["local_chat", "receipt_review", "repo_edit", "open_vscode", "open_cursor", "open_terminal"].sort(),
    );
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

  it("grants local_chat to aether for the private local demo", () => {
    const aether = BUDDY_MANIFEST.aether;
    expect(aether.routes.local).toContain("lm_studio");
    expect(aether.effectors).toContain("local_chat");
  });
});
