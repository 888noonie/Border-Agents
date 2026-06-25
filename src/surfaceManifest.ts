import { EFFECTOR_SPECS, type EffectorId, type RouteProvider } from "./buddyManifest";
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
  { id: "claude_code", label: "Claude Code", effectorId: "summarize_long", provider: "codex", posture: "work" },
  { id: "live_hermes", label: "Live Hermes", effectorId: "voice_out", provider: "grok", posture: "play" },
  { id: "agent_zero", label: "Agent Zero", effectorId: "summarize_long", provider: "custom", posture: "work" },
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
