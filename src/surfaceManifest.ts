import { EFFECTOR_SPECS, LAUNCHER_REACH_EFFECTORS, type EffectorId, type RouteProvider } from "./buddyManifest";
import type { UserPosture } from "./core";

export type SurfaceId =
  | "session"
  | "private_local_chat"
  | "claude_code"
  | "live_hermes"
  | "agent_zero"
  | "customize";

export interface SurfaceSpec {
  id: SurfaceId;
  label: string;
  effectorId?: EffectorId;
  provider?: RouteProvider;
  posture: UserPosture;
}

export const SURFACES: readonly SurfaceSpec[] = [
  { id: "session", label: "Session", posture: "work" },
  {
    id: "private_local_chat",
    label: "Private local chat",
    effectorId: "local_chat",
    provider: "lm_studio",
    posture: "private",
  },
  { id: "claude_code", label: "Claude Code", effectorId: "open_claude_code", provider: "claude", posture: "work" },
  { id: "live_hermes", label: "Live Hermes", effectorId: "voice_out", provider: "grok", posture: "play" },
  { id: "agent_zero", label: "Agent Zero", effectorId: "open_agent_zero", provider: "custom", posture: "work" },
  { id: "customize", label: "Customize", posture: "work" },
];

export const SURFACE_ORDER: readonly SurfaceId[] = SURFACES.map((surface) => surface.id);

export const surfaceById: Readonly<Record<SurfaceId, SurfaceSpec>> = Object.freeze(
  Object.fromEntries(SURFACES.map((surface) => [surface.id, surface])) as Record<SurfaceId, SurfaceSpec>,
);

export function isSurfaceId(value: unknown): value is SurfaceId {
  return typeof value === "string" && Object.prototype.hasOwnProperty.call(surfaceById, value);
}

export function getSurface(value: string): SurfaceSpec | undefined {
  return isSurfaceId(value) ? surfaceById[value] : undefined;
}

/**
 * How a surface stands relative to its backing effector — the single taxonomy the soul uses
 * to decide how a dock/body should present it (Slice 2 dims `unwired`). It deliberately
 * separates the three states the old "no effectorId = unwired" test conflated:
 *
 *   available — presentational, needs no effector (session, customize)
 *   unwired   — names a real effector that hasn't been wired yet (placeholders)
 *   gated     — wired effector, reachable but still needs soul authorization at act time
 *
 * This lives in the TS manifest, consumed by the soul; the native body never imports it —
 * if the body needs availability it must arrive soul-pushed on the wire (preserves the
 * manifest-free protocol). An unknown surface id degrades to `available` rather than throwing.
 */
export type SurfaceAvailability = "available" | "unwired" | "gated";

export function surfaceAvailability(value: string): SurfaceAvailability {
  const surface = getSurface(value);
  if (!surface || surface.effectorId === undefined) {
    return "available";
  }
  const spec = EFFECTOR_SPECS[surface.effectorId];
  if (!spec || spec.wired === false) {
    return "unwired";
  }
  return "gated";
}

export interface SurfaceDescriptor {
  id: SurfaceId;
  label: string;
  availability: SurfaceAvailability;
  /**
   * `surface` (default) cycles/switches the active surface; `launcher` opens an external tool
   * via a reach `action_request` for `effector`. A canonical surface backed by a launcher reach
   * effector (e.g. claude_code → open_claude_code) hydrates as a launcher so a tap spawns the
   * tool instead of switching surface. Absent `kind` means `surface` (older snapshots stay valid).
   */
  kind?: "surface" | "launcher";
  effector?: EffectorId;
}

/** True when a surface's backing effector is a launcher reach effector (opens a real tool). */
export function isLauncherSurface(surface: SurfaceSpec): boolean {
  return surface.effectorId !== undefined && LAUNCHER_REACH_EFFECTORS.has(surface.effectorId);
}

/**
 * The ordered surface list (canonical SURFACE_ORDER) with each surface's availability —
 * what the soul ships on `hydrate` so a manifest-free body can cycle and dim surfaces
 * without importing this module. A launcher-backed surface additionally carries
 * `kind:"launcher"` + its `effector`, so the body opens the tool rather than switching to it.
 */
export function surfaceHydrationList(): SurfaceDescriptor[] {
  return SURFACES.map((surface) => {
    const descriptor: SurfaceDescriptor = {
      id: surface.id,
      label: surface.label,
      availability: surfaceAvailability(surface.id),
    };
    if (isLauncherSurface(surface)) {
      descriptor.kind = "launcher";
      descriptor.effector = surface.effectorId;
    }
    return descriptor;
  });
}
