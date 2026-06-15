import { describe, expect, it } from "vitest";
import { EFFECTOR_SPECS } from "../buddyManifest";
import { getSurface, isSurfaceId, SURFACE_ORDER, SURFACES, surfaceById } from "../surfaceManifest";

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
