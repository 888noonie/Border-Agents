import type { EffectorId, RouteProvider } from "./buddyManifest";
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
