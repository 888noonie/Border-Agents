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

### Protocol gaps (reconciled with the roundtable audit — see frozen decisions below)
```ts
// On action_result (NOT express — face from decision, chrome from alertLevel, one cue):
alertLevel?: "quiet" | "ready" | "confirm" | "blocked" | "critical";

// On surface_active — one nested route object, NOT flat locality (no Slice 3 migration):
route?: { label: string; locality: "local" | "cloud"; health?: "ready" | "degraded" | "unavailable" };

// Missing everywhere:
effectors?: { id: string; granted: boolean }[];  // Slice 5 only
```

### Frozen decisions (GPT · Gemini · Composer · Grok audit, 2026-06-15)

Unanimous "Approve Slice 1, charter Slices 2–5." The reconciled contract:

1. **`alertLevel` rides on `action_result`, not `express`.** Face keeps deriving from `decision` (`Emotion::for_decision`); chrome derives from `alertLevel`. One event, one truth — no express/action_result ordering race (Composer Option A · Grok).
2. **`surface_active.route` is one nested object** carrying `label` + `locality` + optional `health`. No flat `locality` to migrate when the Slice 3 ring lands (Composer · Grok).
3. **`decisionAlertLevel()` lives in `src/soulActions.ts`** beside `decisionEmotion()`; garbage → `critical` (fail loud); no `allow → quiet` conditional (GPT · Grok).
4. **Surface availability is `available | unwired | gated`** as `surfaceAvailability()` in `surfaceManifest.ts`; `session`/`customize` are `available`, not unwired (GPT · all).
5. **Passport replaces `SessionCard` at idle only** — `Text`/`Image`/stub output paths are untouched (Grok).
6. **`presence.rs` hand-parses JSON** (not serde) — additive fields are ignored, never fatal; no `#[serde(default)]` needed (Grok correcting Gemini).
7. **Slice 2 splits** into 2a (arrow cycle + dim) and 2b (hold-to-bloom); body never reads `surfaceManifest.ts` — availability arrives soul-pushed (Grok · Composer).
8. **Phase C fields are frozen:** COMPASS additive fields must not break golden fixtures or the Forge Playwright specs. The browser proof has since landed: the Forge `/review repo_edit <path>` membrane now proves hard block, traversal block, no-action-backing block, and the action-backed permit twin.

---

## Slice 1 — Passport Torso (tests-first)
**Credit: GPT 5.5 Passport Halo (layout) + LIMEN (route chip semantics)**

**Goal**: Fix the 142px overflow visible in screenshots. Replace the current freeform `SessionCard` with a fixed-row passport ledger. Face carries trust (already works). No halo yet.

### 1a · Protocol delta — TS types ✅ LANDED (2026-06-15)

**File**: `src/presenceProtocol.ts`

- `PresenceAlertLevel` type + `PRESENCE_ALERT_LEVELS`; `alertLevel?` added to **`PresenceActionResult`** (not express), with `isAlertLevel` validator + factory wiring.
- `SurfaceRoute` type (`{ label; locality; health? }`); `route?` added to **`PresenceSurfaceActive`** (not flat `locality`), with `isSurfaceRoute` validator + factory wiring.

Additive, non-breaking — existing bodies ignore unknown fields.

### 1b · Tests pinning the contract ✅ LANDED (green)

**File**: `src/__tests__/presenceProtocol.test.ts`
- `alertLevel: "blocked"` round-trips on `action_result`; unknown level (`"loud"`) rejected.
- `route` (with optional `health`) round-trips on `surface_active`; bad `locality`/`health` rejected.

**File**: `src/__tests__/surfaceManifest.test.ts`
- `surfaceAvailability()` taxonomy: `session`/`customize` → `available`, `private_local_chat` → `gated`, placeholders → `unwired`, unknown → `available`.
- (The naive "no effectorId = unwired" test the original plan proposed was dropped — `surfaceManifest.test.ts` already pins the correct placeholder taxonomy.)

**File**: `src/__tests__/soulActions.test.ts`
- `decisionAlertLevel`: allow→ready, needs_confirmation→confirm, blocked→blocked, garbage→critical; lockstep block proves face+chrome derive from one decision.

Gate at this checkpoint: `npx tsc --noEmit` clean; full `npx vitest run` → **213 passed**.

### 1c · Soul pushes new fields ✅ LANDED (commit 425ae19)

**File**: `scripts/soul-server.ts`

On `action_result`, set `alertLevel = decisionAlertLevel(decision)` (imported from `src/soulActions.ts`):
```ts
// push: presence.actionResult(buddy, { effector, decision, receiptId, alertLevel })
```

On `surface_active`, build `route` from the active route (omit `health` until Slice 3):
```ts
// push: presence.surfaceActive(buddy, { surface, posture, label, providerLabel,
//   route: { label: providerLabel, locality } })
```

Gate at this checkpoint: `npx tsc --noEmit` clean; full `npx vitest run` → **214 passed**.

### 1d · Rust body — PassportCard render ✅ LANDED (commit 7b48312)

**Scope guard (Grok):** `PassportCard` supersedes `SessionCard` **at idle only**. The `Text` /
`Image` / `ImageStub` / `FileStub` output paths stay exactly as they are — `output_preview` is a
one-line idle peek, not a replacement for the full output cards. Keep `draw_clay_texture()` and the
current palette; the warm-LIMEN/glass chrome is a later slice. `locality` now comes from
`surface_active.route.locality` (the nested object), not a flat field.

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

**Added `TorsoOutput::Passport(PassportCard<'a>)` variant** and dispatch in `draw_torso_output()`.

**Updated `BodyView`** assembly to emit `Passport` for the idle/status surface state while keeping full output cards untouched.

### 1e · Presence state bridge ✅ LANDED (commit 7b48312)

**File**: `desktop-body/src/presence.rs`

The `Cue::SurfaceActive` handler populates the new `route` (label/locality) on body state and
triggers a `Passport` torso re-render. `presence.rs` **hand-parses** the
JSON `Value` — additive wire fields are silently ignored, never fatal (no `#[serde(default)]`
needed). `Cue::SurfaceActive` / `Cue::ActionResult` variants and parse arms now read the landed fields.

### 1f · Fixture + cross-language parity gate ✅ LANDED (commit 7b48312)

The new fields must round-trip the golden fixtures, or the Rust body and TS soul silently diverge:

```bash
npm run gen:fixtures          # regenerate fixtures/presence-v0.json with route + alertLevel
cd desktop-body && cargo test  # parses_surface_active_fixture etc. must stay green
```

Plus a `render.rs` layout regression test (pattern exists beside `Emotion::for_decision`) asserting
`PassportCard` rows fit within `TORSO_W = 142`. COSMIC visual smoke is human-only — don't block the
commit on it.

Gate at this checkpoint: `npm run gen:fixtures` clean; `cd desktop-body && cargo test` → **41 passed**.

### Slice 1 follow-ups (non-blocking)

- **Persona label truncation**: row 0 shares width between the persona label and posture tag, so long labels such as `Border Wizard` truncate to `Border W...`. Fix later in `draw_passport_card()` row-0 geometry by moving the posture tag to a corner lane or normalizing persona labels before truncation.
- **Empty idle speech bubble**: the blank pill can still render when `speech` is empty. The passport idle peek now carries the status text that justified the bubble, so a later pass should suppress empty idle speech bubbles.

---

## Slice 2 — Surface Switcher
**Credit: LIMEN (hold-to-bloom dial, full words, unwired=dim) + Deepseek 4.2 (inner ring cycling)**

**Goal**: Replace static N/E/S/W perimeter letters with a semantic surface dial.

### What exists today
`PerimeterId`: `ArrowN`, `ArrowE`, `ArrowS`, `ArrowW`, `Quick0–3`, `Add`, `Paste`, `Review`, `Edit`.
`surface_request` to-soul event already exists and is wired.

**Split into two PRs (Grok/Composer): 2a is low-risk; 2b is a new input subsystem.**

### 2a · Arrow cycle + availability dim ✅ LANDED (2026-06-15)
- `hydrate.surfaces[]` now carries the ordered surface descriptors from the soul:
  `{ id, label, availability: "available" | "unwired" | "gated" }`.
- `surfaceHydrationList()` in `src/surfaceManifest.ts` is the TS source of truth: canonical `SURFACE_ORDER`, surface labels, and `surfaceAvailability()` classification.
- `scripts/soul-server.ts` includes that list in the attach-time hydrate snapshot; the native body stays manifest-free.
- Rust parses the hydrate surface list with closed-set availability validation, stores it on body state, and cycles the stored order instead of the static `SURFACE_ORDER` mirror. The static Rust order is now only a pre-hydrate fallback seed.
- `ArrowN/E/S/W` skip `unwired` surfaces while cycling so they never dead-end on an unavailable surface.
- Quick buttons render dim when their surface is `unwired`. Tapping a dim quick button does not emit `surface_request`; it shows a "not wired yet" speech cue.

### 2b · Hold-to-bloom dial ✅ LANDED (2026-06-16)
- Hold gesture (250ms in place — new press-timing + segment hit-testing, distinct from the `CLICK_SLOP` drag→`grabbed` path): blooms a radial dial of all 6 surface labels at clock positions, full text, active at 12 o'clock, prev/next at 10 and 2.
- Tap a dial segment or drag-release onto one → `surface_request { surface }`. The dial builds on the stored hydrate surface list; the body stays manifest-free.
- Own Rust harness covers hold timing/stillness, active-at-12 rotation, six-slot clock geometry, and segment hit-testing. Gate at this checkpoint: `cd desktop-body && cargo test` → **50 passed**.

### Protocol additions for Slice 2
2a adds soul-pushed ordered `surfaces[]` on `hydrate`. `surface_request` already exists.

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

**Status (2026-06-25): Slices 1–4 + Phase C are DONE, Opus-audited, and LANDED on `main`** (PRs #6–#11 merged; `main` tree-identical to the former `slice-4` tip; `tsc` clean, vitest 222 green). **Slice 5 remains charter (design intent), not committed scope, until the next fork is explicitly chosen. Phase C protocol fields are frozen — COMPASS additive fields must not break golden fixtures or Playwright specs.**

**Update (2026-06-26):** On the presence/Step-4 track (not a COMPASS slice), the soul-gated
`commandeer` screen effector landed end to end (driver enumerate/activate/type ·
`targets_available`/`commandeer` wire · soul act gate · body two-phase P/M/C picker), plus a
body-local settings panel (Colour/Size editable; Posture/Buddy read-only). Counts moved with it:
vitest **236**, `desktop-body` cargo test **84**. See `docs/STEP4_WIRE_THE_SOUL_PLAN.md` §0c.

**Phase C browser proof (landed):** `e2e/governance-act-effector.spec.ts` proves the live Forge execution membrane for `repo_edit`: protected target blocked, traversal blocked, safe target without action-backing blocked, and the same safe target with action-backing flowing `needs_confirmation → Confirm → allow` with an execution receipt. Gate: full Playwright suite **12 passed**.

| Slice | Deliverable | Tests gate | Status |
|-------|------------|-----------|--------|
| 1a–b | `alertLevel` on action_result + `route` on surface_active; tests pin the contract | `tsc` clean · vitest 213 ✅ | **DONE** |
| 1c | Soul pushes `decisionAlertLevel` + `route` | `tsc` clean · vitest 214 ✅ | **DONE** |
| 1d–e | Rust passport torso (idle only; keep output paths) | `render.rs` layout regression test ✅ | **DONE** |
| 1f | `gen:fixtures` + `cargo test` parity | cargo 41 ✅ | **DONE** |
| Phase C | Forge `/review repo_edit <path>` browser proof | Playwright 12 ✅ | **DONE** |
| 2a | Arrow cycle + soul-pushed hydrate `surfaces[]` availability dim | `tsc` clean · vitest 217 ✅ · cargo 46 ✅ | **DONE** |
| 2b | Hold-to-bloom dial (separate input subsystem) | cargo 50 ✅ | **DONE** |
| 3 | `route.health` ring + `routeHealthFromSoul` soul derivation | `tsc` clean · vitest 222 ✅ · cargo 51 ✅ | **DONE** |
| 4 | Receipt rail — *extends* existing Review/Confirm, thin cues only | `tsc` clean · vitest 222 ✅ · cargo 57 ✅ | **DONE** |
| 5 | Eyes/ears/mouth + vision shutter + host launch (governance, not render) | effectors[] round-trip test | charter |

Each slice ships independently. Each leaves the body in a working, honest state.

---

## What the Face Already Does (Do Not Duplicate)

`Emotion::for_decision()` in `render.rs` is already real, tested, and correct:
- `allow` → `Emotion::Happy`
- `needs_confirmation` → `Emotion::Curious`
- `blocked` / unknown → `Emotion::Alert` (fails loud on garbage input)

The halo reinforces this later. The face leads. Do not build the halo before Slice 3 route ring — and even then, keep it as a secondary painted ring, not the primary trust channel.

---

# Appendix · Roundtable Audit Transcripts (2026-06-15)

Verbatim auditor passes that produced the Frozen Decisions above. Kept for provenance — the
binding amendments are already folded into the plan body; this appendix is the record, not the spec.

## GPT Audit

Approve the plan, with two small corrections before implementation.

The build order is right: **tests first → protocol types → soul pushes → Rust passport torso**. That keeps Slice 1 honest and avoids drifting into visual polish before the data contract exists.

I’d adjust these bits:

1. **Surface “unwired” wording needs tightening.**  
   `session` and `customize` have no `effectorId`, but they should not be treated the same as blocked/unwired placeholders. I’d define/test something like:

   `available`: no effector needed, or wired effector  
   `unwired`: effector exists but `EFFECTOR_SPECS[id].wired === false`  
   `gated`: wired effector, needs soul authorization

   So change the test from “surfaces without `effectorId` are classified as unwired” to “placeholder surfaces with known unwired effectors classify as unwired; session/customize remain presentational/available.”

2. **Add an exported alert helper next to `decisionEmotion`.**  
   Instead of embedding the mapping only in `scripts/soul-server.ts`, put it in `src/soulActions.ts`:

   ```ts
   export function decisionAlertLevel(decision: string): PresenceAlertLevel {
     switch (decision) {
       case "allow":
         return "ready";
       case "needs_confirmation":
         return "confirm";
       case "blocked":
         return "blocked";
       default:
         return "critical";
     }
   }
   ```

   Then `soulActions.test.ts` can test the contract cleanly, and `soul-server.ts` just calls it.

Everything else looks good. I especially agree with the plan’s restraint: **face first, passport torso first, halo later**. That is the right engineering move for this stack.

Gemini - 

My apologies for jumping the gun on the implementation! I have reverted my changes.

I've reviewed the COMPASS Implementation Plan, particularly Slice 1, and the plan is extremely solid. The strict order of execution (tests -> TS protocol types -> soul logic -> Rust body) is the right way to build across this IPC membrane. It prevents visual drift and guarantees that the dumb-renderer body is always rendering a truthful data contract.

Here is my advice and approval on the plan:

1. I completely endorse the GPT Audit notes
The two points at the bottom of the plan are spot on:

Tightening the "unwired" taxonomy: Conflating session (which genuinely doesn't need an effector) with agent_zero (which needs one but isn't wired) will cause UI bugs later when you try to distinguish between a safe idle state and an inoperable surface. Testing for available vs unwired vs gated is much safer.
Exporting decisionAlertLevel: Moving the mapping to src/soulActions.ts is critical because scripts/soul-server.ts is generally the outer imperative shell and harder to unit-test cleanly. By putting it in soulActions, you can guarantee the logic in soulActions.test.ts.
2. Rust Serde Warning (Slice 1d/1e)
When you get to updating desktop-body/src/presence.rs to receive alertLevel and locality, make sure your Rust structs use Option<String> (or a custom enum wrapped in Option) with #[serde(skip_serializing_if = "Option::is_none")] and potentially #[serde(default)].

Since the protocol specifically mentions these fields are "additive, non-breaking," the Rust JSON parser will silently fail/drop the entire message if it strictly expects these new fields to be present.

3. Decoupling Emotion from Alert Level is the right move
By keeping emotion as the primary trust indicator but adding alertLevel to the express event, you are perfectly setting up the board for Slice 3. It allows the body to know "how to color the route ring/pulse" independently of "what face the character is making", strictly adhering to Law 7 (the body shouldn't have to infer policy status from a facial expression string).

4. Layout Constraints (142px)
The 142px TORSO_W constraint is tight, but breaking the PassportCard into fixed rows (Persona/Posture -> Route Chip -> Divider -> Output) is much safer than the current freeform layout. For the locality dot and provider name, you might want to ensure the Rust body has text-truncation (...) cleanly implemented in the tiny-skia renderer so long provider names don't break the layout.

Verdict: The plan is clean, disciplined, and strictly adheres to your architectural laws. You are completely clear to begin Slice 1 in the exact order specified. Let me know when you are ready to write the tests!

---

## Composer Audit (Grok)

**Date:** 2026-06-15

### Verdict

**Approve the plan, with amendments.** Opus wrote a disciplined, repo-grounded implementation spec. The slice order (passport torso → surface dial → route ring → governance rail → effector honesty), the “face first, halo later” restraint, and the tests-first Slice 1 contract are all correct engineering for this stack.

Do **not** start Slice 2+ until Slice 1 is landed and you have decided how COMPASS relates to the open Phase C work (Forge browser act membrane). Slice 1 is ready to execute; Slices 2–5 need tightening before you treat them as committed scope.

---

### What the plan gets right

**Ground rules are accurate.** Pure-Rust tiny-skia, no glassmorphism, `TORSO_W = 142`, Law 7, WebSocket JSON — all match the codebase and `OVERLAY_POSTMORTEM`. The audit’s “no halo in `render.rs`” finding is correctly reflected.

**Build order inside Slice 1 is correct.** Types → failing tests → soul pushes → Rust render is the right cross-language seam discipline (same pattern as `STEP4_WIRE_THE_SOUL_PLAN.md` and the execution membrane landing).

**Face-first trust is the right call.** `Emotion::for_decision()` and `decisionEmotion()` are already a tested cross-surface contract. Deferring halo to Slice 3 avoids painting trust signals before the data contract exists.

**Out of scope is honest.** No GPU compositing, no autonomous screen action, no silent fallback — all aligned with AGENTS.md and postmortem reality.

**GPT’s two audit notes above are correct** and should be folded into the main plan body, not left as append-only commentary.

---

### Where the plan is stale or partially done

#### 1. “Current Wire Protocol” understates what’s already shipped

The baseline table is missing fields that already exist:

- `action_request`: `intent`, `routeHint`, `confirmed`, `requestId`
- `action_result.outcome`: `executed`, `executionReceiptId`, `route` (with `locality`, `downgraded`, `fallbackOf`)

Slice 1’s *new* gaps are real, but narrower than the doc implies:

| Field | TS types | Validator | Rust parser | Soul pushes |
|-------|----------|-----------|-------------|-------------|
| `alertLevel` on `express` | ❌ | ❌ | ❌ | ❌ |
| `locality` on `surface_active` | ❌ | ❌ | ❌ | ❌ |

Slice 1a is not greenfield — it’s finishing half-started work. Tests in `presenceProtocol.test.ts` already *aspire* to `alertLevel`/`locality`, but `tsc` fails on those cases today. That’s compile-time red, not assertion-time red.

#### 2. Slice 1b is partly superseded

`surfaceManifest.test.ts` already implements the **better** unwired taxonomy GPT recommends (“maps placeholders to known but unwired effectors”). The plan’s test *“surfaces without effectorId are classified as unwired”* is wrong and should be removed from Slice 1b — it’s already been corrected in code.

What’s still missing from 1b:

- Formal `PresenceAlertLevel` type + validator
- `decisionAlertLevel()` in `soulActions.ts` (parallel to `decisionEmotion`)
- `gen:fixtures` + golden fixture update for new fields (required by Step 4 cross-language parity — not mentioned in the plan)

#### 3. Slice 2 still uses the wrong “unwired” definition

Slice 2 says *“Unwired surfaces (no effectorId in surfaceManifest)”* — that would dim `session` and `customize`, which are intentionally presentational. GPT’s `available | unwired | gated` taxonomy must be applied here before Slice 2 is scoped.

---

### Structural gaps (fix before trusting Slices 2–5)

#### A. Slice 1 vs Slice 3 protocol collision

Slice 1 adds flat `locality` on `surface_active`. Slice 3 adds a nested `route: { label, locality, health }` on the same event.

That will force a migration or duplicate fields. **Recommendation:** In Slice 1, either:

- Ship only `providerLabel` + passport row 1 using existing fields, defer `locality` to Slice 3’s `route` object; **or**
- Introduce the full `route` object in Slice 1 and skip flat `locality`.

Don’t add flat `locality` in 1a and nest it under `route` in 3.

#### B. `alertLevel` source-of-truth is underspecified

Today the native body sets face from `action_result.decision` via `Emotion::for_decision()` in `main.rs`. The soul *also* sends `express` before `action_result` with `decisionEmotion()`.

Slice 1 adds `alertLevel` on `express` only. That creates three parallel mappings of the same decision (emotion on express, emotion on action_result, alertLevel on express).

**Recommendation:** Pick one authoritative path for torso/ring coloring:

- **Option A (cleanest):** Add optional `alertLevel` on `action_result` (alongside `decision`), body reads it for passport/ring; `express` stays emotion-only.
- **Option B (plan as written):** `alertLevel` lives on `express`; document that passport/ring must *not* re-derive from `action_result.decision` locally.

Either works; the plan should state which, or you’ll get flicker/desync bugs.

Also fix the inconsistency: soul-server snippet uses `?? "quiet"` for unknown decisions; GPT audit says `critical`. Pick one and test it.

And the plan says `allow → "ready" or quiet if no output pending` — the snippet doesn’t implement that conditional. Either drop the conditional from the spec or add the rule to `decisionAlertLevel()`.

#### C. Slice 3 `route.health` has no soul derivation

Who sets `ready | degraded | unavailable`? The plan doesn’t say. Without a deterministic soul-side function (e.g. from provider reachability, `downgraded`, grant state), the outer ring becomes decorative — exactly what the plan claims to avoid.

**Add to Slice 3 before build:** a `routeHealthFromSoul(...)` spec tied to real signals (downgrade flag, wired state, last action outcome), with unit tests in `soulActions.test.ts`.

#### D. Slice 4 overlaps shipped work and violates Law 7 on detail

**Review/Confirm already exists** (Step 4, 2026-06-13): on-body button flips on `needs_confirmation`. Slice 4’s inline confirm bar duplicates this unless you explicitly retire the button path.

**“Tap card → expand to full receipt detail”** — the body cannot show full derivation without the soul sending it. Law 7: full `ActionReceipt` stays soul-side. Slice 4 needs either:

- A `receipt_summary` cue (thin, like `action_result`), or
- Scope receipt rail to `summary` + `decision` + `receiptId` only (no “full detail” claim)

#### E. Slice 2 hold-to-bloom is a large input-system project

The body emits `grabbed` on drag past `CLICK_SLOP`, not on press-and-hold in place. A 250ms hold radial dial needs new press timing, hit-testing for arc segments, and animation state — with no test plan beyond “surface_request round-trip.”

**Recommendation:** Slice 2 Phase A = arrow fallback cycle + unwired dim (low risk). Phase B = hold-to-bloom (separate PR, separate test harness). Don’t bundle them as one slice.

#### F. No explicit relationship to Phase C

The `What it is becoming.md` charter is:

```text
Forge (browser) → /review repo_edit <path> → three Playwright cases
```

COMPASS Slice 1 is native-body UX. They share protocol work but serve different product claims:

| Track | Proves |
|-------|--------|
| Phase C | Users see the execution membrane on Forge |
| COMPASS Slice 1 | Native buddy shows passport torso + route locality |

These can run in parallel **only if** protocol changes are coordinated. The plan should add a one-line precedence rule, e.g. *“Phase C protocol fields (`intent`/`outcome` on wire) are frozen; COMPASS additive fields must not break golden fixtures or Playwright specs.”*

---

### Slice-by-slice readiness

| Slice | Ready to build? | Notes |
|-------|-----------------|-------|
| **1** | ✅ Yes, with amendments above | Finish types/validators/fixtures/soul push/Rust passport. Add `render.rs` layout regression test (pattern already exists) alongside COSMIC smoke. |
| **2** | ⚠️ Partially | Fix unwired taxonomy; split hold-to-bloom from arrow cycle. |
| **3** | ❌ Not yet | Resolve locality shape with Slice 1; add `health` derivation spec. |
| **4** | ❌ Not yet | De-dupe Confirm UX; scope receipt detail to what the wire allows. |
| **5** | ❌ Not yet | `terminal`/`launch_codex` likely unwired; host launch is a governance slice, not a render slice. Target bracketing risks looking like screen control — needs UX copy guard. |

---

### Recommended amendments to the doc

1. Update **“Current Wire Protocol”** to include `intent`/`outcome` fields already shipped.
2. Replace Slice 1b’s naive unwired test with a pointer to existing `surfaceManifest.test.ts` coverage + add `decisionAlertLevel` tests.
3. Add **Slice 1f**: `fixtures/presence-v0.json` + `npm run gen:fixtures` + Rust `cargo test` parity gate.
4. Resolve **flat `locality` vs nested `route`** before Slice 1a merges.
5. Document **`alertLevel` authority** (express vs action_result).
6. Fix Slice 2 **unwired definition** to `available | unwired | gated`.
7. Add **Phase C coordination** note at top of Build Order Summary.
8. Split Slice 2 into **2a (cycle/dim)** and **2b (hold-to-bloom)**.
9. Add soul derivation spec for **`route.health`** before Slice 3 is scoped.
10. Revise Slice 4 to **extend** existing Review/Confirm, not replace it.

---

### Bottom line

Opus’s plan is one of the better roundtable-to-builder translations in this repo: it respects the Rust stack limits, doesn’t chase glassmorphism, and sequences trust visuals behind data contracts.

**Composer vote:** Execute **Slice 1 only**, with GPT’s two corrections baked in, plus the fixture/authority/locality amendments above. Charter Slices 2–5 as design intent, not committed sprint scope, until Phase C’s browser proof is either landed or explicitly deprioritized.

---

## Grok Audit

**Date:** 2026-06-15  
**Auditor:** Grok (second pass — complements Composer audit above; net-new points only)

### Verdict

**Approve Slice 1. Charter Slices 2–5.** Opus translated the roundtable into something buildable on this stack. I agree with GPT, Gemini, and Composer: the plan’s discipline is right. What follows are additions and one correction Gemini got slightly wrong.

---

### Net-new: things the plan should say explicitly

#### 1. Passport replaces the *idle* torso only — not all `TorsoOutput` variants

Today `main.rs` drives the torso through a `TorsoSurface` state machine:

- `Session` → `SessionCard` (six fields: name, provider, model, gateway, status, note — this is the 142px overflow)
- `output` cues → `Text` / `Image` / stubs

Slice 1d should state clearly: **`PassportCard` supersedes `SessionCard` at idle**, but `Text`/`Image`/`ImageStub`/`FileStub` remain unchanged when the soul pushes `output`. Row 3’s `output_preview` is a one-line peek when idle; it does not replace the full text/image cards.

Without that sentence, an implementer will try to fold all output into the passport and break the existing `apply_output` path.

#### 2. Defer flat `locality` — ship `route` once in Slice 1a instead

Composer flagged the Slice 1 / Slice 3 collision. **Grok vote:** skip flat `locality` on `surface_active`. Introduce the Slice 3 shape early:

```ts
route?: { label: string; locality: "local" | "cloud"; health?: "ready" | "degraded" | "unavailable" };
```

- Slice 1 passport row 1 reads `providerLabel` + `route.locality` (dot) — no migration later.
- `health` stays optional until Slice 3; soul omits it in 1c.
- Avoids deleting flat `locality` in a follow-up PR.

#### 3. `alertLevel` authority — Grok picks Option A

Composer listed two options. **Put `alertLevel` on `action_result`**, not only on `express`:

- Face continues from `decision` → `Emotion::for_decision` (already shipped, tested).
- Passport row tint / future ring reads `alertLevel` from the same `action_result` cue — one event, one truth.
- Soul still may send `express` for mood, but torso chrome must not depend on event ordering between express and action_result.

Export `decisionAlertLevel()` beside `decisionEmotion()` in `soulActions.ts` (GPT note). Test both in lockstep, same way `soulActions.test.ts` already asserts the `for_decision` twin.

**Default for garbage input:** `critical` (GPT), not `quiet` (plan snippet). Fail loud matches `Emotion::for_decision` → Alert.

**Drop the conditional** `allow → ready or quiet if no output pending` from Slice 1 unless you add a named soul function `idleAlertLevel(buddyState)` with tests. Otherwise ship `allow → ready` and revisit `quiet` when idle semantics are spec’d.

#### 4. Gemini’s serde warning — partially wrong for this codebase

`desktop-body/src/presence.rs` does **not** serde-deserialize cues. It hand-parses JSON with `Value` and builds `Cue` enums. Additive fields on the wire are **ignored**, not fatal — the risk is they are **never extracted**, not that the whole frame drops.

Slice 1e must: extend `Cue::Express` / `Cue::SurfaceActive` variants, update `parse_to_body` match arms, extend golden fixtures, and add Rust unit tests beside the existing `parses_surface_active_fixture` pattern. No `#[serde(default)]` needed on cue structs.

#### 5. Surface availability taxonomy — promote to shared TS helper (Slice 1b, not Slice 2)

GPT’s `available | unwired | gated` classification should live in `src/surfaceManifest.ts` (or a tiny `surfaceAvailability.ts`), unit-tested once, consumed by:

- Browser buddy (dim surfaces in dock)
- Soul (when pushing `surface_active`)
- Later: Rust body only if soul pushes availability flags per surface (preferred — **body must not read `surfaceManifest.ts`**)

The native body cannot import the TS manifest. Slice 2’s “unwired = dim” either needs soul-pushed hints on `surface_active` (e.g. `availability: "available" | "unwired" | "gated"`) or a duplicated constant in Rust — the plan should pick soul-pushed hints to preserve manifest-free protocol.

#### 6. Slice 1 acceptance gates (concrete)

Before calling Slice 1 done:

```bash
npx tsc --noEmit                    # must be clean (today fails on alertLevel/locality test cases)
npx vitest run src/__tests__/presenceProtocol.test.ts src/__tests__/soulActions.test.ts src/__tests__/surfaceManifest.test.ts
npm run gen:fixtures                # regenerate presence-v0.json
cd desktop-body && cargo test       # golden fixture parity
```

Plus: `render.rs` layout regression test (pattern exists at `Emotion::for_decision` tests) asserting `PassportCard` row heights fit within `TORSO_W`. COSMIC smoke is human-only; don’t block the commit on it.

#### 7. Anchor (Grok 4.3) — preserve clay in Slice 1

Credit table lists Grok 4.3 · Anchor for “clay → premium glass evolution.” Slice 1 is **passport layout on existing clay**, not a visual refresh. Keep `draw_clay_texture()` and current palette. Glass/warm-LIMEN chrome is a later slice after the ledger rows are truthful.

#### 8. Slice 2 prerequisite the plan omits

`main.rs` used to cycle surfaces via a Rust `SURFACE_ORDER` mirror. Slice 2a resolves this by
having the soul push the ordered surface list on `hydrate`; the Rust constant now exists only as a
pre-hydrate fallback seed. Hold-to-bloom can build on the stored hydrate list instead of adding a
second manifest.

---

### Corrections to fold into the main plan body (not append-only)

| Section | Change |
|---------|--------|
| Slice 1a | `route?` on `surface_active` instead of flat `locality`; `alertLevel` on `action_result` |
| Slice 1b | Remove naive unwired test; add `decisionAlertLevel` + `surfaceAvailability()` tests |
| Slice 1d | Passport replaces `SessionCard` idle only; keep Text/Image output path |
| Slice 1f (new) | `gen:fixtures` + `cargo test` parity gate |
| Slice 2 | Soul-pushed `availability` per surface; split 2a cycle / 2b hold-to-bloom |
| Slice 4 | Extend existing Review/Confirm button; receipt rail = thin cues only |
| Ground Rules | Note `action_request.intent` / `action_result.outcome` already on wire |

---

### Slice readiness (Grok)

| Slice | Go? | Grok note |
|-------|-----|-----------|
| 1 | ✅ | Best ROI: fixes screenshot overflow, grounds COMPASS in data contract |
| 2a | ⚠️ | Arrow cycle + dim only, after availability helper exists |
| 2b | ❌ | Hold-to-bloom = new input subsystem; own PR |
| 3 | ❌ | Needs `route.health` soul derivation spec |
| 4 | ❌ | De-dupe Confirm; no full receipt detail without new cue |
| 5 | ❌ | Host launch is governance; eyes/ears need `effectors[]` + grants model |

---

### Grok vote

**Ship Slice 1 with the amendments above.** It is the smallest honest step from “clay session card overflows” to “passport ledger renders soul truth.” Everything else in COMPASS is valuable design intent — keep it in this doc, don’t sprint it until Slice 1 is green on `tsc`, vitest, fixtures, and `cargo test`.

*— Grok · 2026-06-15*

---

*COMPASS · Implementation Plan · Border Agents desktop buddy*
*One keeper · many souls · every action leaves a light*
