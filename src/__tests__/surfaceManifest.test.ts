import { describe, expect, it } from "vitest";
import { EFFECTOR_SPECS } from "../buddyManifest";
import {
  getSurface,
  isSurfaceId,
  surfaceAvailability,
  surfaceHydrationList,
  SURFACE_ORDER,
  SURFACES,
  surfaceById,
} from "../surfaceManifest";

describe("surface manifest", () => {
  it("orders real and placeholder surfaces from declarative data", () => {
    expect(SURFACE_ORDER).toEqual(SURFACES.map((surface) => surface.id));
    expect(surfaceById.session.label).toBe("Session");
    expect(isSurfaceId("private_local_chat")).toBe(true);
    expect(isSurfaceId("not-a-surface")).toBe(false);
    expect(getSurface("not-a-surface")).toBeUndefined();
  });

  it("maps private local chat to a private local_chat reach effector", () => {
    const surface = surfaceById.private_local_chat;
    expect(surface).toMatchObject({
      effectorId: "local_chat",
      provider: "lm_studio",
      posture: "private",
    });
    expect(EFFECTOR_SPECS.local_chat.kind).toBe("reach");
    expect(EFFECTOR_SPECS.local_chat.wired).toBe(true);
  });

  it("maps placeholders to known but unwired effectors", () => {
    for (const id of ["claude_code", "live_hermes", "agent_zero"] as const) {
      const effectorId = surfaceById[id].effectorId;
      expect(effectorId).toBeTruthy();
      expect(EFFECTOR_SPECS[effectorId!]).toBeDefined();
      expect(EFFECTOR_SPECS[effectorId!].wired, `${id} should block as unwired`).toBe(false);
    }
  });
});

describe("surface availability taxonomy", () => {
  it("classifies presentational surfaces as available (no effector needed)", () => {
    expect(surfaceAvailability("session")).toBe("available");
    expect(surfaceAvailability("customize")).toBe("available");
  });

  it("classifies a wired-effector surface as gated (reachable, needs soul authorization)", () => {
    expect(EFFECTOR_SPECS.local_chat.wired).toBe(true);
    expect(surfaceAvailability("private_local_chat")).toBe("gated");
  });

  it("classifies known-but-unwired placeholder surfaces as unwired", () => {
    for (const id of ["claude_code", "live_hermes", "agent_zero"] as const) {
      expect(surfaceAvailability(id)).toBe("unwired");
    }
  });

  it("degrades an unknown surface id to available rather than throwing", () => {
    expect(surfaceAvailability("not-a-surface")).toBe("available");
  });
});

describe("surfaceHydrationList (the soul-pushed surface snapshot)", () => {
  it("emits one descriptor per surface, in canonical SURFACE_ORDER", () => {
    const list = surfaceHydrationList();
    expect(list.map((s) => s.id)).toEqual([...SURFACE_ORDER]);
  });

  it("carries each surface's label and computed availability", () => {
    const byId = Object.fromEntries(surfaceHydrationList().map((s) => [s.id, s]));
    expect(byId.session).toEqual({ id: "session", label: "Session", availability: "available" });
    expect(byId.private_local_chat.availability).toBe("gated");
    expect(byId.claude_code.availability).toBe("unwired");
    for (const entry of surfaceHydrationList()) {
      expect(entry.label).toBe(surfaceById[entry.id].label);
      expect(entry.availability).toBe(surfaceAvailability(entry.id));
    }
  });
});
