// Buddy manifest — the declarative capability / route / effector layer.
//
// This sits ALONGSIDE `buddyProfiles.ts` (which holds the visual identities the
// bodies render). A profile says what a buddy *looks like*; a manifest entry says
// what job it's *for*, which providers can power it, and which tools it may one day
// reach for. The Wizard marries the two during onboarding.
//
// Two laws govern this file:
//
//   1. Bodies present; souls act. Nothing here is a body capability. `routes` and
//      `effectors` describe the SOUL's reach. The body only ever sees `name`/`role`
//      and the *current* route label — never the effector grants.
//
//   2. Make powerful tools reachable, not replace them. Encoded as `EffectorKind`:
//      a `reach` effector opens / hands off to the real tool (the on-brand default);
//      an `act` effector performs the action directly and therefore needs a heavier
//      governance gate. New buddies should prefer `reach`.
//
// Everything here is DECLARATIVE DATA with STUBBED effectors: every `EffectorSpec`
// ships `wired: false` UNLESS it is in `GATED_WIRED_EFFECTORS` below. Live tools get
// switched on one at a time, behind the governance gate (src/core/actionGate.ts), during
// the governance slice — never by editing this file alone. The first live effector is
// `receipt_review`: read-only, `reach`, and only ever runs after the gate authorizes it.

// ---------------------------------------------------------------------------
// Capability groups — "what are you trying to do?", not "which subscription?"
// ---------------------------------------------------------------------------

export type CapabilityGroup =
  | "coding" // build & code
  | "writing" // draft & edit prose
  | "research" // check, verify, cite
  | "creative" // images, decks, packaged artifacts
  | "data" // spreadsheets, queries, charts
  | "local_private" // offline / on-device, nothing leaves the machine
  | "voice_device" // speech in/out, device control
  | "orchestration" // plan & coordinate work within a project
  | "connectors" // open & route to platforms, apps, accounts
  | "memory_recall" // your own notes, history, personal knowledge
  | "automation"; // recurring, triggered, repeatable workflows

export const CAPABILITY_LABELS: Record<CapabilityGroup, string> = {
  coding: "Coding & building",
  writing: "Writing & drafting",
  research: "Research & verification",
  creative: "Creative & visual",
  data: "Data & analysis",
  local_private: "Local & private",
  voice_device: "Voice & device",
  orchestration: "Project orchestration",
  connectors: "Subscriptions & connectors",
  memory_recall: "Memory & recall",
  automation: "Automation & workflows",
};

export const CAPABILITY_DESCRIPTIONS: Record<CapabilityGroup, string> = {
  coding: "Write, refactor, and ship code with repo and editor tools.",
  writing: "Draft, rewrite, and polish prose for any audience.",
  research: "Verify claims, open sources, and review receipts.",
  creative: "Generate and package images, docs, and slides.",
  data: "Reason over numbers — spreadsheets, queries, and charts.",
  local_private: "Stay on-device. Nothing leaves the machine.",
  voice_device: "Listen and speak; drive the device hands-free.",
  orchestration: "Plan multi-step work and coordinate the other buddies.",
  connectors: "Open and route to the apps, models, and accounts you already pay for.",
  memory_recall: "Recall your own notes, history, and saved knowledge.",
  automation: "Run recurring, triggered workflows on a schedule.",
};

export const CAPABILITY_ORDER: CapabilityGroup[] = [
  "coding",
  "writing",
  "research",
  "creative",
  "data",
  "memory_recall",
  "orchestration",
  "automation",
  "connectors",
  "voice_device",
  "local_private",
];

// ---------------------------------------------------------------------------
// Provider routes — the swappable "brain". Provider-neutral and ranked.
// ---------------------------------------------------------------------------

// The routing namespace is intentionally a superset of `buddyProfiles.BuddyProvider`:
// that type names a *connection/adapter*; this names a *brain route* the soul can
// pick per task (incl. `web` search and `gpt`/`gemini`, which aren't adapters today).
export type RouteProvider =
  | "claude"
  | "gpt"
  | "gemini"
  | "grok"
  | "codex"
  | "lm_studio"
  | "ollama"
  | "openrouter"
  | "web"
  | "custom";

export const ROUTE_PROVIDER_LABELS: Record<RouteProvider, string> = {
  claude: "Claude",
  gpt: "GPT",
  gemini: "Gemini",
  grok: "Grok",
  codex: "Codex",
  lm_studio: "LM Studio",
  ollama: "Ollama",
  openrouter: "OpenRouter",
  web: "Web search",
  custom: "Custom",
};

// Ranked tiers. The buddy shows its *current* route ("Forge → Claude"); the user
// never picks a subscription, only a job. `local` is the privacy/offline tier.
export type ProviderRoute = {
  primary: RouteProvider[];
  fallback?: RouteProvider[];
  local?: RouteProvider[];
};

// ---------------------------------------------------------------------------
// Effectors — governed tool grants. ALL stubbed (`wired: false`) for now.
// ---------------------------------------------------------------------------

// `reach`  → open / hand off to the real tool. The safe, on-brand default.
// `act`    → perform the action directly, in place of the tool. Heavier gate.
export type EffectorKind = "reach" | "act";

export type EffectorId =
  // connectors — reach: open the real surface the user already has
  | "open_chatgpt"
  | "open_claude"
  | "open_grok"
  | "open_lmstudio"
  | "local_chat"
  | "open_github"
  | "open_gmail"
  | "open_calendar"
  | "open_vscode"
  // research — reach: surface sources and receipts, don't fabricate them
  | "web_search"
  | "open_source"
  | "receipt_review"
  // creative — act: produce an artifact
  | "image_gen"
  | "doc_build"
  | "slides_build"
  | "file_export"
  // coding — act: touch the project
  | "terminal"
  | "repo_edit"
  // voice — reach/act split on the device
  | "voice_in"
  | "voice_out"
  // explain — act: transform text the user already has
  | "summarize_long";

export type EffectorSpec = {
  id: EffectorId;
  label: string;
  kind: EffectorKind;
  // STUB FLAG. Stays false until this effector is wired behind the assumptions gate
  // during the governance slice. Tests assert the whole registry is unwired today.
  wired: boolean;
  // Whether the user must explicitly grant it in the Wizard before the soul may use it.
  requiresGrant: boolean;
  // Plain, literal note on what the gate must enforce. No metaphor here — this is
  // the trust-critical edge of the manifest.
  governanceNote: string;
};

function reach(
  id: EffectorId,
  label: string,
  governanceNote: string,
  requiresGrant = false,
): EffectorSpec {
  return { id, label, kind: "reach", wired: false, requiresGrant, governanceNote };
}

function act(id: EffectorId, label: string, governanceNote: string): EffectorSpec {
  return { id, label, kind: "act", wired: false, requiresGrant: true, governanceNote };
}

export const EFFECTOR_SPECS: Record<EffectorId, EffectorSpec> = {
  open_chatgpt: reach("open_chatgpt", "Open ChatGPT", "Launch the user's existing ChatGPT; never auto-send without confirmation."),
  open_claude: reach("open_claude", "Open Claude", "Launch the user's existing Claude; never auto-send without confirmation."),
  open_grok: reach("open_grok", "Open Grok", "Launch the user's existing Grok; never auto-send without confirmation."),
  open_lmstudio: reach("open_lmstudio", "Open LM Studio", "Launch the local LM Studio app; stays on-device."),
  local_chat: { ...reach("local_chat", "Local chat", "Hands your message to a local model and brings the reply back — stays on-device, never leaves the machine.", true), wired: true },
  open_github: reach("open_github", "Open GitHub", "Open a GitHub view; read-only until repo write is separately granted.", true),
  open_gmail: reach("open_gmail", "Open Gmail", "Open Gmail; drafting only, never send without explicit confirmation.", true),
  open_calendar: reach("open_calendar", "Open Calendar", "Open the calendar; propose events, never create without confirmation.", true),
  open_vscode: reach("open_vscode", "Open in VS Code", "Open files/folders in the editor; no edits applied by this effector."),
  web_search: reach("web_search", "Web search", "Query the web and return sources; surface citations, do not assert unsourced facts."),
  open_source: reach("open_source", "Open source", "Open a cited source in the browser for the user to read."),
  // First live effector — read-only, gated. See GATED_WIRED_EFFECTORS below.
  receipt_review: { ...reach("receipt_review", "Review receipts", "Open the governance receipt ledger for inspection. Read-only."), wired: true },
  image_gen: act("image_gen", "Generate image", "Create an image artifact; label it AI-generated, keep it local until shared."),
  doc_build: act("doc_build", "Build document", "Assemble a document artifact; user reviews before any export or share."),
  slides_build: act("slides_build", "Build slides", "Assemble a slide deck; user reviews before any export or share."),
  file_export: act("file_export", "Export file", "Write an artifact to disk at a user-confirmed path. Confirm overwrite."),
  terminal: act("terminal", "Run terminal command", "Execute a shell command. Highest-risk effector: requires per-command confirmation."),
  // First true `act` effector behind the membrane — gated through GATED_WIRED_ACT_EFFECTORS.
  // The gate authorizes the EFFECT (typed intent + target), not just the grant; protected
  // targets (AGENTS.md, src/core, deps, .git) are hard-blocked even after confirmation.
  repo_edit: { ...act("repo_edit", "Edit repository", "Apply code changes via a typed ActionIntent; protected targets are hard-blocked; surface a reviewable diff before writing."), wired: true },
  voice_in: reach("voice_in", "Listen (voice)", "Capture microphone input on an explicit push-to-talk; no always-on listening.", true),
  voice_out: act("voice_out", "Speak (voice)", "Synthesize speech output. User controls volume and can mute instantly."),
  summarize_long: act("summarize_long", "Summarise long context", "Condense text the user already has; never invent content not in the source."),
};

// ---------------------------------------------------------------------------
// Buddy manifest entries — role + capability + routes + (stubbed) effector grants
// ---------------------------------------------------------------------------

export type BuddyManifestEntry = {
  schemaVersion: 1;
  id: string;
  name: string;
  role: string;
  capability: CapabilityGroup;
  routes: ProviderRoute;
  effectors: EffectorId[]; // grants — every id resolves to a stubbed EffectorSpec
  // Honor "reachable, not replace": when true, the entry must include at least one
  // `reach` effector (validated below). New buddies should set this true.
  reachFirst: boolean;
  // The presentation persona (character) id that wears this governance identity in the
  // dock/body (e.g. the "owl" character is the "veritas" governance buddy). The body emits
  // action requests under its persona id; the gate authorizes under the governance id.
  // `resolveManifestId` bridges the two. Omit when persona id === governance id.
  persona?: string;
};

export const BUDDY_MANIFEST: Record<string, BuddyManifestEntry> = {
  forge: {
    schemaVersion: 1,
    id: "forge",
    name: "Forge",
    role: "Build & Code",
    capability: "coding",
    routes: { primary: ["claude", "codex"], fallback: ["gpt"], local: ["lm_studio"] },
    effectors: ["open_github", "open_vscode", "repo_edit", "terminal"],
    reachFirst: true,
    persona: "crab",
  },
  veritas: {
    schemaVersion: 1,
    id: "veritas",
    name: "Veritas",
    role: "Check & Verify",
    capability: "research",
    routes: { primary: ["gpt", "grok", "web"], fallback: ["claude", "gemini"] },
    effectors: ["web_search", "open_source", "receipt_review"],
    reachFirst: true,
    persona: "owl",
  },
  nova: {
    schemaVersion: 1,
    id: "nova",
    name: "Nova",
    role: "Create & Package",
    capability: "creative",
    routes: { primary: ["gpt", "gemini"], fallback: ["claude"] },
    effectors: ["image_gen", "doc_build", "slides_build", "file_export"],
    reachFirst: false, // creation is inherently `act`; no reach effectors expected
  },
  nexus: {
    schemaVersion: 1,
    id: "nexus",
    name: "Nexus",
    role: "Connect & Route",
    capability: "connectors",
    routes: { primary: ["openrouter"], fallback: ["custom"] },
    effectors: [
      "open_chatgpt",
      "open_claude",
      "open_grok",
      "open_lmstudio",
      "open_github",
      "open_gmail",
      "open_calendar",
    ],
    reachFirst: true,
    persona: "fox",
  },
  aether: {
    schemaVersion: 1,
    id: "aether",
    name: "Aether",
    role: "Summarise & Explain",
    capability: "writing",
    routes: { primary: ["gpt"], fallback: ["claude"], local: ["lm_studio", "ollama"] },
    effectors: ["summarize_long", "voice_out", "local_chat"],
    reachFirst: false, // transforms text the user already has
  },
};

// Two gated-live lanes. An effector may ship `wired: true` ONLY through one of them;
// `validateBuddyManifest` throws on any wired effector outside both.
//
//   Reach lane — open/inspect the real tool, or a governed reach to a local,
//                on-device provider. Data stays on the machine; the buddy never acts
//                in place of the tool.
//   Act lane   — effectors that perform an effect in place of the tool. A `reach` rail is
//                not enough here, so this lane is STRICTER: each entry must declare a typed
//                intent schema and an execution-outcome receipt (and the gate/soul suites
//                prove the no-execute-on-block invariant). This is the only way an `act`
//                effector goes live — never by flipping a spec in place.
export const GATED_WIRED_REACH_EFFECTORS: ReadonlySet<EffectorId> = new Set<EffectorId>(["receipt_review", "local_chat"]);

export interface GatedActEffector {
  id: EffectorId;
  /** The effector must authorize a typed `ActionIntent` (target-level), not just the grant. */
  requiresIntentSchema: true;
  /** Running it must emit a separate ExecutionReceipt (the world-facing outcome + route). */
  requiresOutcomeReceipt: true;
}

export const GATED_WIRED_ACT_EFFECTORS: readonly GatedActEffector[] = [
  { id: "repo_edit", requiresIntentSchema: true, requiresOutcomeReceipt: true },
];

// The union of both lanes — every effector allowed to ship `wired: true`.
export const GATED_WIRED_EFFECTORS: ReadonlySet<EffectorId> = new Set<EffectorId>([
  ...GATED_WIRED_REACH_EFFECTORS,
  ...GATED_WIRED_ACT_EFFECTORS.map((entry) => entry.id),
]);

export const BUDDY_MANIFEST_ORDER: string[] = ["forge", "veritas", "nova", "nexus", "aether"];

// ---------------------------------------------------------------------------
// Helpers & invariants
// ---------------------------------------------------------------------------

export function manifestEntry(id: string): BuddyManifestEntry | undefined {
  return BUDDY_MANIFEST[id];
}

// Persona/character id (e.g. "owl") → governance manifest id (e.g. "veritas"). Built once
// from the `persona` fields on the manifest entries so the mapping has a single source of
// truth. A governance id, or any id with no persona alias, resolves to itself — so callers
// can pass either form. The action gate keys grants by governance id; the dock/body speak
// in persona ids, so every action request is resolved through here before authorization.
const PERSONA_TO_MANIFEST: Record<string, string> = Object.fromEntries(
  Object.values(BUDDY_MANIFEST)
    .filter((entry): entry is BuddyManifestEntry & { persona: string } => Boolean(entry.persona))
    .map((entry) => [entry.persona, entry.id]),
);

export function resolveManifestId(id: string): string {
  return PERSONA_TO_MANIFEST[id] ?? id;
}

export function effectorsFor(entry: BuddyManifestEntry): EffectorSpec[] {
  return entry.effectors.map((id) => EFFECTOR_SPECS[id]);
}

export function isWired(id: EffectorId): boolean {
  return EFFECTOR_SPECS[id].wired === true;
}

export function currentRouteLabel(entry: BuddyManifestEntry): string {
  // What the BODY is allowed to show: the role and its top primary route. Never the
  // effector grants. e.g. "Forge → Claude".
  const top = entry.routes.primary[0];
  return `${entry.name} → ${ROUTE_PROVIDER_LABELS[top]}`;
}

// Static guarantees the rest of the app (and the tests) can lean on. Throws on the
// first violation so a bad edit fails loudly rather than smuggling a live or
// dangling effector into the body layer.
export function validateBuddyManifest(): void {
  // No effector ships wired unless it has been explicitly gated through one of the two
  // lanes. Live tools are switched on behind the action gate, not by editing a spec in place.
  for (const spec of Object.values(EFFECTOR_SPECS)) {
    if (spec.wired && !GATED_WIRED_EFFECTORS.has(spec.id)) {
      throw new Error(`effector "${spec.id}" is wired but not gated; effectors stay stubbed until gated`);
    }
  }
  // Reach lane: read-only hand-offs only. An `act` effector can never be smuggled in here.
  for (const id of GATED_WIRED_REACH_EFFECTORS) {
    if (EFFECTOR_SPECS[id].kind !== "reach") {
      throw new Error(`gated reach effector "${id}" is not a reach effector; the reach lane is read-only`);
    }
  }
  // Act lane: must be `act`, must declare the stricter membrane guarantees, and may not also
  // sit in the reach lane. An act effector goes live ONLY through this stricter path.
  for (const entry of GATED_WIRED_ACT_EFFECTORS) {
    const spec = EFFECTOR_SPECS[entry.id];
    if (spec.kind !== "act") {
      throw new Error(`gated act effector "${entry.id}" is not an act effector`);
    }
    if (GATED_WIRED_REACH_EFFECTORS.has(entry.id)) {
      throw new Error(`effector "${entry.id}" cannot be in both gated lanes`);
    }
    if (!entry.requiresIntentSchema || !entry.requiresOutcomeReceipt) {
      throw new Error(`gated act effector "${entry.id}" must require a typed intent schema and an outcome receipt`);
    }
  }

  for (const entry of Object.values(BUDDY_MANIFEST)) {
    if (entry.routes.primary.length === 0) {
      throw new Error(`buddy "${entry.id}" has no primary route`);
    }
    for (const id of entry.effectors) {
      if (!(id in EFFECTOR_SPECS)) {
        throw new Error(`buddy "${entry.id}" grants unknown effector "${id}"`);
      }
    }
    // reachable-not-replace: a reachFirst buddy must offer a way to hand off to a
    // real tool, not only act in its place.
    if (entry.reachFirst) {
      const hasReach = effectorsFor(entry).some((spec) => spec.kind === "reach");
      if (!hasReach) {
        throw new Error(`buddy "${entry.id}" is reachFirst but grants no "reach" effector`);
      }
    }
  }
}
