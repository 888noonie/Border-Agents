# COMPASS Implementation Plan
## Border Agents · Desktop Buddy UI

---

## Credit Table

| Area | Credit | Entry |
|------|--------|-------|
| Vision & product language | Opus 4.8 | LIMEN |
| Implementation architecture | GPT 5.5 | Passport Halo |
| Surface/route ring mechanic | Deepseek 4.2 | Aether Ring |
| Governance receipt rail | Kimi 2.6 | Klein Vessel |
| Character warmth & scale | Grok 4.3 | Anchor |
| Vision routing discipline | Gemini 3.1 Pro | Lumina Core |
| Merge synthesis | Composer (Grok) | COMPASS |

---

## Ground Rules (from repo, not vibes)

- **Body**: pure-Rust tiny-skia software renderer, `wl_shm` pixel buffer, wlr-layer-shell/Wayland. No GPU. No compositor. No glassmorphism or backdrop-blur.
- **Trust channel that is already shipped**: `Emotion::for_decision()` in `render.rs` → `action_result.decision` → face expression. Face IS the trust primary. Tests already assert it.
- **Halo/glow/ring renders**: zero lines in `render.rs` today. They come after the passport stack, not before.
- **TORSO_W = 142.0px** — the density problem. Every persona/posture/route/output row must fit this column.
- **IPC**: WebSocket JSON, discriminated by `kind`. No auth. Soul is trusted peer.
- **Law 7 (AGENTS.md)**: Bodies present; souls act. Body never makes governance decisions.

---

## Current Wire Protocol — What Exists

### To-body events (soul → body)
| Event kind | Relevant fields |
|-----------|----------------|
| `express` | `emotion: string` (required), `intensity?`, `pose?` |
| `surface_active` | `surface`, `posture`, `label?`, `providerLabel?` |
| `action_result` | `effector`, `decision` (allow/needs_confirmation/blocked), `receiptId`, `summary?`, `outcome?` |
| `hydrate` | `position?`, `emotion?`, `speech?` |
| `output` | `surface`, `text?`, `caption?`, `media_type?`, `data_base64?` |

### Protocol gaps (what the passport torso needs that doesn't exist yet)
```ts
// Missing from express:
alertLevel?: "quiet" | "ready" | "confirm" | "blocked" | "critical";

// Missing from surface_active:
locality?: "local" | "cloud";

// Missing everywhere:
effectors?: { id: string; granted: boolean }[];  // Slice 5 only
```

---

## Slice 1 — Passport Torso (tests-first)
**Credit: GPT 5.5 Passport Halo (layout) + LIMEN (route chip semantics)**

**Goal**: Fix the 142px overflow visible in screenshots. Replace the current freeform `SessionCard` with a fixed-row passport ledger. Face carries trust (already works). No halo yet.

### 1a · Protocol delta — TS types first

**File**: `src/presenceProtocol.ts`

Add to `PresenceExpress`:
```ts
alertLevel?: "quiet" | "ready" | "confirm" | "blocked" | "critical";
```

Add to `PresenceSurfaceActive`:
```ts
locality?: "local" | "cloud";
```

These are additive, non-breaking. Existing body ignores unknown fields.

### 1b · Failing tests (write before any render code)

**File**: `src/__tests__/presenceProtocol.test.ts`
- Assert `alertLevel` is a valid optional field on `express` events
- Assert `locality` is a valid optional field on `surface_active` events
- Assert that `alertLevel: "blocked"` round-trips through the protocol codec

**File**: `src/__tests__/surfaceManifest.test.ts`
- Assert that surfaces without `effectorId` are classified as "unwired"
- Assert `SURFACE_ORDER` matches `SURFACES` map order

**File**: `src/__tests__/soulActions.test.ts`
- Assert `ActionDecision.allow` maps to `alertLevel: "ready"` (or quiet if no output pending)
- Assert `ActionDecision.needs_confirmation` maps to `alertLevel: "confirm"`
- Assert `ActionDecision.blocked` maps to `alertLevel: "blocked"`

All three test files should fail (or not compile) before 1c begins. That is the contract.

### 1c · Soul pushes new fields

**File**: `scripts/soul-server.ts`

When pushing `express` after an `action_result`, include `alertLevel` mapped from `decision`:
```ts
const alertLevel = {
  allow: "ready",
  needs_confirmation: "confirm",
  blocked: "blocked",
}[decision] ?? "quiet";
// push: { kind: "express", emotion, alertLevel }
```

When pushing `surface_active`, include `locality` from the active route:
```ts
// push: { kind: "surface_active", surface, posture, label, providerLabel, locality }
```

### 1d · Rust body — PassportCard render

**File**: `desktop-body/src/render.rs`

**New struct** `PassportCard` (analogous to existing `SessionCard`):
```rust
pub struct PassportCard<'a> {
    pub persona_label: &'a str,   // surface label from surface_active.label
    pub posture: &'a str,         // "work" | "play" | "private"
    pub provider: Option<&'a str>,// providerLabel from surface_active
    pub locality: Option<&'a str>,// "local" | "cloud" — new field
    pub output_preview: Option<&'a str>, // first line of last output, if any
}
```

**Row layout** (all within TORSO_W = 142px, 8px side padding, 126px usable):
```
┌─ row 0: persona · posture ──────────────┐  ~14px tall
│ [persona_label]          [posture tag]  │
├─ row 1: route chip ─────────────────────┤  ~12px tall
│ [provider] · [locality dot]             │
├─ divider ───────────────────────────────┤
│ [output_preview or session placeholder] │  remaining height
└─────────────────────────────────────────┘
```

- Posture tag: small rounded rect, 3-color: work=steel, play=amber, private=indigo
- Locality dot: ●local (green) / ●cloud (blue) / none if unknown
- Provider name: truncated at ~80px with ellipsis if needed
- Font sizes: row 0 = 11px bold, row 1 = 10px, output = 10px regular

**Add `TorsoOutput::Passport(PassportCard<'a>)` variant** and dispatch in `draw_torso_output()`.

**Update `BodyView`** (or wherever `TorsoOutput` is assembled from presence state) to emit `Passport` when `surface_active` has been received with at least `posture`.

### 1e · Presence state bridge

**File**: `desktop-body/src/presence.rs`

The `Cue::SurfaceActive` handler should populate new `locality` field on whatever body state struct holds it, and trigger a `Passport` torso re-render.

---

## Slice 2 — Surface Switcher
**Credit: LIMEN (hold-to-bloom dial, full words, unwired=dim) + Deepseek 4.2 (inner ring cycling)**

**Goal**: Replace static N/E/S/W perimeter letters with a semantic surface dial.

### What exists today
`PerimeterId`: `ArrowN`, `ArrowE`, `ArrowS`, `ArrowW`, `Quick0–3`, `Add`, `Paste`, `Review`, `Edit`.
`surface_request` to-soul event already exists and is wired.

### Changes
- **Hold gesture** on body (250ms threshold): bloom a radial dial showing all 6 surface labels at clock positions, full text, `SURFACE_ORDER` sequence
- **Unwired surfaces** (no `effectorId` in `surfaceManifest`): render dim, not hidden; tap shows tooltip "not wired yet"
- **Active surface** at 12 o'clock; previous/next at 10 and 2
- **Tap on arc segment**: emit `surface_request { surface }` to soul
- `ArrowN/E/S/W` perimeter controls remain as fallback cycle (tap = next in `SURFACE_ORDER`)
- `Quick0–3` may be promoted to pinned surface shortcuts (Customize surface)

### Protocol additions for Slice 2
None. `surface_request` and `surface_active` already exist.

---

## Slice 3 — Route Discipline
**Credit: Deepseek 4.2 (split ring, outer=route/health) + GPT 5.5 (route state types)**

**Goal**: Make provider, locality, and route health visible at a glance without the passport ledger needing to carry it all.

### Protocol additions
```ts
// Extend surface_active:
route?: {
  label: string;
  locality: "local" | "cloud";
  health: "ready" | "degraded" | "unavailable";
};
```

Note: today `route` only exists on `PresenceActionOutcome` (inside `action_result.outcome`), not on `surface_active`. This addition mirrors it into the surface_active context.

### Render additions
- Thin outer pixel ring around figure boundary: green=ready, amber=degraded, red=unavailable
- Local→cloud downgrade: flash amber border on `SurfaceActive` where `locality` changed from local to cloud; hold for 2s then settle
- Route chip in passport ledger (Slice 1 row 1) promotes `health` via color: row background tint when degraded

---

## Slice 4 — Expanded Mode + Governance Rail
**Credit: Kimi 2.6 (receipt mini-cards, left rail) + LIMEN (governance table semantics)**

**Goal**: When body_len is at BODY_LEN_MAX, show a left-edge receipt rail.

### Receipt mini-card format (Kimi)
```
✅  repo_edit · Forge · 14:32:05
⏳  voice_out · pending · 14:33:01
❌  web_search · blocked · 14:33:44
```

- Width: 160px left panel, body shifts right
- Cards: 28px tall, icon + effector + surface + time
- Tap on card: expand to full receipt detail
- Receipts sourced from `action_result` events, accumulated in body state (last 20)

### Inline confirm bar (no popups)
When `decision === "needs_confirmation"`, left arm button activates in-torso bar:
```
[Confirm: repo_edit in /src?] [✓] [✗]
```
Approve emits `action_request { effector, confirmed: true, requestId }`.

---

## Slice 5 — Effectors + Vision/Voice/Framing
**Credit: GPT 5.5 (vision shutter + host launch) + Gemini (attach-first) + Grok (ear/eye honesty)**

**Goal**: Make eyes/ears/mouth honest about when they're active; add vision shutter and host launch flow.

### Protocol additions
```ts
// On hydrate or surface_active:
effectors?: { id: string; granted: boolean }[];
```

### Render
- Eyes: open (white+pupil) only when `voice_in` or `image_gen` granted; closed otherwise (current default)
- Ears: small outer nub only when `voice_in` active; absent otherwise
- Mouth: closed/neutral unless `voice_out` granted
- Vision shutter: attach-first UI (Gemini rule) — drag file/screenshot onto body, THEN route dialog appears

### Host launch flow (GPT)
- `/codex` surface → torso shows preview card with estimated scope
- Confirm → `action_request { effector: "terminal", intent: "launch_codex" }` → receipt
- Body shows amber pulse until receipt arrives (already `needs_confirmation` → Alert face)

### `TargetBounds` window framing (COMPASS addition)
`TargetAcquired` / `TargetMoved` events already in presence.rs. In Expanded mode, arms visually bracket the target window bounds (pixel lines from arm endpoints to target rect corners).

---

## Out of Scope (v1)

- Multi-buddy roster
- Silent screen read or autonomous click (violates law 7)
- Auto provider fallback without visible confirm
- Full API key vault in docked torso
- Glassmorphism / backdrop-blur / GPU compositing — documented as impossible on this stack (see `docs/OVERLAY_POSTMORTEM_AND_REBUILD_PLAN.md`)
- Full crystal/capsule abstraction (Kimi/Grok/Gemini visual direction) — loses the figure

---

## Build Order Summary

| Slice | First deliverable | Tests gate |
|-------|------------------|-----------|
| 1a–b | Protocol types + failing tests | TS typecheck + test run |
| 1c | Soul pushes alertLevel + locality | Tests go green |
| 1d–e | Rust passport torso renders | Visual smoke on COSMIC body |
| 2 | Surface dial + unwired dim | surface_request round-trip test |
| 3 | Route chip health + outer ring | protocol type test |
| 4 | Receipt rail + inline confirm | receipt accumulation test |
| 5 | Eyes/ears/mouth + vision shutter | effectors[] round-trip test |

Each slice ships independently. Each leaves the body in a working, honest state.

---

## What the Face Already Does (Do Not Duplicate)

`Emotion::for_decision()` in `render.rs` is already real, tested, and correct:
- `allow` → `Emotion::Happy`
- `needs_confirmation` → `Emotion::Curious`
- `blocked` / unknown → `Emotion::Alert` (fails loud on garbage input)

The halo reinforces this later. The face leads. Do not build the halo before Slice 3 route ring — and even then, keep it as a secondary painted ring, not the primary trust channel.

---

*COMPASS · Implementation Plan · Border Agents desktop buddy*
*One keeper · many souls · every action leaves a light*
