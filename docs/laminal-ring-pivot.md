# Laminal Ring Pivot — plan & source of truth

**Status:** active. R1 landed (`bbdff73`). This document is the durable anchor for the
figure→ring visual pivot: the decisions that are locked, the slice plan (R1–R4, F1–F4),
the "done" bar, and the drift canaries. If a slice instruction conflicts with what is
written here, the conflict is surfaced and resolved before code — not guessed around.

---

## The stance (load-bearing)

The product must be **laminal**: governance is a thin laminate, *felt as structure, not
as the thing itself*. The surface the user touches is the buddy; the laminate (governance)
shows up only when a boundary is crossed — as guidance, not bureaucracy. And the visual
surface is a **ring whose hue encodes state**, not an anthropomorphic face. The clay
figure is being demoted as the primary identity.

This is a trajectory correction, not a principle change: the stance docs already say
laminal (README: "the edge-native governance layer *underneath*"; UX.md law 1: "the agents
are the hero"; law 2: "governance should feel like guidance, not bureaucracy"). What
drifted is the *surface* — the native body renders a clay figure (face, blink, emotion,
speech) as the primary identity, and governance display area kept accreting. The pivot
turns the ring into the surface and freezes the figure.

## The finding that shapes the work

The laminate ring is **not a from-scratch build — it is ~80% already wired, and the body
was discarding the signal.**

- The protocol already defines the exact five states:
  `PresenceAlertLevel = "quiet" | "ready" | "confirm" | "blocked" | "critical"`
  (`src/presenceProtocol.ts`).
- The soul already computes it from the gate decision — `decisionAlertLevel` in
  `src/soulActions.ts` (allow→ready, needs_confirmation→confirm, blocked→blocked,
  unknown→critical) — and already puts it on the `action_result` wire.
- The protocol authors already planned it: the doc comment on `PresenceAlertLevel` reads
  "the chrome … *later the route ring* … the face and the chrome derive from one event and
  one truth." That is "the membrane and the ring are the same laminate from two sides,"
  already in the architecture.
- **But** before R1 the native body had no `alert_level` field on `BodyView`; its ring was
  the narrower 3-value `route_health` stroke (`route_health_ring_rgba`, ready/degraded/
  unavailable), painted as a stroke of the *figure's silhouette* (`stroke_figure_boundary`).

So the pivot is: **consume the `alertLevel` the soul already sends, generalize the 3-hue
figure-stroke into the 5-hue ring, and detach the ring from the figure silhouette so it can
stand alone when the figure is off.** Smaller and safer than "rewrite render.rs."

Note on history: the "perimeter ring" retired in `c7adbfb` was a ring of *control buttons*
around the figure, **not** a state-hue halo. The laminate ring has never been tried and
rejected. Different thing.

---

## Decisions locked (do not relitigate)

### 1. Figure → opt-in skin, not deletion
`BB_SKIN=ring|clay`, default `ring`. The clay figure is **frozen: skin-only, never
extended.** It is not deleted (it is working, drag-stable, user-verified code; a flag
preserves optionality at zero cost). The discipline that enforces laminality is not
destroying the figure — it is: **the figure is never the default, and nothing new renders
against it.** `BB_SKIN=ring` belongs in the default dev task so the laminal path is the one
we live in.

The face was doing three jobs; all three are reassigned:
- **State** → the ring hue + its pulse cadence (more glanceable than a face, and readable
  peripherally via the tucked edge light bar).
- **Identity** → a minimal role mark + the pane header (see decision 2), *not* a face.
- **Personality** → ring *behavior* (a calm green breath vs. a held amber vs. a sharp red)
  + language in the pane and the Wizard's voice. Personality was never in the mitten hands;
  it was in "One claim needs evidence." It survives the face's removal intact.

### 2. Identity = mark slot + one filled instance
Replace face-as-identity with a **mark slot** (`mark: Option<Sigil>` + a render slot inside
the ring) and fill exactly **one** instance: `Sigil::Anvil` for Forge. Two hard constraints:
- **Build the slot, fill one instance.** No per-role sigil set now — a second role adds a
  `Sigil` variant later, never a surface refactor. Designing 7 sigils before one flow is
  proven is *visual infrastructure ahead of use* — the same drift the arc hunts, at the
  render layer. (This is why the identity slice is F1, single-buddy, not an early
  multi-role build.)
- **The mark is hue-independent (monochrome).** State and identity are orthogonal channels:
  ring hue = state, sigil = who. The anvil is the same grey whether the ring is amber
  (Forge needs confirm) or green (Forge ran). Coupling them would break the "one
  unmistakable reading" property.

Layering: the **pane header** ("Forge · Intent→Action") is the authoritative identity; the
**anvil** is the rapid-recognition marker at the *peeked* state (edge-visible, pane closed)
— the low-friction moment where you'd otherwise have to summon to disambiguate. If the
Forge flow lands and users read the header instead, the per-role system stays deferred
indefinitely; if they read the anvil, the channel is validated for the cost of one mark.
Neither outcome is reversible-expensive — which is why deferring is free.

### 3. Governance is proven → maintenance only
The core is deterministic, test-backed, receipt-producing, and the vertical slice traces
byte-identically to the ledger. No new governance surface area during this pivot unless a
named user flow requires it.

---

## The slice plan

Same discipline as the governance vertical slice: each slice stops at a gate; audit before
push; verify claims independently (re-run gates yourself — cargo's incremental compile is
the staleness check for any test that shells to a binary, never `existsSync`); extend the
CI trace harness rather than eyeballing.

### Week 1 — the ring becomes the state surface (figure untouched)

- **R1 — Consume `alert_level`. ✅ DONE (`bbdff73`).** Added the closed enum
  `AlertLevel { Quiet | Ready | Confirm | Blocked | Critical }`; threaded it wire → parse →
  body state → `BodyView`, mirroring the `route_health` path. **No render change** — the
  compiler's "field never read" warning on `BodyView.alert_level` is the proof of that, and
  it clears when R2 reads the field. Extended the trace harness to assert
  `decision → alertLevel` wire===body for all three decisions. tsc clean, vitest 278, cargo
  98+0+29. (Discovery: the golden fixture already carries `alertLevel:"ready"`, so it now
  anchors the present-valid parse; the absent→None back-compat anchor moved to a unit test.)

- **R2 — The 5-hue palette, one source; drive the existing ring from `alert_level`.**
  Define the palette once (extend the `route_health` hue table into a full `AlertLevel`→hue
  map: quiet=blue-grey, ready=green, confirm=amber, blocked=red, critical=violet — four of
  five already exist). Read `BodyView.alert_level` in the existing ring paint path via an
  exhaustive `match`. Figure still draws; its halo now speaks the full 5-state vocabulary.
  Gate: all five states paint distinct hues on the current figure; trace/fixture asserts
  `decision → alertLevel → hue`.

- **R3 — Detach the ring into a primitive.** Introduce `draw_ring(alert_level)` as an
  independent halo that renders correctly *with the figure absent*. Add `BB_SKIN=ring|clay`
  (default `ring`). Gate: `BB_SKIN=ring` shows a standalone ring reading all 5 states;
  `BB_SKIN=clay` restores today's figure. Verify by manual native walk in both skins.

- **R4 — The tucked edge light bar.** When tucked, an edge-of-screen light bar mirrors the
  ring hue. Gate: peripheral readability — bar hue === ring hue === `alert_level`, asserted
  in the harness.

### Week 2 — one laminal flow, genuinely good

- **F1 — Identity-mark slot + Forge anvil.** Per decision 2: the slot + one monochrome
  instance + pane header. Gate: the buddy is recognizable with the face off (the §done-bar
  criterion 2 test).

- **F2 — The `repo_edit` flow in ring language.** Wire the real workspace `repo_edit`
  through the gate rendered entirely in the ring: amber hold → green receipt → red on
  protected target. Grade receipt id available on tap, not on the surface. Gate: the flow
  completes end-to-end with the figure off.

- **F3 — Pane content polish.** Intent + Approve/Cancel; refusal + why + next step; UX law 3
  ("every warning offers a next action") honored in every state. Gate: the full done-bar,
  all five criteria; independent native walk + harness green.

- **F4 (stretch) — Wizard in ring language.** Confirm the Wizard renders against ring+pane,
  no special figure. Likely mostly true already (it drives the ordinary body); this is a
  verification pass, not new build.

---

## "Done" for the polished flow (F-series) — measurable, not vibe

The flow is laminal when **all** hold, verified independently:

1. **Ring drives, from the wire.** Ring hue through the whole flow is a pure function of the
   soul's `alertLevel` on `action_result`. Provable by fixture: same decision → same hue.
2. **Figure absent from the signal path.** Run with `BB_SKIN=ring` (figure off). Every state
   is still unambiguously readable — state, identity (sigil + pane), next action. If removing
   the figure loses *any* meaning, the flow isn't laminal yet. This is the load-bearing
   criterion.
3. **Peripheral readability.** Tucked, the edge light bar mirrors the ring hue; a user not
   looking at the buddy can tell "waiting on me" (amber) vs "done" (green).
4. **Receipt is the laminate, not the headline.** On allow, the green ring carries the
   decision; the grade receipt id is on tap, not shoved forward. Governance sits one
   interaction *below* the surface — reachable, not presented.
5. **Trace intact.** The wire===body===ledger harness still passes for this flow.

---

## Drift canaries (watch every commit — message + diff)

- **Governance-as-product drift:** a `feat(body:)` / `feat(governance:)` commit whose
  subject is a new governance *display* (another rail, badge, receipt projection) rather
  than a user *flow*. Rule: **no new governance surface area unless a named flow requires
  it.** If the PR can't name the flow in "what border does this make visible," it's drift.
- **Figure-as-primary drift:** any commit *adding* figure behavior (touching `draw_eyes`,
  `draw_mouth`, `Emotion`, a pose/expression path). Rule: **the figure is frozen — skinned,
  never extended.** A commit that adds figure behavior is the drift. Reverse-tell: if
  `BB_SKIN=ring` starts silently breaking in normal dev because everyone tests with the
  figure on, the figure has quietly become load-bearing again — hence `BB_SKIN=ring` in the
  default dev task.

The F-series is where the "display not a flow" temptation is highest (polish ahead of flow
validation). That canary is watched hardest under the F-series lead.

---

## Roles

R1–R4 (ring becomes the state surface, figure untouched) run under the building-side
project lead; the builder builds. **Switch point: R4 → F1.** At R4→F1, project-lead passes
to the builder (Opus) and the building-side lead becomes auditor for the F-series (product
work: the `repo_edit` flow in ring language).

## Parked (don't lift unless forced)

- **Enum-vs-string seam.** `AlertLevel` is a real Rust enum while `health`/`locality`/
  `decision` are validated `String`s. The enum is the right call for R2's exhaustive
  `match alert_level → hue`. The two styles coexist; consolidation is optional, not required.
- **Ring persistence / idle decay.** `active_alert_level` persists until the next
  `action_result` overwrites it (mirrors `active_route_health`). Decide *before the
  F-series* whether the ring's tier should decay to `Quiet` on idle or only change on the
  next decision — so idle behavior is intentional, not inherited. Not blocking R2's first
  paint.
- **Tauri 2nd webview vs. native** for onboarding flows that outgrow the in-torso panel.
  Scale, not correctness.
