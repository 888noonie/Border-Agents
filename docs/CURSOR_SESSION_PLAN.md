# Cursor session plan — H-series (2026-07-02)

**You are Composer 2.5, the builder for this session.** The project lead (Fable) audits
everything you produce; the owner walks the native gates. Your work comes back to the lead
for audit before anything is pushed.

**Authority:** `docs/laminal-ring-pivot.md` is the source of truth, **including its
§Amendment 2026-07-02** (owner-ratified: default = clay figure + alert halo; standalone
ring skin = dev/test track). Read that document first. If anything in this plan conflicts
with it, or the code conflicts with either, **stop and write the conflict into your report
— do not guess around it.**

---

## Ground rules (non-negotiable)

1. **Scope:** this session touches `desktop-body/src/main.rs`, `desktop-body/src/render.rs`,
   and `scripts/bb-body.sh` only. If you believe a TS/soul change is needed, stop and report
   — do not make it. (Law 7: the body paints, never infers.)
2. **Commits:** one commit per slice, locally. **Never push. Never force-anything.**
   Commit message style: follow `git log` on this branch (`laminal-ring-pivot`) — subject
   `feat(body): laminal ring pivot — Slice Hn — <what>`, body explains what/why/tests/gates
   with exact counts.
3. **Gates, run for real and reported verbatim** (cargo's incremental compile is the
   staleness check — never `existsSync`-style shortcuts):
   - `cd desktop-body && cargo test` — baseline **108 passed, 0 failed, 29 ignored**; H-slices add named tests.
   - `cd desktop-body && cargo build --release` — clean; 10 pre-existing warnings are known
     (frame-driver dead code + `interior_rows` orphan). **Nothing new.**
   - repo root: `npx tsc --noEmit` — clean.
   - repo root: `npx vitest run` — baseline **278 passed / 31 files**, expected **unchanged**
     (these slices are Rust-render/launch only).
4. **Canaries** (check your own diff before committing, report the grep):
   - No new RGBA literals outside `alert_level_ring_rgba` / `route_health_ring_rgba`
     (test-only sampler literals like `[0,0,0,0]` are fine; name any halo geometry consts).
   - `draw_bump`, `draw_closed_eyes`, `draw_eyes`, `draw_mouth` **bodies byte-identical** —
     call sites may gain gates/additions, bodies never change.
   - No new governance display surface. No `Sigil`/mark work (F1 is deprioritized).
5. **Known trap:** `tiny_skia::Pixmap::data()` is **premultiplied RGBA** (not BGRA — the
   BGRA swap happens later in `blit_premultiplied_bgra`). R4's test sampler/demultiply
   helpers in `render.rs` already handle this; reuse them, don't re-derive.

---

## Slice H1 — flip the default skin to Clay; make the ring-skin tuck retrievable

**Why:** §Amendment point 1–2. Also fixes the R4 walk failure: under `BB_SKIN=ring` the
tucked buddy's summon target is the old *invisible* bump rect, so the owner could not
retrieve it. No trap ships, even on the dev track.

**Changes:**

1. `env_skin()` (`desktop-body/src/main.rs:145`): default arm becomes `Skin::Clay`;
   `Some("ring")` opts in. Update the doc comment to match the amendment.
2. `scripts/bb-body.sh:58-60`: flip `export BB_SKIN="${BB_SKIN:-ring}"` to `clay`, and
   rewrite the comment — the ring path's rot guard is now the cargo test suite, not the
   launch default. `BB_SKIN=ring bb-body.sh` opts into the dev track.
3. **Ring-skin tucked hit target = the full edge bar.** Recon first: find every call site
   of `tucked_bump_rect` / `point_in_tucked_bump` / `render::bump_rect` /
   `render::point_in_bump` (known: `main.rs:1389`, `main.rs:1393`, `main.rs:1865`,
   `main.rs:3354`; there may be more). Classify each as **input** (input region, hit-test,
   click/summon) or **display anchor** (peek bubble position, layout). Then:
   - *Input* sites: under `Skin::Ring`, the rect/hit-test covers the full edge bar
     (`BAR_THICKNESS` deep, full edge length — mirror `draw_edge_bar`'s geometry; add a
     `bar_rect(edge, w, h)` helper in `render.rs` next to `bump_rect` so paint and hit share
     one geometry source). Under `Skin::Clay`, byte-identical bump behavior.
   - *Display anchor* sites: leave bump-based (the peek bubble may keep anchoring where the
     bump sits) — unless it looks visually broken against the bar, in which case report,
     don't redesign.
   - Your report must include the call-site classification table.
4. If any existing test pins the old default (`env_skin` default = Ring), flip it
   deliberately and say so in the report — that flip is the amendment landing, not drift.

**Tests (new, named):**
- `env default → Clay` pin (however `env_skin` is currently testable; if it isn't, pin via
  the `Skin` selection seam and note it).
- `ring_tuck_full_bar_is_hittable` — under Ring, a point near the bar's far end (away from
  the old bump position) hits; under Clay, the same point misses and the bump circle hits.
- All existing skin/ring/bar tests still green.

**Commit:** `feat(body): laminal ring pivot — Slice H1 — clay default + full-bar tuck target`

---

## Slice H2 — the tucked clay bump wears the alert hue

**Why:** §Amendment point 3 and done-bar criterion 3 (peripheral readability). The clay
tuck currently shows a hue-less sleeping bump; a tucked buddy should glow amber when it is
waiting on the owner — without losing the face.

**Changes:**

1. New `fn draw_bump_halo(pixmap, edge, w, h, alert_level, route_health)` in `render.rs`:
   a stroked arc/ring around the bump circle (`bump_center` + `BUMP_R` plus a small named
   outset const), hue from **`ring_hue_or_quiet`** (absent → Quiet; the halo is never blank
   — same stance as ring and bar). No pulse/cadence — hue only, same as R4's bar.
2. Call it from the tucked `Skin::Clay` branch **after** `draw_bump`
   (`render.rs:1295` area). `draw_bump`'s body stays byte-identical — the halo is a sibling
   call, not an edit. Ring branch untouched.
3. Halo geometry: stroke width and outset as named consts near `BUMP_R`. Keep it chrome —
   thin, outside the face, never covering the eyes area.

**Tests (mirror the R4 shapes; reuse R4's pixel sampler/demultiply helpers):**
- `bump_halo_reads_all_five_states_distinctly` — per level, halo visible; no two collapse.
- `bump_halo_hue_equals_palette_exactly` — sample a halo stroke pixel, demultiply,
  ≈ `alert_level_ring_rgba(level)` within ±1, across all four edges.
- `bump_halo_precedence_alert_over_route` — Confirm-over-ready === Confirm-only.
- `bump_halo_never_vanishes_absent_rests_at_quiet` — no tier + no route → visible Quiet;
  route fallback survives.
- Existing `skin_gates_tucked_path_ring_vs_clay` still passes.

**Commit:** `feat(body): laminal ring pivot — Slice H2 — tucked bump wears the alert hue`

---

## STOP after H2

Do **not** start F2 (`repo_edit` flow) or anything else. F2's prerequisite — the
green-clear event definition — is a lead+owner decision that has not been made. Two
audited slices beat three unaudited ones.

## Report back (required, per slice)

- Commit hash + full message.
- All four gate outputs, verbatim counts (cargo test / cargo build --release warnings /
  tsc / vitest).
- Canary greps and their results (palette literals; figure-function bodies).
- H1: the call-site classification table (site → input | display anchor → action taken).
- Discoveries, uncertainties, anything you chose NOT to do and why.
- Any conflict stops (plan vs doc vs code) — verbatim, with your reasoning, no workaround.

---

## Builder report (Composer 2.5, 2026-07-02)

**Status:** H1 + H2 complete. Stopped after H2 per plan. **Not pushed.**

### H1 commit

```
05c1cc59677be27a44a50268d7b7d0ffa50a1592
feat(body): laminal ring pivot — Slice H1 — clay default + full-bar tuck target

Flip BB_SKIN default to clay (figure + alert halo); ring opts into the dev/test
track. Ring-skin tuck hit/input region now covers the full edge bar via shared
bar_rect geometry (paint and hit-test use one source). bb-body.sh default inverts
to match the Amendment 2026-07-02 stance.

Tests: cargo test 111 passed (desktop-body main), +skin_default_is_clay,
+ring_tuck_full_bar_is_hittable, +env_skin_defaults_to_clay_without_bb_skin.
```

### H2 commit

```
ff78712d16ab56fe1e00da5816aaf9270762762b
feat(body): laminal ring pivot — Slice H2 — tucked bump wears the alert hue

Add draw_bump_halo as a sibling call after draw_bump on the tucked Clay path.
Hue resolves through ring_hue_or_quiet (same precedence as ring/bar); draw_bump
body stays byte-identical. Ring tucked path untouched.

Tests: cargo test 115 passed (desktop-body main), +bump_halo_reads_all_five_states_distinctly,
+bump_halo_hue_equals_palette_exactly, +bump_halo_precedence_alert_over_route,
+bump_halo_never_vanishes_absent_rests_at_quiet; skin_gates_tucked_path_ring_vs_clay still green.
```

### Gate outputs (verbatim)

**`cd desktop-body && cargo test`**

```
     Running unittests src/main.rs (target/debug/deps/bb_desktop_body-...)
running 115 tests
test result: ok. 115 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.31s

     Running unittests src/bin/frame_driver.rs (...)
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running unittests src/bin/parse_action_result.rs (...)
running 29 tests
test result: ok. 29 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

Baseline was 108 passed (main); H1 landed +3 named tests (111); H2 landed +4 (115).

**`cd desktop-body && cargo build --release`**

```
warning: `bb-desktop-body` (bin "bb-desktop-body") generated 10 warnings
    Finished `release` profile [optimized] target(s) in 0.12s
```

10 warnings — frame-driver dead code + `interior_rows` orphan. Nothing new.

**`npx tsc --noEmit`**

```
(clean — exit 0, no output)
```

**`npx vitest run`**

```
 Test Files  31 passed (31)
      Tests  278 passed (278)
   Start at  20:20:30
   Duration  1.30s
```

Unchanged from baseline (278 / 31 files).

### Canary greps

| Canary | Result |
|--------|--------|
| No new governance RGBA literals outside palette tables | PASS — `draw_bump_halo` uses `ring_hue_or_quiet` only; geometry consts `BUMP_HALO_OUTSET`, `BUMP_HALO_STROKE` |
| `draw_bump` / `draw_closed_eyes` / `draw_eyes` / `draw_mouth` bodies byte-identical | PASS — H2 adds `draw_bump_halo` as sibling call; no body edits |
| No new governance display surface; no Sigil work | PASS |

### H1 call-site classification

| Site | Role | Action |
|------|------|--------|
| `main.rs` `tucked_bump_rect` | input | Ring -> `bar_rect`; Clay -> `bump_rect` |
| `main.rs` `point_in_tucked_bump` | input | Ring -> `point_in_bar`; Clay -> `point_in_bump` |
| `main.rs` `update_input_region` | input | Uses `tucked_bump_rect` (inherits skin gate) |
| `main.rs` tucked press handler | input | Uses `point_in_tucked_bump` (inherits skin gate) |
| `main.rs` `clamp_tucked` | input | Uses `tucked_bump_rect` (full bar under Ring) |
| `render.rs` `tucked_peek_origin` / bubble / input rects | display anchor | unchanged — bump-anchored |

### Discoveries / not done

- `Skin::default()` flipped to `Clay`; `env_skin_defaults_to_clay_without_bb_skin` pins the
  `env_skin()` arms (`ring` opt-in, everything else -> clay). Amendment landing, not drift.
- Peek bubble/input anchors left bump-based — visually acceptable; no redesign attempted.
- F2, push, TS/soul changes: not started (out of scope / plan stop).
- `.vscode/tasks.json` has an unstaged prior edit (BB forge + soul ring task) — outside H-series scope.

### Conflict stops

None. Plan, `laminal-ring-pivot.md` Amendment 2026-07-02, and code align.

---

## Lead audit (Fable, 2026-07-02)

**Verdict: PASS — both slices. Push gated only on the owner walk.**

Independently verified (not taken from the report):
- Gates re-run with forced recompile (`touch src/*.rs`): cargo **115+0 / 29**, release
  warnings **10** (pre-existing set), tsc **clean**, vitest **278 / 31**. All match.
- Figure-freeze canary: `draw_bump` / `draw_closed_eyes` / `draw_eyes` / `draw_mouth`
  bodies md5-identical between `81632a3` and HEAD.
- Palette canary: zero new RGBA literals in the H1+H2 diff; both new paints resolve
  through `ring_hue_or_quiet`.
- H1 geometry is genuinely single-source: `draw_edge_bar` was refactored to consume the
  same `bar_rect` the hit-test uses — paint and input cannot drift apart.
- Call-site classification checked against the diff: input sites skin-gated, peek
  bubble/input anchors left bump-based per spec.

Observations (recorded, not blockers):
1. `env_skin_defaults_to_clay_without_bb_skin` mutates process env; Rust tests run in
   parallel threads, so this could flake if another test ever reads `BB_SKIN`. None does
   today. If it ever flakes, serialize it — don't delete it.
2. The ring-skin tuck hit target is now the full bar but only `BAR_THICKNESS` (8px) deep.
   Findable, possibly fiddly. If the dev-track walk says so, the named follow-up is a
   *deeper input rect* (grab zone > visual bar), not a thicker bar.
3. `.vscode/tasks.json` carries an unstaged pre-session edit ("BB forge + soul (ring)"
   task). Consistent with the amendment (ring as explicit opt-in). Owner to keep/stage
   or drop — not part of H-series.

**Owner walk (2026-07-02, COSMIC): PASSED — both skins.** Clay default: figure + amber
halo on "Edit repository needs your confirmation" → green halo + "Ran Edit repository";
tucked bump wears the hued halo; summon works. Ring dev track: full-bar tuck target
confirmed fixed by the owner. Pushed after this record.

**Owner proposals from the walk (next H-slices, specs to follow):**
1. **Dock settings** — tuck appearance becomes a user option, not skin-bound: toggles
   **Head** and **Bar** (either or both; never neither — no trap).
2. **Bar geometry** — halve the bar's length, thickness 8px → 12px (matches the audit's
   grab-depth observation).
3. **Parked, named future flow** — tucked eye states as activity display (open/blinking
   on task, closed idle, variants for amber/red). Requires a soul-side "busy" signal
   first (law 7: the body paints, never infers) — not a body-only slice.

The original walk script, for the record:
(see below; H3 brief follows the audit section) launch plainly (no `BB_SKIN`) → full
figure wearing the halo; click E → amber halo + confirm bubble; confirm → green halo;
tuck → sleeping bump with hued halo ring; summon by clicking the bump. Optionally
`BB_SKIN=ring`: tuck → bar, click anywhere along it → summons. Walk pass ⇒ push
`75e8db6..HEAD` (R4 + docs + H1 + H2). **Pushed 2026-07-02 (`6fef913`).**

---

# Slice H3 brief — dock modes + centered half-bar (2026-07-02)

**Builder: Composer 2.5. Self-contained — do not rely on prior session context.**
Authority unchanged: `docs/laminal-ring-pivot.md` incl. §Amendment 2026-07-02; conflicts
stop-and-report. Ground rules, canaries, report format: identical to the H1/H2 section
above. **Scope: `desktop-body/src/main.rs`, `desktop-body/src/render.rs`,
`scripts/bb-body.sh` only. STOP after H3** — no H4 (Customize UI/persistence), no
eyes-as-activity, no soul/TS changes.

**New gate baselines** (post-H2): cargo test **115 passed / 0 failed** (main) + **29**
(parse bin); `cargo build --release` **10 known warnings, nothing new**; `npx tsc
--noEmit` clean; `npx vitest run` **278 / 31** unchanged.

## Why

Owner walk 2026-07-02: tuck appearance is a *preference*, not a skin property. Owner
wants head, bar, or both when tucked, and a shorter, thicker bar. `BB_SKIN` stays "which
render family"; `BB_DOCK` is "what shows when tucked" — orthogonal knobs.

## Design decisions (pinned by lead — do not relitigate, report conflicts)

1. **`enum DockShow { Head, Bar, Both }`**, `#[default] Both`. Not two bools — "never
   neither" is structural, one match arm, not scattered validation.
2. **Parse `BB_DOCK` once at startup** (mirror `env_skin`): `"head"` / `"bar"` /
   `"both"` (trimmed, ascii-lowercased); anything else including unset/`"none"` →
   `Both`. The trap is impossible even from a mis-set env.
3. **`BB_DOCK` applies to the Clay tucked path.** Under `Skin::Ring` every mode coerces
   to bar-only — the ring skin has no head primitive, and coercing (not erroring) keeps
   "never nothing renders" true. One explicit `match` with a comment; do not make dock
   clay-only silently, make the coercion legible.
4. **Bar geometry** (`render.rs`): `BAR_THICKNESS: 8.0 → 12.0` (visual === input; this
   supersedes the audit's "deeper input rect over an 8px bar" note).
   `bar_rect(edge, w, h, along: f32) -> Rect` gains the along-edge anchor: length =
   **half the edge extent** (`const BAR_LENGTH_FRAC: f32 = 0.5`, named), **centered on
   `along`**, clamped to stay fully on-surface. `along` comes from `bump_center`'s
   along-edge coordinate (`render.rs:662`) so bar and head share one anchor and the bar
   reads as the head's underline glow under `Both`.
5. **Paint order under Clay:** `Head` → bump + H2 halo; `Bar` → bar; `Both` → bump +
   halo **then** bar. Hue for the bar: `ring_hue_or_quiet`, unchanged.
6. **Hit-test/input = union of what is painted:** Head ∈ mode → `point_in_bump` /
   `bump_rect`; Bar ∈ mode → `point_in_bar` / `bar_rect`. Input region
   (`main.rs:1872` rects vec) pushes one rect per visible primitive. Paint and hit stay
   single-source (H1's rule) — nothing hittable that isn't visible, nothing visible
   that isn't hittable.
7. **`scripts/bb-body.sh`:** `export BB_DOCK="${BB_DOCK:-both}"` beside the `BB_SKIN`
   export, one-line comment.

## Tests (new, named — plus fix existing bar tests for the new signature)

- `env_dock_parse` — head/bar/both parse; unset, garbage, and `"none"` → `Both`.
- `bar_is_half_length_centered_on_anchor` — bar length ≈ extent/2; center ≈ `along`;
  clamped when the anchor sits near a corner.
- `bar_hit_matches_bar_paint` — click inside the new bar bounds hits; beyond either end
  (still on the edge) misses. (Existing `ring_tuck_full_bar_is_hittable` will need
  updating for the shorter bar — rename/adjust deliberately and say so.)
- `dock_head_only_bump_hits_bar_misses` and `dock_bar_only_bar_hits_bump_misses` —
  every mode leaves at least one live summon target.
- `ring_skin_coerces_dock_to_bar` — under Ring, `DockShow::Head` still paints/hits the
  bar.
- Existing `edge_bar_*` hue tests: sample points must move inside the new bar bounds —
  adjust sampling, not tolerances.

**Commit:** `feat(body): laminal ring pivot — Slice H3 — dock modes (head|bar|both) + centered half-bar`

Append your builder report below this brief, same format as H1/H2. **STOP after H3.**

---

## Builder report — H3 (Composer 2.5, 2026-07-02)

**Status:** H3 complete. **Stopped after H3.** Not pushed.

### Commit

```
bb203b7955ad12dba762a41866f3e8e8afcaa99e
feat(body): laminal ring pivot — Slice H3 — dock modes (head|bar|both) + centered half-bar

DockShow enum (default Both) via BB_DOCK; unset/garbage/none -> Both. Ring skin coerces
every mode to bar-only. bar_rect(edge, w, h, along): 12px thick, half-edge length
(BAR_LENGTH_FRAC), centered on bump_along_edge. Tucked paint/hit union per effective dock.

Tests: cargo test 121 passed (main), +env_dock_parse, +bar_is_half_length_centered_on_anchor,
+bar_hit_matches_bar_paint, ring_tuck_full_bar renamed to ring_tuck_half_bar_is_hittable_within_bounds,
+dock_head_only_bump_hits_bar_misses, +dock_bar_only_bar_hits_bump_misses, +ring_skin_coerces_dock_to_bar.
```

### Gate outputs (verbatim)

**`cd desktop-body && cargo test`** (after `touch src/*.rs`)

```
running 121 tests
test result: ok. 121 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.31s
running 29 tests (parse bin)
test result: ok. 29 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

Baseline was 115 (post-H2); H3 landed +6 named tests (121). Renamed `ring_tuck_full_bar_is_hittable` -> `ring_tuck_half_bar_is_hittable_within_bounds`.

**`cd desktop-body && cargo build --release`**

```
warning: `bb-desktop-body` (bin "bb-desktop-body") generated 10 warnings
    Finished `release` profile [optimized] target(s) in 2.46s
```

10 warnings — known set. Nothing new.

**`npx tsc --noEmit`** — clean (exit 0).

**`npx vitest run`** — **278 passed / 31 files** unchanged.

### Canary greps

| Canary | Result |
|--------|--------|
| No new governance RGBA literals | PASS — bar still uses `ring_hue_or_quiet`; geometry consts `BAR_LENGTH_FRAC`, `BAR_THICKNESS` |
| Figure function bodies byte-identical | PASS — `draw_bump` / `draw_closed_eyes` / `draw_eyes` / `draw_mouth` untouched |
| No new governance display surface | PASS |

### Discoveries

- **Left-edge geometry overlap:** when bar and bump share the same `along` anchor on left/right edges, the half-bar strip can lie entirely inside the bump circle. Dock-mode liveness tests use `BumpEdge::Top` so bar endpoints sit outside the bump — documented in test comments, not a product bug (head-only still summons via bump centre).
- **`tucked_summon_rects` / `point_in_tucked_summon` / `tucked_summon_bounds`** centralize paint/hit/clamp union in `render.rs`; `main.rs` no longer skin-gates per-primitive.
- Ring coercion is explicit in `effective_dock_show` + legible `match` at paint site.

### Conflict stops

None.

### Not done (by design)

H4 (Customize toggles + disk persistence), eyes-as-activity, soul/TS changes, push.

---

## Lead audit — H3 (Fable, 2026-07-02)

**Verdict: PASS.** Cleared to owner walk. Push gated on the walk, as always.

Independently re-ran all four gates with forced recompile (`touch src/*.rs`):

| Gate | Lead result | Matches builder |
|------|-------------|-----------------|
| `cargo test` | 121 + 0 (main) / 29 (parse bin) | ✅ (+6 named tests, rename accounted) |
| `cargo build --release` | 10 warnings, identical known set | ✅ |
| `npx tsc --noEmit` | clean | ✅ |
| `npx vitest run` | 278 / 31 files | ✅ |

Diff review `957b4b0..bb203b7` against the pinned brief decisions — all conform:

- `DockShow { Head, Bar, #[default] Both }`; `env_dock()` parses once at startup, trims + lowercases, `head|bar|both` explicit, everything else (incl. `none`, unset) → `Both`. Never-neither is structural: no code path yields an empty tucked surface.
- `effective_dock_show(Skin::Ring, _) => Bar` — coercion is one named function, asserted by `ring_skin_coerces_dock_to_bar`.
- `bar_rect(edge, w, h, along)`: `BAR_THICKNESS` 8→12, `BAR_LENGTH_FRAC = 0.5`, centered on `bump_along_edge` (shared with `bump_center`), clamped on-surface. `draw_edge_bar` consumes the same rect — visual === input holds.
- Single-source union: `tucked_summon_rects` (input region), `point_in_tucked_summon` (press target), `tucked_summon_bounds` (tuck drag clamp) all live in render.rs; main.rs's old skin-gated `tucked_bump_rect`/`point_in_tucked_bump` pair fully replaced at all three call sites (input region ~:1880, press ~:2400, drag clamp ~:3380).
- Scope exactly the three permitted files. Commit subject verbatim per brief.

Canaries (lead-verified, not just builder-claimed):

- md5 of figure bodies at `957b4b0` vs `bb203b7`: `draw_figure`, `draw_clay_head_at`, `draw_clay_texture`, `draw_eyes`, `draw_closed_eyes`, `draw_mouth`, `draw_bump`, `draw_bump_halo`, `draw_route_boundary_chrome` — **all IDENTICAL**.
- Palette grep over added lines: **zero new color literals**; bar hue still flows through `ring_hue_or_quiet`.

Noted, non-blocking:

- `env_dock_parse` mutates process env (same pattern as `env_skin_defaults_to_clay_without_bb_skin`); standing note applies — serialize, don't delete, if it ever flakes under parallel test threads.
- Left/right-edge bar⊂bump overlap discovery is well-handled (top-edge tests + product behavior unaffected); worth revisiting only if H4 makes bar-only the persisted default.

**Owner walk (pending):** plain launch → figure+halo, tuck → head+bar; `BB_DOCK=head` → bump+halo only; `BB_DOCK=bar` → 12px half-bar only, centered on tuck point, summons; `BB_SKIN=ring` (any dock) → bar-only coercion.

---

## Slice H3.1 — bar centering fix (shrink-don't-slide) + 10px width — brief for Composer

**Status: READY FOR COMPOSER.** Owner walked H3 2026-07-02: all four passes summon correctly — H3 core (union hit-test, dock modes, ring coercion) is ratified. Two geometry findings from the walk; both fixes land in this one small slice. Same ground rules as H1–H3 (scope, gates verbatim with forced recompile, canaries, commit locally, NEVER push, STOP after).

### Finding 1 — left/right bar is off-centre on the head (owner screenshot)

Root cause (lead-diagnosed, confirmed against constants): on `Left`/`Right` the along anchor is `HEAD_CY = 58` (via `bump_center`), near the surface top. `bar_rect` computes `len = extent * BAR_LENGTH_FRAC` (≈280 on a ~560 surface), wants `y = 58 − 140 = −82`, and the current `.clamp(0.0, …)` **slides** the whole bar to `y = 0` — full length preserved, centre at 140, head at 58 → "top heavy off-centre." Top/bottom never show it because `FIG_CX = 280` is mid-surface.

**Pinned fix — shrink, don't slide.** In `bar_rect`, replace the slide-clamp with symmetric shrink:

```rust
let len = (2.0 * along.min(extent - along)).min(extent * BAR_LENGTH_FRAC);
let start = along - len / 2.0;
```

(`extent` = `hf` for Left/Right, `wf` for Top/Bottom.) Invariant becomes: **the bar is always centered on `along`**; length is what yields near a surface end. Safety already holds structurally: `bump_center` clamps the anchor into `[BUMP_R, extent − BUMP_R]`, so `len ≥ 2 × BUMP_R` (68px — never a sliver) and `start ≥ 0`, `start + len ≤ extent` by construction. Do not add a defensive re-clamp that could reintroduce sliding; if you feel one is needed, that is a conflict stop.

### Finding 2 — 12px thickness obscures underlying window controls

`BAR_THICKNESS: 12.0 → 10.0`. Owner suggested 9–10; lead pins 10 (keeps most of the grab-depth win over the original 8px). Visual === input still holds automatically — paint and hit share `bar_rect`.

### Tests

- Adjust `bar_is_half_length_centered_on_anchor`: top/bottom case unchanged (mid anchor → full half-length). The near-edge sub-case now asserts **centre stays on the anchor and length shrinks** (e.g. anchor 40 → len 80, span [0, 80]) instead of asserting a slid full-length rect.
- New named test `bar_shrinks_symmetric_near_edge_left_right`: `Left` edge with realistic dims (extent where `HEAD_CY`-style anchor < len/2), assert `(rect.y + rect.h/2 − along).abs() < 0.5`, `rect.y >= 0`, `rect.y + rect.h <= extent`, `rect.h >= 2.0 * BUMP_R`.
- Existing hue-sampling tests sample bar centres via `bar_rect` already (H3) — they should pass untouched; if one samples a point that shrinkage moved off-bar, adjust the sampling point, never the tolerance.

### Gates (baselines post-H3)

`cargo test` = 121 + 0 (main) / 29 (parse bin) — growth by the one named test only; `cargo build --release` = 10 known warnings; `npx tsc --noEmit` clean; `npx vitest run` = 278 / 31. Forced recompile (`touch src/*.rs`) before cargo gates.

### Scope

`desktop-body/src/render.rs` only (both changes live there). No main.rs, no script changes.

### Commit subject (verbatim)

`fix(body): laminal ring pivot — Slice H3.1 — bar shrinks symmetric on anchor (no slide) + 10px thickness`

Append builder report below, STOP after H3.1.

---

## Builder report — H3.1 (Composer 2.5, 2026-07-02)

**Status:** H3.1 complete. **Stopped after H3.1.** Not pushed.

### Commit

```
457b930a14c20fb48067ba067c111e85b40f8f44
fix(body): laminal ring pivot — Slice H3.1 — bar shrinks symmetric on anchor (no slide) + 10px thickness

Replace bar_rect slide-clamp with symmetric shrink: len = min(2*room_to_nearest_end,
extent*BAR_LENGTH_FRAC), start = along - len/2 — bar centre always tracks the head anchor
on left/right edges. BAR_THICKNESS 12 -> 10 (visual === input via shared bar_rect).

Tests: cargo test 122 passed (main), +bar_shrinks_symmetric_near_edge_left_right;
bar_is_half_length_centered_on_anchor updated for shrink semantics; dock_head_only widened
to W=560 so bar endpoints clear the bump circle.
```

### Gate outputs (verbatim, forced recompile via `touch src/render.rs`)

**`cd desktop-body && cargo test`**

```
running 122 tests
test result: ok. 122 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.33s
running 29 tests (parse bin)
test result: ok. 29 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

Baseline was 121 (post-H3); H3.1 landed +1 named test (`bar_shrinks_symmetric_near_edge_left_right`).

**`cd desktop-body && cargo build --release`** — 10 warnings (known set). Nothing new.

**`npx tsc --noEmit`** — clean.

**`npx vitest run`** — **278 / 31** unchanged.

### Canary greps

| Canary | Result |
|--------|--------|
| Figure bodies byte-identical | PASS — `render.rs` only; no figure function touched |
| No new palette literals | PASS |
| No defensive re-clamp added | PASS — shrink formula only; no slide-clamp |

### Conflict stops

None. No re-clamp added.

### Scope

`desktop-body/src/render.rs` only, per brief.

### Not done

H4, push, main.rs, scripts.

---

## Lead audit — H3.1 + T/B length match (Fable, 2026-07-02)

**Verdict: PASS.** Owner walk already passed (left/right centering ✓, 10px ✓, top/bottom length match ✓). Pushed.

Covers all three commits: `457b930` (briefed slice), `8fff20b` (report), **`902fe43` (owner-requested mid-slice, outside the brief — audited to the same bar)**.

Gates re-run with forced recompile: cargo **123 + 0 / 29** (+1 briefed test, +1 for the T/B match — both named), **10 known warnings**, tsc clean, vitest **278 / 31**. Canaries: zero new color literals; all figure/halo/chrome function bodies md5-identical across `eb0f99d..HEAD`.

Briefed slice conforms: shrink-don't-slide exactly as pinned (`len = min(2·min(along, extent−along), extent·FRAC)`, `start = along − len/2`), **no defensive re-clamp**, thickness 12→10, scope render.rs only.

Unbriefed `902fe43` design review: `tuck_bar_along_length` computes the left/right length with an **infinite cap**, so for L/R edges the cap equals their own length — L/R geometry provably unaffected; only top/bottom get capped to match. No circularity, no slide path reintroduced. `dock_head_only_bump_hits_bar_misses` rewrite (find-an-endpoint-outside-bump) verified sound on its new left-edge dims.

Noted, non-blocking:
- Test name `bar_is_half_length_centered_on_anchor` no longer describes its assertions (it now checks T/B↔L/R match + shrink); rename opportunistically next time that file is open.
- **New owner walk finding (logged for backlog, not H-series):** tucked speech-bubble text is cut off (e.g. `"Edit repository" needs` truncated). Bubble render/positioning works well in the tucked space otherwise. Candidate small slice after H4: tucked bubble wrap/height vs. fixed bubble rect.

**Process note:** owner asked Composer directly for the T/B match mid-slice — fine outcome this time (small, well-tested, audited clean), but preference stands: route scope adds through the lead brief so the audit knows what it's checking before it reads the diff.

---

## Slice H4a — settings persistence (dock + color + size, per-buddy JSON) — brief for Composer

**Status: READY FOR COMPOSER.** Owner ratified 2026-07-02: persist the full current settings set, not dock-only. Lead split H4: **H4a = persistence layer** (this slice — disk I/O is new territory for the body, it gets its own audit+walk), **H4b = Customize "Dock" toggles UI** (brief cut after H4a's walk passes). Same ground rules: gates verbatim with forced recompile, canaries, commit locally, NEVER push, STOP after H4a.

### Scope (amended for this slice)

`desktop-body/src/main.rs`, **new file** `desktop-body/src/settings.rs`, `scripts/bb-body.sh`. **render.rs untouched** — figure canary should be trivially green. No new Cargo deps: `serde_json = "1"` is already in the tree.

### Pinned design

**File:** `<config>/border-buddies/body-settings.json` where `<config>` resolves `BB_CONFIG_DIR` (tests/dev override) → `XDG_CONFIG_HOME` → `~/.config`. One file, keyed per buddy:

```json
{ "forge": { "dock": "both", "color": [201, 109, 60], "body_len": 320.0 } }
```

**Persisted fields (exactly three):** `dock` (`"head"|"bar"|"both"`), `color` (`[u8;3]`), `body_len` (f32, re-clamped to `BODY_LEN_MIN..=BODY_LEN_MAX` on load). **`BB_SKIN` is NOT persisted** — skin stays a dev knob. Buddy identity is the key, not a field.

**Precedence at startup (per field):** explicit env (`BB_DOCK`, `BB_COLOR`/per-buddy color env) → persisted value for this buddy → compiled default. This requires a `bb-body.sh` change: **delete the `export BB_DOCK="${BB_DOCK:-both}"` default line** (added in H3) — a script default makes env always look "set" and would shadow persistence forever. Pass `BB_DOCK` through only if the user set it. Keep `BB_SKIN="${BB_SKIN:-clay}"` as is. Update the bb_log line to print the *resolved* dock source if convenient, else drop dock from it.

**Save-on-change through ONE funnel:** `App::persist_settings(&self)` — serializes this buddy's three fields into the file (read-modify-write so other buddies' entries survive; atomic: write `.tmp` sibling then `rename`). Call sites, exactly these:
- color cycle handler (`self.color = next_color(...)`, main.rs ~:3214)
- size-preset cycle handler (settings panel)
- pointer **release** after a leg/feet drag that changed `body_len` — NOT inside `set_body_len` itself (feet-drag calls it every motion event; one write per drag, not per pixel)
- `dock` has no in-session mutator until H4b — on any save, write the startup-resolved dock value so the file stays complete.

**Failure posture (never crash, never block paint):** missing file / unreadable / garbage JSON / wrong-typed field → that field falls back to default, log one line (existing eprintln/log style), continue. Write failure → log, continue. No retries, no dialogs.

**Law 7:** settings are body-local presentation. No soul messages, no wire changes, nothing in `presence`.

### Tests (named)

In `settings.rs` (pure fns take the dir path / parsed JSON — keep file-path resolution separate from (de)serialization so most tests need no env):
- `settings_roundtrip_per_buddy` — save forge + hermes, reload, both intact
- `settings_missing_file_yields_defaults`
- `settings_garbage_file_yields_defaults` (invalid JSON, and valid JSON with wrong-typed fields)
- `settings_partial_entry_fills_defaults` (entry with only `color` → dock/body_len default)
- `body_len_reclamped_on_load` (out-of-range value in file)
- `env_overrides_persisted_dock` — env-mutating: standing note applies (serialize, don't delete, if it flakes under parallel threads); use `BB_CONFIG_DIR` pointed at a temp dir

### Gates (baselines post-H3.1)

`cargo test` = **123 + 0** (main) / 29 (parse bin) — growth by the named tests above only; `cargo build --release` = **10 known warnings**; `npx tsc --noEmit` clean; `npx vitest run` = **278 / 31**. Forced recompile (`touch src/*.rs`) before cargo gates.

### Owner walk (after Fable audit)

Launch plain → cycle color + size in the settings panel → kill → relaunch plain → **color and size survive**. Drag feet to stretch → relaunch → survives. `BB_DOCK=head` launch → env wins over file. Open `~/.config/border-buddies/body-settings.json` → human-readable, one entry per buddy walked.

### Commit subject (verbatim)

`feat(body): laminal ring pivot — Slice H4a — settings persistence (dock+color+size, per-buddy JSON)`

Append builder report below. **STOP after H4a** — H4b (Dock toggles UI) brief follows the walk.

---

## Builder report — H4a (Composer 2.5, 2026-07-02)

**Status:** H4a complete. **Stopped after H4a.** Not pushed.

### Commit

```
69ac888
feat(body): laminal ring pivot — Slice H4a — settings persistence (dock+color+size, per-buddy JSON)

Per-buddy body-settings.json under ~/.config/border-buddies (BB_CONFIG_DIR override).
Precedence env → persisted → default for dock, colour, body_len. Atomic tmp+rename
writes via App::persist_settings on colour/size cycle and feet-drag release.
Remove BB_DOCK script default in bb-body.sh so persistence is not shadowed.

Tests: cargo test 129 passed (main), +6 settings::*; parse bin 29 unchanged.
```

### Gate outputs (verbatim, forced recompile via `touch src/*.rs`)

**`cd desktop-body && cargo test`**

```
running 129 tests
test result: ok. 129 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.63s
running 29 tests (parse bin)
test result: ok. 29 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
```

Baseline was 123 (post-H3.1+T/B); H4a landed +6 named tests in `settings.rs`.

**`cd desktop-body && cargo build --release`** — 10 warnings (known set). Nothing new.

**`npx tsc --noEmit`** — clean.

**`npx vitest run`** — **278 / 31** unchanged.

### Canary greps

| Canary | Result |
|--------|--------|
| `render.rs` untouched | PASS — figure-freeze trivially green |
| No soul/wire/presence changes | PASS — law 7 body-local only |
| No new Cargo deps | PASS — `serde_json` only |

### Conflict stops

None.

### Scope

`desktop-body/src/settings.rs` (new), `desktop-body/src/main.rs`, `scripts/bb-body.sh`. `render.rs` untouched.

### Implementation notes

- `settings::resolve_startup` applies env → persisted → default per field; `dock_from_env_value` preserves pre-H4a garbage/`none` → `Both` semantics when `BB_DOCK` is explicitly set.
- `App::persist_settings` called from `cycle_color`, `cycle_size`, and feet-drag release (`PressTarget::Feet` + `dist > CLICK_SLOP`) — not inside `set_body_len`.
- `bb-body.sh`: removed `export BB_DOCK="${BB_DOCK:-both}"`; log line no longer prints dock (resolved at runtime from file/env).

### Not done

H4b (Dock toggles UI), push.

---

## Lead audit — H4a (Fable, 2026-07-02) — **PASS**

Audited `69ac888` (+ builder report `d163d90`). Owner walk **PASSED** same evening: colour/kill/persist ✅, stretch/kill/persist ✅. Persisted file inspected on the walk machine — human-readable, one entry per buddy walked, feet-dragged `body_len` and cycled colour recorded exactly as briefed.

### Gates (lead re-run, forced recompile)

| Gate | Result |
|------|--------|
| `cargo test` | **129 + 0** (main) / 29 (parse bin) — baseline 123 + exactly the 6 named `settings::` tests |
| `cargo build --release` | **10 warnings** — known set (one line changed content, see note) |
| `npx tsc --noEmit` | clean |
| `npx vitest run` | **278 / 31** |

### Brief conformance

- Scope exact: `settings.rs` (new), `main.rs`, `bb-body.sh`. **render.rs untouched** → figure canary trivially green. No new deps (`serde_json` only). Commit subject verbatim.
- Precedence env → persisted → default per field via `resolve_startup`; `BB_COLOR` + per-buddy colour env both honoured as briefed; `BB_SKIN` not persisted.
- **One funnel** `App::persist_settings` at exactly the briefed call sites: colour cycle, size cycle, feet-drag **release** (`PressTarget::Feet` + `dist > CLICK_SLOP` early-return — verified behaviorally identical to the old fall-through, one write per drag not per pixel). Not inside `set_body_len`. Dock written from startup-resolved value pending H4b.
- `bb-body.sh` `BB_DOCK:-both` default **deleted**; BB_DOCK now passes through only when user-set. `BB_SKIN` default kept.
- Atomic write (tmp + rename, parent `create_dir_all`); read-modify-write preserves other buddies' entries; never-crash posture on missing/garbage/wrong-typed (per-field fallback + one eprintln); write failure logged, paint never blocked.
- Env-mutating test uses the standing serialize pattern (`ENV_LOCK` mutex + save/restore); old `env_dock_parse` test correctly retargeted at `settings::parse_dock_str`/`dock_from_env_value`.
- Builder-flagged semantic ratified: explicitly-set-but-garbage `BB_DOCK` → `Both` (overrides persisted) — preserves H3's never-neither contract; acceptable, revisit only if a user complains a typo'd env hid their persisted dock.

### Non-blocking notes

1. `render::Layout::initial` is now dead (startup height computed from `Layout { facing, body_len }` directly) — it merged into the existing `interior_rows` never-used warning, count still 10 but the line's content changed. Cleanup candidate alongside the stale `bar_is_half_length_centered_on_anchor` rename.
2. `buddy_env_key` now exists in both `main.rs` and `settings.rs` — harmless duplication, fold when convenient.
3. `settings.rs` missing trailing newline.
4. Concurrent-writer read-modify-write race (two buddy processes saving at once could drop each other's entry) — single-buddy reality today, becomes real if multi-buddy launch lands; note for then.

**Verdict: PASS. Pushing `69ac888..` + this audit. H4b brief follows below.**

---

## Slice H4b — Customize "Dock" row (head/bar/both cycle, persisted) — brief for Composer

**Status: READY FOR COMPOSER.** H4a walked + pushed. H4b gives dock its first in-session mutator: a row in the body-local settings panel, cycling like Colour/Size, written through the existing `persist_settings()` funnel. Same ground rules: gates verbatim with forced recompile, canaries, commit locally, NEVER push, STOP after H4b.

### Scope

**`desktop-body/src/main.rs` ONLY.** render.rs untouched (SettingsRow is already generic label/value/editable — no new drawing). settings.rs untouched (`persist_settings` already writes `self.dock_show`). No script changes, no new deps.

### Pinned design

- **Row placement:** insert `("Dock", …, editable)` at **index 2** in the `settings_data` vec (main.rs ~:1502) — after Size, keeping the editable body-local cluster (Colour, Size, Dock) above the read-only governance/identity rows (Posture → idx 3, Buddy → idx 4). `settings_row_count()` 4 → **5**; update its doc comment and the `on_settings_row` match arms to the shifted indices.
- **Value label:** `Both → "Head + bar"`, `Head → "Head"`, `Bar → "Bar"` — small pure fn `dock_label(DockShow) -> &'static str` next to `size_preset_name` style.
- **Cycle handler** `cycle_dock(&mut self)`: pure fn `next_dock(DockShow) -> DockShow` with order **Both → Head → Bar → Both**; handler sets `self.dock_show`, speech `format!("Dock: {}", dock_label(..))`, `update_input_region()`, `persist_settings()` — mirror `cycle_color` exactly.
- **Ring-skin coercion (conflict point, pinned):** under `Skin::Ring`, `effective_dock_show` coerces to Bar regardless — cycling would silently do nothing visible. So under ring skin the Dock row is **read-only** (`editable: false`, same as Posture/Buddy) and tapping it speaks `"Dock is bar-only under the ring skin."` — no mutation, no persist. Clay (the default) gets the full cycle.
- **No tuck special-case needed:** the settings panel lives in the torso, so cycling always happens untucked; the H3 union helpers read `self.dock_show` at paint/hit time, so the new value simply applies on the next tuck. Do not add re-tuck logic.
- **Env note (no code):** if `BB_DOCK` was set at launch, the session started from env; cycling still mutates + persists normally, and the next plain launch uses the persisted value while an env launch still wins. That is the intended H4a precedence — no special handling.
- **Law 7:** dock is body-local presentation. No soul messages, no wire changes.

### Tests (named)

- `dock_cycle_order` — Both→Head→Bar→Both closes the loop, all variants reachable
- `dock_label_covers_all_variants` — three labels, no empty string

(Row-tap dispatch needs a live App/Wayland — walk covers it; keep tests to the pure fns like `next_color`/`next_size` precedent.)

### Gates (baselines post-H4a)

`cargo test` = **129 + 0** (main) / 29 (parse bin) — growth by the 2 named tests only; `cargo build --release` = **10 known warnings** (note: the `initial`/`interior_rows` never-used line is one warning); `npx tsc --noEmit` clean; `npx vitest run` = **278 / 31**. Forced recompile (`touch src/*.rs`) before cargo gates.

### Owner walk (after Fable audit)

Launch plain → Customize → Dock row reads "Head + bar" → tap → "Head" → tuck to an edge → **head only, no bar** → untuck, tap to "Bar" → tuck → **bar only** → kill → relaunch plain → Dock survives as "Bar" → `BB_DOCK=both` launch → env wins ("Head + bar"). Optional: `BB_SKIN=ring` launch → Dock row greyed, tap explains bar-only.

### Commit subject (verbatim)

`feat(body): laminal ring pivot — Slice H4b — Dock setting row (head/bar/both cycle, persisted)`

Append builder report below. **STOP after H4b.** (Backlog after: tucked bubble text cut-off — small render.rs slice, will also carry the stale `bar_is_half_length_centered_on_anchor` rename and the `Layout::initial` orphan cleanup.)

---

## Builder report — H4b (Composer 2.5, 2026-07-02)

**Status:** H4b complete. **Stopped after H4b.** Not pushed.

### Commit

```
feat(body): laminal ring pivot — Slice H4b — Dock setting row (head/bar/both cycle, persisted)

Dock row at Customize index 2 cycles Both→Head→Bar through persist_settings.
Ring skin makes the row read-only with an explanatory tap message.

Tests: cargo test 131 passed (main), +dock_cycle_order, +dock_label_covers_all_variants.
```

### Gate outputs (verbatim, forced recompile via `touch src/*.rs`)

**`cd desktop-body && cargo test`**

```
running 131 tests
test result: ok. 131 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.33s
running 29 tests (parse bin)
test result: ok. 29 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

Baseline was 129 (post-H4a); H4b landed +2 named tests.

**`cd desktop-body && cargo build --release`** — 10 warnings (known set). Nothing new.

**`npx tsc --noEmit`** — clean.

**`npx vitest run`** — **278 / 31** unchanged.

### Canary greps

| Canary | Result |
|--------|--------|
| `render.rs` untouched | PASS — SettingsRow already generic |
| `settings.rs` untouched | PASS — `persist_settings` already writes `dock_show` |
| No soul/wire/presence changes | PASS — law 7 body-local only |

### Conflict stops

None.

### Scope

`desktop-body/src/main.rs` only, per brief.

### Implementation notes

- `settings_data` inserts Dock at index 2; `settings_row_count` 4 → 5; Posture/Buddy shifted to indices 3/4.
- `dock_label` / `next_dock` pure fns; `cycle_dock` mirrors `cycle_color` (speech + `update_input_region` + `persist_settings`).
- Ring skin: Dock row `editable: false`; tap speaks `"Dock is bar-only under the ring skin."` — no mutation, no persist.

### Not done

Push, tucked bubble backlog slice.

---

## Lead audit — H4b (Fable, 2026-07-02) — **PASS (pending owner walk)**

Audited `c83a038` (+ builder report `4b60bc5`).

### Gates (lead re-run, forced recompile)

| Gate | Result |
|------|--------|
| `cargo test` | **131 + 0** (main) / 29 (parse bin) — baseline 129 + exactly `dock_cycle_order` + `dock_label_covers_all_variants` |
| `cargo build --release` | **10 warnings** — known set, unchanged |
| `npx tsc --noEmit` | clean |
| `npx vitest run` | **278 / 31** |

### Brief conformance

- Scope exact: **main.rs only** — render.rs and settings.rs untouched (figure + settings canaries trivially green). Commit subject verbatim.
- Dock row at index 2 in `settings_data`, editable flag `!matches!(self.skin, Skin::Ring)`; Posture → 3, Buddy → 4, `settings_row_count` 4 → 5; all doc comments updated to match.
- `cycle_dock` mirrors `cycle_color` exactly: `next_dock` (Both→Head→Bar→Both), speech via `dock_label`, `update_input_region()`, `persist_settings()`.
- Ring-skin tap: read-only, speaks "Dock is bar-only under the ring skin.", **no mutation, no persist** — as pinned.
- Labels exact: "Head + bar" / "Head" / "Bar". No tuck special-casing added (correctly none).
- Law 7 clean: no soul/wire/presence changes.

### Non-blocking notes

None — cleanest slice of the series.

**Verdict: PASS. Push held for owner walk** (Customize → Dock cycles → tuck shows head-only / bar-only per setting → relaunch persists → BB_DOCK env wins → optional ring-skin greyed row).

**Owner walk PASSED 2026-07-03 (native, COSMIC):** dock cycle + tuck rendering per mode ✅, persistence across kill/relaunch ✅, ring launch bar-only with greyed row + explanatory tap ✅. **H-series COMPLETE — pushed.**

---

## Slice H5 — tucked bubble text fits (3 lines + honest ellipsis) + render tidy — brief for Composer

**Status: READY FOR COMPOSER.** H-series product arc is complete; H5 is the polish slice closing the owner-walk finding from H3.1 (tucked speech cut mid-sentence, e.g. `"Edit repository" needs`). Same ground rules: gates verbatim with forced recompile, canaries, commit locally, NEVER push, STOP after H5.

### Scope

**`desktop-body/src/render.rs` ONLY.** No main.rs, no settings.rs, no scripts, no new deps.

### Root cause (lead-diagnosed, verified against source)

`draw_tucked_bubble` (render.rs ~:3683) wraps into the fixed `tucked_bubble_rect`: `max_lines = floor((TUCK_PEEK_BUBBLE_H − pad_top − 6) / LINE_H)` = `floor((60 − 18 − 6) / 20.8)` = **1 line** (~20 chars at 188px text width). And `wrap()` (~:3850) only appends `…` when the final line overflows in **width**; when the **line budget** is exhausted with input remaining, the tail is dropped silently. Two defects, two fixes.

### Pinned design

1. **Bubble grows to a 3-line budget, still fixed-size:** `TUCK_PEEK_BUBBLE_H` 60 → **88** (= pad_top 18 + 3×LINE_H 62.4 + bottom 6, rounded up). Nothing else about the peek geometry changes — `tucked_bubble_rect` stays the single source of truth for paint AND hit-test/input-region (the pure-geometry, no-font contract in the comment block at ~:855 is load-bearing; do NOT make the height text-dependent). `tucked_peek_origin`'s existing clamps absorb the taller group on every edge.
2. **Honest truncation in `wrap()` itself:** when the loop stops because `lines.len() == max_lines` while input remains unconsumed, ellipsize the final line (trim chars until `…` fits `max_w`, same trim loop as the existing width-overflow path — factor it, don't duplicate). This is a shared fn (~12 call sites: chat bubble, panels, cards, pinned) — the behavior change is strictly "silently dropped tail → visible `…`", which is the desired posture everywhere. Callers passing `usize::MAX` are unaffected by construction.
3. **Rider — stale test rename:** `bar_is_half_length_centered_on_anchor` (~:4925) no longer asserts half-length; rename to `bar_full_length_when_anchor_clear_of_edges` (or closer to what it actually asserts — read it first), body unchanged unless the name-lie extends to a stale comment.
4. **Rider — dead-code tidy:** `Layout::initial` and `Layout::interior_rows` are test-only callers now (render.rs:4664 etc., main.rs:4131 is inside `mod tests`). Gate BOTH with `#[cfg(test)]` — do not delete. This removes one known release warning: **expected release warning count drops 10 → 9.** If cfg-gating breaks a non-test caller you find, that's a conflict stop, not a workaround.

### Tests (named)

- `wrap_ellipsizes_when_line_budget_exhausted` — long text, small max_lines → last line ends `…`, fits max_w
- `wrap_unlimited_budget_never_ellipsizes` — same text, `usize::MAX` → no `…`, all content present
- `tucked_bubble_budget_is_three_lines` — the max_lines expression at the new height = 3 (compute the same way `draw_tucked_bubble` does; keep them from drifting)

### Canaries (render.rs IS touched this slice — figure canary is live, not trivial)

Figure function set (`draw_figure`, `draw_clay_head_at`, `draw_clay_texture`, `draw_eyes`, `draw_closed_eyes`, `draw_mouth`, `draw_bump`, `draw_bump_halo`, `draw_route_boundary_chrome`) byte-identical. Zero new color literals. No soul/wire/presence changes.

### Gates (baselines post-H4b)

`cargo test` = **131 + 0** (main) / 29 (parse bin) — growth by the 3 named tests only (rename is count-neutral); `cargo build --release` = **9 warnings** after the tidy (was 10); `npx tsc --noEmit` clean; `npx vitest run` = **278 / 31**. Forced recompile (`touch src/*.rs`) before cargo gates.

### Owner walk (after Fable audit)

Tuck a buddy → trigger speech longer than one line (e.g. tap the Edit interior row for the repo-edit prompt) → bubble shows up to 3 wrapped lines; if still longer, last line ends in a visible `…` — never a silent mid-sentence cut. Quick regression: untucked chat bubble and Customize panel still render sanely (shared `wrap()`).

### Commit subject (verbatim)

`fix(body): laminal ring pivot — Slice H5 — tucked bubble 3-line budget + honest wrap ellipsis, render tidy`

Append builder report below. **STOP after H5.**

---

## Builder report — H5 (Composer 2.5, 2026-07-02)

**Status:** H5 complete. **Stopped after H5.** Not pushed.

### Commit

```
fix(body): laminal ring pivot — Slice H5 — tucked bubble 3-line budget + honest wrap ellipsis, render tidy

TUCK_PEEK_BUBBLE_H 60→88 for a three-line tucked bubble budget. wrap() ellipsizes
when the line budget is exhausted, not only on width overflow. Rename stale bar
test; #[cfg(test)] on Layout::initial and interior_rows (release warnings 10→9).

Tests: cargo test 134 passed (main), +wrap_ellipsizes_when_line_budget_exhausted,
+wrap_unlimited_budget_never_ellipsizes, +tucked_bubble_budget_is_three_lines.
```

### Gate outputs (verbatim, forced recompile via `touch src/*.rs`)

**`cd desktop-body && cargo test`**

```
running 134 tests
test result: ok. 134 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.32s
running 29 tests (parse bin)
test result: ok. 29 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

Baseline was 131 (post-H4b); H5 landed +3 named tests (rename count-neutral).

**`cd desktop-body && cargo build --release`** — **9 warnings** (was 10; `Layout::initial`/`interior_rows` cfg-gated as briefed).

**`npx tsc --noEmit`** — clean.

**`npx vitest run`** — **278 / 31** unchanged.

### Canary greps

| Canary | Result |
|--------|--------|
| Figure function set byte-identical | PASS — diff touches only wrap, TUCK_PEEK_BUBBLE_H, Layout cfg, tests |
| Zero new color literals | PASS |
| No soul/wire/presence changes | PASS |

### Conflict stops

None.

### Scope

`desktop-body/src/render.rs` only, per brief.

### Implementation notes

- `truncated` flag threaded through `wrap` / `push_wrapped_line` / `hard_wrap_current`; `ellipsize_line_in_place` factors the shared trim-until-`…`-fits loop.
- `tucked_bubble_rect` unchanged as geometry source of truth — height bump is constant-only.

### Not done

Push.

---

## Lead audit — H5 (Fable, 2026-07-03) — **PASS (pending owner walk)**

Audited `e678892` (+ builder report `1a79950`).

### Gates (lead re-run, forced recompile)

| Gate | Result |
|------|--------|
| `cargo test` | **134 + 0** (main) / 29 (parse bin) — baseline 131 + exactly the 3 named wrap/bubble tests (rename count-neutral) |
| `cargo build --release` | **9 warnings** — 10 → 9 as briefed (`initial`/`interior_rows` line gone) |
| `npx tsc --noEmit` | clean |
| `npx vitest run` | **278 / 31** |

### Canaries (LIVE this slice — render.rs touched)

All 9 protected figure functions **md5-identical** across `e678892^..e678892` (`draw_figure`, `draw_clay_head_at`, `draw_clay_texture`, `draw_eyes`, `draw_closed_eyes`, `draw_mouth`, `draw_bump`, `draw_bump_halo`, `draw_route_boundary_chrome`). **Zero** color literals in added lines. No soul/wire/presence changes.

### Brief conformance

- Scope exact: render.rs only. Commit subject verbatim.
- `TUCK_PEEK_BUBBLE_H` 60 → 88; peek geometry otherwise untouched — `tucked_bubble_rect` remains sole geometry source, no font-dependence introduced.
- `wrap()` truncation flag threaded through every early exit; `ellipsize_line_in_place` factors the trim loop (no duplication). Traced the hard-wrap branch: `carry_rev` is non-empty whenever the caller's budget-break fires, so no false ellipsis there. `usize::MAX` callers unaffected by construction, confirmed by `wrap_unlimited_budget_never_ellipsizes`.
- Rename `bar_full_length_when_anchor_clear_of_edges` — body identical. `#[cfg(test)]` on `Layout::initial` + `Layout::interior_rows`, both compile under test (134 include their callers).

### Non-blocking notes

1. Trailing-`\n` landing exactly at the line budget sets `truncated` with nothing following → cosmetic false `…`. Unreachable for realistic speech strings; note only.
2. `tucked_bubble_budget_is_three_lines` re-states `pad_top = 18.0` rather than sharing a constant with `draw_tucked_bubble` — drift-prone pair, acceptable per brief ("compute the same way").

**Verdict: PASS. Push held for owner walk** (tuck → long speech shows up to 3 lines, honest `…` if longer; quick untucked chat/Customize regression for the shared `wrap()`).

**Owner walk PASSED 2026-07-03 (native, COSMIC):** tucked bubble renders the full three-line speech (blocked-effector message complete, no mid-sentence cut) ✅; untucked chat bubble + torso text output regression clean (shared `wrap()`) ✅. **H5 COMPLETE — pushed. Tucked state polish done.**
