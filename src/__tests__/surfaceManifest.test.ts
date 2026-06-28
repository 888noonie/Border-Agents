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

  it("maps the CLI-agent surfaces to wired reach launcher effectors", () => {
    expect(surfaceById.claude_code.effectorId).toBe("open_claude_code");
    expect(surfaceById.agent_zero.effectorId).toBe("open_agent_zero");
    for (const id of ["open_claude_code", "open_agent_zero"] as const) {
      expect(EFFECTOR_SPECS[id].kind, `${id} must be reach`).toBe("reach");
      expect(EFFECTOR_SPECS[id].wired, `${id} must be wired live`).toBe(true);
    }
  });

  it("keeps live_hermes a known-but-unwired placeholder", () => {
    const effectorId = surfaceById.live_hermes.effectorId;
    expect(effectorId).toBeTruthy();
    expect(EFFECTOR_SPECS[effectorId!].wired, "live_hermes should block as unwired").toBe(false);
  });
});

describe("surface availability taxonomy", () => {
  it("classifies presentational surfaces as available (no effector needed)", () => {
    expect(surfaceAvailability("session")).toBe("available");
    expect(surfaceAvailability("customize")).toBe("available");
  });

  it("classifies a wired-effector surface as gated (reachable, needs soul authorization)", () => {
    expect(EFFECTOR_SPECS.local_chat.wired).toBe(true);
    for (const id of ["private_local_chat", "claude_code", "agent_zero"] as const) {
      expect(surfaceAvailability(id)).toBe("gated");
    }
  });

  it("classifies the remaining known-but-unwired placeholder as unwired", () => {
    expect(surfaceAvailability("live_hermes")).toBe("unwired");
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
    expect(byId.claude_code.availability).toBe("gated");
    for (const entry of surfaceHydrationList()) {
      expect(entry.label).toBe(surfaceById[entry.id].label);
      expect(entry.availability).toBe(surfaceAvailability(entry.id));
    }
  });

  it("tags a launcher-backed surface with kind:launcher and its effector", () => {
    const byId = Object.fromEntries(surfaceHydrationList().map((s) => [s.id, s]));
    expect(byId.claude_code).toMatchObject({ kind: "launcher", effector: "open_claude_code" });
    expect(byId.agent_zero).toMatchObject({ kind: "launcher", effector: "open_agent_zero" });
    // A plain surface carries no launcher fields, so older bodies still parse it as a surface.
    expect(byId.session.kind).toBeUndefined();
    expect(byId.session.effector).toBeUndefined();
    // private_local_chat is gated but NOT a launcher (its effector opens no external tool).
    expect(byId.private_local_chat.kind).toBeUndefined();
  });
});
