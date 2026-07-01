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

- **R2 — The 5-hue palette, one source; drive the existing ring from `alert_level`. ✅ DONE.**
  `alert_level_ring_rgba(AlertLevel) -> [u8;4]` is the single palette source, total and
  exhaustive (a new variant won't compile without a hue): quiet=blue-grey `[122,138,168]`,
  ready=green, confirm=amber, blocked=red (reused from route_health), critical=violet
  `[138,79,214]`. The ring paint path reads `BodyView.alert_level`; figure untouched.
  **Fork resolved — precedence, not replace:** `alert_level` is the ring's primary voice;
  `route_health` remains the fallback when no tier is set. Governance tier and provider
  health are different signals — folding provider failure into `critical` is a *soul-side*
  derivation (law 7: the body paints, never infers), parked below. Tests: 5-hue
  total+distinct pin; a real precedence proof (Confirm-over-ready ≠ ready-alone). The
  `decision → alertLevel → hue` chain is proven across two deterministic layers: vitest
  harness (decision→alertLevel, wire===body) + Rust tests (alertLevel→hue against the
  exact paint function). cargo 100+0+29, tsc clean, vitest 278.

- **R3 — Detach the ring into a primitive. ✅ DONE (code; pending native walk).**
  `draw_ring(alert_level, route_health, route_flash)` is a standalone halo with its own
  geometry (`RING_CX/CY/R/THICKNESS`, a clean circle on the presence column) — not a stroke
  of the figure silhouette, so it holds its shape with the figure gone. R2's precedence
  survives the detachment via a shared `ring_hue_rgba` helper. `Skin { Ring default | Clay }`
  + `env_skin()` reads `BB_SKIN` once at startup; `BB_SKIN=ring` is the default dev path
  (`bb-body.sh` exports it, the reverse-tell guard). Figure function bodies unchanged —
  only the eyes/mouth call sites are gated behind `Skin::Clay`. Tests: 5-state distinct +
  pairwise-unique; never-vanishes-at-Quiet (R2 precedence survives); skin-selects-through-
  paint-path (same BodyView, Ring vs Clay → different pixels). cargo 103+0+29, tsc clean,
  vitest 278. **Idle-decay decision (due at R3, ratified): hold the last tier — no wall-
  clock decay — with `absent → Quiet` so the ring is never blank. `confirm`/`blocked`/
  `critical` are standing obligations that must persist (criterion 3); decay could only
  ever apply to `ready`, and `ready`'s clear is event-driven ("green clears when the user
  engages the reply") deferred to F2 — where the flow defines the event, not a blind
  duration. F2's gate must surface the green-clear event definition.**

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

R1–R3 ran under the building-side project lead (GLM); the builder (Opus) built, the lead
audited. **Switch point: R3 → R4** (moved forward from R4→F1 after R3's native walk
passes). At R3→R4 the roles reverse: GLM becomes the builder (coder), Opus becomes
project lead + auditor. The F-series (product judgment: is this flow good?) then runs
with Opus as lead — fitting, since Opus authored the design doc — and GLM executing the
build. The "display not a flow" canary is watched hardest under the F-series lead,
whoever that is when the F-series lands.

## Parked (don't lift unless forced)

- **Enum-vs-string seam.** `AlertLevel` is a real Rust enum while `health`/`locality`/
  `decision` are validated `String`s. The enum is the right call for R2's exhaustive
  `match alert_level → hue`. The two styles coexist; consolidation is optional, not required.
- **Ring persistence / idle decay.** `active_alert_level` persists until the next
  `action_result` overwrites it (mirrors `active_route_health`). **Decide at R3** — when
  the ring detaches into a standalone primitive (`BB_SKIN=ring`, figure off) and idle
  behavior first becomes visible with the figure off — whether the ring's tier should
  decay to `Quiet` on idle or only change on the next decision. R3 is the natural
  decision point: you can't ship a standalone ring without deciding what it does at idle,
  and deciding it under R3 (infrastructure) keeps it deliberate rather than a last-minute
  call under F-series flow polish. Not blocking R2's first paint (R2's ring still rides
  the figure, so idle is contextualized by the figure being present).
- **Route-health fold into `critical` (soul-side).** The design doc folds provider failure
  into the `critical` tier. That unification is a *soul-side derivation* — the soul (e.g.
  `decisionAlertLevel` or a sibling) deciding route failure is a boundary event and emitting
  `critical` on the wire — never a body render conflation (law 7). Until the soul does this,
  the body's `route_health` fallback ring stays (R2's precedence design). When the soul
  folds it, the fallback either retires or stays as transport-only chrome — explicit call
  at that point. Natural slot: alongside R4 or early F-series, when the ring vocabulary is
  complete and the soul-side mapping is a small, testable change.
- **Tauri 2nd webview vs. native** for onboarding flows that outgrow the in-torso panel.
  Scale, not correctness.
