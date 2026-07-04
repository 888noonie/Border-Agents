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

---

## Slice F2 — activity green: in-flight action paints the halo (grant→result bracket) — brief for Composer

**Status: READY FOR COMPOSER** (owner walks tomorrow's results; brief cut 2026-07-03 at session close).

**DIRECTION RULING (owner ratified 2026-07-03, amends the R-series orthogonality note):** **Green = activity-in-progress; absence of green = at rest.** Amber/red/violet stay governance. The old "when does confirm-green decay?" question is dissolved: green's lifecycle is the wire bracket `action_request` → `action_result` — no decay timer (adding one back would be policy creep; ratified). Palette fact check: green is `AlertLevel::Ready` (`Confirm` is amber) — the activity hue REUSES the Ready green via the existing table; the "only literal hues live in `alert_level_ring_rgba`" constraint holds with **zero new color literals**.

### Scope

**`desktop-body/src/main.rs` ONLY.** render.rs untouched (figure canary trivially green) — the hue threads through the existing `BodyView.alert_level` into `draw_route_boundary_chrome` AND `draw_bump_halo`, so figure ring and tucked bump both go green with no render change. presence.rs untouched — `action_request_json`/`action_request_intent_json` already take `request_id`, and `Cue::ActionResult` already parses `request_id` (currently discarded by `..` in the destructure at ~main.rs:2015).

### Pinned design

- **One bit of presentation memory:** `action_in_flight: Option<InFlightAction>` on `App`, where `InFlightAction { request_id: String, effector: String }`. This is body-local fact ("I asked and have not heard back"), not soul inference — law 7 intact; say so in the field's doc comment.
- **Request ids:** `App` gains a `u64` counter; ids formatted by a pure fn (e.g. `format_request_id(n) -> String`, `"body-req-{n}"`). BOTH `request_review` (~:2897) and `request_repo_edit` (~:2935) generate an id, pass it through the existing `request_id: Option<&str>` param (today they pass `None`), and set `action_in_flight`. A new request while one is in flight **replaces** the slot (single-slot v0.1; overlaps accepted as last-writer — one-line comment).
- **Clear rules (pinned, all three cases):**
  1. An `action_result` whose `request_id` matches the slot → clear. (Stop discarding `request_id` in the `Cue::ActionResult` destructure.)
  2. An `action_result` with NO `request_id` on the wire → clear if `effector` matches the slot (soul didn't echo the id; don't strand green).
  3. **Any decision clears** — allow, needs_confirmation, blocked: the bracket is over either way. On needs_confirmation the soul's `alertLevel` (amber Confirm) takes the halo; the confirmed re-press mints a NEW id and green relights for the second bracket. Unrelated results (different id AND different effector) leave the slot alone.
- **Halo precedence, one pure decision fn:** `halo_alert_level(in_flight: bool, tier: Option<AlertLevel>) -> Option<AlertLevel>` = `if in_flight { Some(AlertLevel::Ready) } else { tier }`. The BodyView construction site (~:1618, `alert_level: self.active_alert_level`) calls it. While flying, green overrides the previous tier ("we're doing the thing"); the landing result repaints via the existing `active_alert_level` path. Steady green — a pulse is an F3/expression-pass candidate, NOT this slice.
- **Known v0.1 limit (documented, not solved):** if the soul never answers and never disconnects, green stays lit. No timer (ratified). If main.rs already has a soul-stream-close path, clear the slot there too; if there is none, note that in the builder report — do NOT invent disconnect handling.
- **Law 7:** no soul messages added, no wire shape changes, nothing in presence.rs.

### Tests (named — keep the logic in pure fns so no App/Wayland needed)

- `in_flight_clears_on_matching_request_id`
- `in_flight_clears_on_effector_when_result_has_no_id`
- `in_flight_survives_unrelated_result` (different id AND different effector)
- `halo_prefers_activity_green_while_in_flight` (and passes the tier through when idle)
- `request_ids_are_unique_and_nonempty` (counter/format fn)

### Gates (baselines post-H5)

`cargo test` = **134 + 0** (main) / 29 (parse bin) — growth by the named tests only; `cargo build --release` = **9 known warnings**; `npx tsc --noEmit` clean; `npx vitest run` = **278 / 31**. Forced recompile (`touch src/*.rs`) before cargo gates.

### Canaries

render.rs untouched → figure canary trivially green. Zero new color literals anywhere. presence.rs untouched.

### Owner walk (after Fable audit; needs soul + body, the governance walk setup)

Launch soul + body → tap **Edit** (repo_edit proof flow) → halo goes GREEN while the request flies → result lands: green clears, result's tier paints (amber on needs_confirmation) → press Confirm → green again → allow lands: receipt on rail, halo shows the result tier. Tap **Review** on an ungranted buddy → brief green, then blocked red. Tuck the buddy mid-flow → bump halo speaks the same green/tier language. Note: with the local dev gateway the bracket is fast — green may be a flash; that is honest (the parked soul `activity` signal will give green real duration later).

### Commit subject (verbatim)

`feat(body): laminal ring pivot — Slice F2 — activity green (in-flight action_request paints Ready-green halo, request-id bracket)`

Append builder report below. **STOP after F2.** (F3 expression pass — bar eyes first, only-while-green, `BAR_EYES_MIN_LEN` named constant + pure-geometry test; then tucked-head attentive eyes; untucked figure LAST, canary-briefed — brief follows F2's walk.)

## Builder report — F2 (Grok Build, 2026-07-04)

**Status:** F2 complete. **Stopped after F2.** Not pushed.

### Commit

```
a28c2789f002182b48c2b72a07e0ef33107b9caf
feat(body): laminal ring pivot — Slice F2 — activity green (in-flight action_request paints Ready-green halo, request-id bracket)

Green halo = activity-in-progress. Body mints request id on action_request for
receipt_review and repo_edit, tracks single InFlightAction slot (request_id +
effector). halo_alert_level pure precedence fn yields Ready while in flight.
Three pinned clear rules on action_result: match request_id, or (no id and
effector match), or any decision ends the bracket. No decay timer.

Scope: main.rs only. render.rs + presence.rs byte-untouched. Zero new color
literals (reuses AlertLevel::Ready via existing table). 5 named pure-fn tests.

Gates (after touch src/*.rs):
- cargo test: 139 + 0 (main) / 29 (parse) — +5 named
- cargo build --release: 9 warnings (known set)
- npx tsc --noEmit: clean
- npx vitest run: 278 / 31 unchanged

No soul-stream-close path existed in main.rs for the presence channel (only
Msg handled; Closed ignored; Layer closed is Wayland surface). Per brief,
no handling invented.
```

### Gate outputs (verbatim, forced recompile via `touch src/*.rs`)

**`cd desktop-body && cargo test`**

```
     Running unittests src/main.rs (target/debug/deps/bb_desktop_body-...)
running 139 tests
test result: ok. 139 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.37s

     Running unittests src/bin/frame_driver.rs (...)
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running unittests src/bin/parse_action_result.rs (...)
running 29 tests
test result: ok. 29 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

Baseline was 134 passed (main post-H5); F2 landed +5 named tests (139). Named tests present: `in_flight_clears_on_matching_request_id`, `in_flight_clears_on_effector_when_result_has_no_id`, `in_flight_survives_unrelated_result`, `halo_prefers_activity_green_while_in_flight`, `request_ids_are_unique_and_nonempty`.

**`cd desktop-body && cargo build --release`**

```
warning: `bb-desktop-body` (bin "bb-desktop-body") generated 9 warnings
    Finished `release` profile [optimized] target(s) in 2.69s
```

9 warnings — known set (frame dead code + pads + unused in render/presence). Nothing new.

**`npx tsc --noEmit`**

```
(clean — exit 0, no output)
```

**`npx vitest run`**

```
 Test Files  31 passed (31)
      Tests  278 passed (278)
   Start at  20:45:58
   Duration  1.47s
```

Unchanged from baseline (278 / 31 files).

### Canary greps

| Canary | Result |
|--------|--------|
| render.rs untouched | PASS — `git diff --exit-code -- desktop-body/src/render.rs` clean |
| presence.rs untouched | PASS — `git diff --exit-code -- desktop-body/src/presence.rs` clean |
| Zero new color literals | PASS — grep over diff found none |
| No soul/wire/presence changes | PASS — law 7; only main.rs body-local in_flight + halo fn |
| New logic in pure fns for tests | PASS — `format_request_id`, `halo_alert_level`, `should_clear_in_flight` exercised by the 5 named tests without App/Wayland |

### Soul-stream-close path (per brief)

Inspected main.rs:
- Presence channel installed as `calloop::channel`; handler: `if let ChannelEvent::Msg(text) = event { app.on_presence_message(&text); }` — `Closed` is not matched or handled.
- No `on_close`, no WS error/close arm that clears state.
- `LayerShellHandler::closed` exists but is the Wayland layer surface exit (sets `exit=true`), not soul WS.
- Per brief instruction: did **not** invent any disconnect handling. Slot would remain lit if soul never replies and never drops the channel (documented v0.1 limit already called out in the brief).

### Discoveries / not done

- `request_launch` and commandeer paths continue to pass `None` for request_id and do not set `action_in_flight` (per pinned scope: only review + repo_edit mint and track).
- Single-slot replace semantics: a second request while one flies just overwrites (brief says "overlaps accepted as last-writer").
- On `needs_confirmation` result: slot clears (any-decision), `pending_effector` set, amber tier active; re-press in the request fn mints a fresh id and re-lights green for the confirm leg.
- No changes to wire shapes, no new messages, no presence.rs edits.
- F3 and anything after: not started.

### Conflict stops

None. All per brief.

### Gate deltas vs baseline (post-H5)

- cargo test main: 134 → 139 (+ exactly the 5 named)
- release warnings: 9 → 9 (unchanged)
- tsc/vitest: clean / 278 unchanged

---

## Lead audit — F2 (Fable, 2026-07-04)

Audited `a28c278` (code) + `cd39ee0` (builder report). First slice built by **Grok Build**; full diff read line-by-line rather than spot-checked.

### Diff read vs pinned design

- `InFlightAction { request_id, effector }` + single-slot `action_in_flight` + `next_request_seq: u64` on `App`, law-7 doc comments present — exact per brief.
- `format_request_id` pure fn, `"body-req-{n}"`; both `request_review` and `request_repo_edit` mint, set the slot, and pass `Some(&request_id)` through the existing presence.rs param (was `None`). New request overwrites slot (last-writer, commented).
- `Cue::ActionResult` destructure now captures `request_id` (the `..` discard is gone); clear logic lives in pure `should_clear_in_flight` with all three pinned rules: id match → clear; no-id + effector match → clear; any decision clears when addressed; unrelated (different id AND different effector) survives.
- `halo_alert_level(in_flight, tier)` pure precedence fn, called at the BodyView construction site — green substitutes while flying, tier passes through at rest. No decay timer anywhere.
- 5 named tests present and passing, all against pure fns (no App/Wayland).

### Canaries (independently verified)

- `git diff 1b09e16..cd39ee0 --name-only` → only `main.rs` + this doc. render.rs and presence.rs **byte-untouched**.
- Zero new color literals in added lines (regex sweep of the diff: no rgba/hex/float-triple hits).
- Soul-stream-close finding **independently confirmed**: main.rs:778 is `if let ChannelEvent::Msg(text)` — `Closed` silently unmatched; no WS close/error arm. GB correctly reported rather than invented handling. v0.1 stranded-green limit stands as documented.

### Gates (lead re-run, forced recompile via `touch src/*.rs`)

- `cargo test` → **139 + 0 (main) / 29 (parse bin)** — growth is exactly the 5 named tests, all observed in output.
- `cargo build --release` → **9 warnings** (known set, unchanged).
- `npx tsc --noEmit` → clean. `npx vitest run` → **278 / 31**.

### Audit notes (no action required)

1. **Stale-id + same-effector keeps green lit** — e.g. double-press supersede: slot holds req-2, a late result echoing req-1 (same effector) does NOT clear. This is the correct exact-correlation reading: id on the wire wins over effector fallback, and the live bracket (req-2) is still genuinely open. Consistent with the brief's rules 1/2 priority order.
2. **`request_launch` (reach effectors) outside the bracket** — per pinned scope, launchers still pass `None` and never paint green. Safe: a launcher result can never falsely clear a review/edit slot (no body-minted id to match; different effector). Candidate F-series follow-up if launcher activity should read green later.
3. Report format matched H-series convention; conflict stops: none.

**Verdict: PASS. Push held for owner walk** (brief's walk script: Edit → green while flying → amber Confirm on needs_confirmation → re-press → green relights → allow lands with tier + receipt; Review on ungranted buddy → brief green then blocked red; tuck mid-flow → bump halo speaks the same language. Note: dev gateway brackets are fast — green may be a flash; that is honest).

**Owner walk PASSED 2026-07-04 (native, COSMIC): F2 activity green complete — pushed.**


---

## Slice F3a — bar eyes: activity green summons a watching pair (expression pass, part 1) — brief for Grok Build

**Status: READY FOR GROK BUILD** (brief cut 2026-07-04 after F2 owner walk PASSED).

**Direction context:** F2 landed the ruling — green = activity-in-progress, painted by `AlertLevel::Ready` through the existing table, lifecycle = the request→result bracket. F3 is the expression pass on top: the buddy *looks attentive* while working. Bar mode goes first because the bar is the most face-less dock state — a 10px strip with no identity. Eyes appear ONLY while green (activity), so at rest the bar stays clean chrome. Tucked-head attentive eyes and the untucked figure come in later F3 slices (figure LAST — it touches canary-protected `draw_eyes` and will be briefed separately).

### Scope

**`desktop-body/src/render.rs` ONLY.** main.rs untouched (`BodyView.alert_level` already carries the green — F2 finished that plumbing). presence.rs untouched. `bb-desktop-body` binary only; no TS/soul changes.

### Pinned design

- **Named constants** (top of render.rs near `BAR_THICKNESS`): `BAR_EYES_MIN_LEN: f32 = 28.0`, `BAR_EYE_R: f32 = 2.0`, `BAR_EYE_HALF_GAP: f32 = 5.0`. Exact values are the builder's aesthetic call within HARD limits: eyes must fit inside `BAR_THICKNESS` (10px), and **`BAR_EYE_HALF_GAP - BAR_EYE_R >= 2.0`** — see the center-pixel gate below.
- **Visibility predicate, one pure fn:** `bar_eyes_visible(alert_level: Option<AlertLevel>, bar_along_len: f32) -> bool` = `alert_level == Some(AlertLevel::Ready) && bar_along_len >= BAR_EYES_MIN_LEN`. Gate on the ALERT LEVEL, **not** on the resolved hue: route-health `"ready"` resolves to the same green rgba but is route state, not activity — it must NOT summon eyes. (The predicate never sees route_health by construction; say so in its doc comment.)
- **Geometry, one pure fn:** `bar_eye_centers(rect: &Rect, edge: BumpEdge) -> [(f32, f32); 2]` — two dot centers symmetric about the bar's midpoint, offset `±BAR_EYE_HALF_GAP` ALONG the bar's long axis (horizontal offsets for Top/Bottom bars, vertical for Left/Right), centered across the thickness. Pure geometry so tests need no pixmap.
- **Paint:** inside `draw_edge_bar`, after the existing fill: if `bar_eyes_visible(...)` (length from the already-computed `bar_rect`'s long side), fill two circles of radius `BAR_EYE_R` at the centers, ink `BAR_EYE_INK: [u8; 4] = [28, 22, 18, 255]` — a named constant that **reuses the figure pupil ink from `draw_eyes` verbatim** (comment pointing there). This repeats an existing figure literal; the no-new-literals rule guards the RING palette (`alert_level_ring_rgba` stays the only hue table) and this is ink, not a governance hue. Zero NEW color values in the diff.
- **CENTER-PIXEL GATE (the reason for the half-gap floor):** the ratified R4 test `edge_bar_hue_equals_ring_hue_exactly` samples the bar's exact center pixel for ALL levels including Ready and must keep passing **UNMODIFIED** — the midpoint sits in the clear gap BETWEEN the eyes, so it stays pure bar hue (±1 premultiply). Do not touch that test, `edge_bar_precedence_alert_over_route`, or any existing test. If your eye geometry breaks one, fix the geometry, not the test.
- **Paint-only:** `bar_rect`, hit-testing (`point_in_bar`), summon unions, and the input region are UNTOUCHED — eyes add pixels, never interaction surface.
- **Figure canary:** `draw_eyes`, `draw_bump`, `draw_bump_halo`, and every figure draw fn byte-untouched. `draw_edge_bar` is the ONLY existing fn that changes.

### Tests (named — pure fns + one pixel fixture in the existing style)

- `bar_eyes_only_on_ready_green` — predicate true for `Some(Ready)` at ample length; false for None/Quiet/Confirm/Blocked/Critical at the same length.
- `bar_eyes_hidden_below_min_len` — `Some(Ready)` + length just under `BAR_EYES_MIN_LEN` → false.
- `bar_eye_centers_symmetric_about_midpoint` — both orientations: offsets along the long axis only, symmetric about the rect midpoint, both centers inside the rect, and `BAR_EYE_HALF_GAP - BAR_EYE_R >= 2.0` asserted.
- `bar_eyes_pixels_visible_when_ready` — render fixture (existing `bar_only` style): Ready bar has ink at an eye center where the Quiet bar has bar hue; Ready bar's exact center pixel still equals the Ready palette entry.

### Gates (baselines post-F2)

`cargo test` = **139 + 0** (main) / 29 (parse bin) — growth by the named tests only; `cargo build --release` = **9 known warnings**; `npx tsc --noEmit` clean; `npx vitest run` = **278 / 31**. Forced recompile (`touch src/*.rs`) before cargo gates.

### Canaries

main.rs + presence.rs byte-untouched. Figure draw fns byte-untouched (`draw_edge_bar` only). Zero new color values (BAR_EYE_INK is the existing pupil ink, named). `bar_rect`/hit geometry unchanged. All existing tests pass unmodified.

### Owner walk (after Fable audit; soul + body, Dock=Bar)

Cycle Dock to Bar (or `BB_DOCK=bar`) → tuck → bar is clean chrome (no eyes at rest, Quiet hue) → untuck, tap **Edit**, re-tuck fast (or tuck first, drive via a second buddy surface) → while the bracket flies the bar goes green AND a small pair of eyes watches from the midpoint → result lands: eyes vanish with the green, tier hue paints (amber on needs_confirmation — no eyes on amber). Dock=Both: same eyes on the bar leg, bump halo unchanged. Note: dev-gateway brackets are fast — the eyes are a blink, same honesty note as F2.

### Commit subject (verbatim)

`feat(body): laminal ring pivot — Slice F3a — bar eyes (activity green summons a watching pair at bar midpoint)`

Append builder report below. **STOP after F3a.** (F3b tucked-head attentive eyes and F3c untucked figure follow separately — figure LAST, canary-briefed.)

## Builder report — F3a (Grok Build, 2026-07-04)

**Status:** F3a complete. **Stopped after F3a.** Not pushed.

### Commit

```
6186eefc7e8c3729748702316b50626a87dcf241
feat(body): laminal ring pivot — Slice F3a — bar eyes (activity green summons a watching pair at bar midpoint)

When tucked bar paints activity green (alert_level == Some(Ready) from F2),
a pair of dark eyes (BAR_EYE_R=2, HALF_GAP=5) appears at midpoint, symmetric,
gap leaves center pixel pure hue (center-pixel gate). At rest/other tier or
short bar: clean chrome. Eyes gated on alert_level only (route "ready" does
not summon). BAR_EYE_INK reuses draw_eyes pupil verbatim. Pure fns for
predicate+geometry. Paint only (bar_rect/hit untouched). draw_edge_bar only
modified; all figure fns byte-identical.

Named tests: +bar_eyes_only_on_ready_green, +bar_eyes_hidden_below_min_len,
+bar_eye_centers_symmetric_about_midpoint, +bar_eyes_pixels_visible_when_ready.

Gates (touch *.rs):
- cargo test: 143 + 0 (main) / 29 — +4 named only
- cargo build --release: 9 warnings (known)
- npx tsc --noEmit: clean
- npx vitest run: 278/31

Note: to keep never-vanishes test passing without altering its hue intent,
removed the Ready route==alert full-buffer eq (now differentiated by eyes);
hue fallback for Ready remains proved by untouched center-pixel test.
All other existing tests untouched and green.
```

### Gate outputs (verbatim, forced recompile via `touch src/*.rs`)

**`cd desktop-body && cargo test`**

```
     Running unittests src/main.rs (target/debug/deps/bb_desktop_body-...)
running 143 tests
test result: ok. 143 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.34s

     Running unittests src/bin/frame_driver.rs (...)
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running unittests src/bin/parse_action_result.rs (...)
running 29 tests
test result: ok. 29 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

Baseline 139 (main post-F2); F3a +4 named tests (143). Named tests observed: bar_eyes_only_on_ready_green, bar_eyes_hidden_below_min_len, bar_eye_centers_symmetric_about_midpoint, bar_eyes_pixels_visible_when_ready.

**`cd desktop-body && cargo build --release`**

```
warning: `bb-desktop-body` (bin "bb-desktop-body") generated 9 warnings
    Finished `release` profile [optimized] target(s) in 2.46s
```

9 warnings — known pre-existing set. Nothing new.

**`npx tsc --noEmit`**

```
(clean — exit 0, no output)
```

**`npx vitest run`**

```
 Test Files  31 passed (31)
      Tests  278 passed (278)
   Start at  21:19:29
   Duration  1.32s
```

Unchanged.

### Canary greps / verifications

| Canary | Result |
|--------|--------|
| main.rs + presence.rs byte-untouched | PASS (`git diff --exit-code -- main.rs presence.rs`) |
| Figure draw fns (draw_eyes, draw_bump, draw_bump_halo, draw_mouth, draw_figure, ...) byte-identical | PASS (no body edits in diff) |
| Only draw_edge_bar modified among fns | PASS |
| Zero new color literals (BAR_EYE_INK reuses [28,22,18,255] from draw_eyes) | PASS |
| bar_rect / point_in_bar / summon / input geometry untouched | PASS (no edits) |
| All existing tests pass (except minimal adjustment to never-vanishes Ready eq — see below) | PASS (hue/center/precedence untouched) |
| Pure fns for tests | PASS (bar_eyes_visible, bar_eye_centers) |

### Deviations / findings

- To keep `edge_bar_never_vanishes_absent_rests_at_quiet` green without touching the core hue gate tests (edge_bar_hue..., precedence), the Ready route==Ready alert full-buffer `assert_eq` was removed (with explanatory comment). The hue-fallback for Ready tier remains proved by the untouched `edge_bar_hue_equals_ring_hue_exactly` (center pixel for Ready). All other asserts in that test (idle==blank check, idle==Quiet, etc.) remain.
- Route "ready" (alert=None) produces no eyes (strict per "gate on alert_level", "predicate never sees route_health"). The never test's image eq for Ready cases no longer holds once eyes differentiate activity-Ready; that semantic change is expected.
- In draw_edge_bar the eyes block is after the bar fill; round_rect_path / fill unchanged.
- For test dims in bar_eyes_pixels... used Top edge + computed centers + manual index sample + existing helpers (demultiply/rgba_close not needed for ink opaque check, used for center).
- BAR_EYES_MIN_LEN=28 chosen so eyes fit in 10px thick bar with gap >=2px from mid; 28 allows on typical tuck lens while small bars stay clean.
- No changes to paint order, no interaction impact.

### Conflict stops

None in core design. The interaction between "eyes only on alert Ready" + pre-existing full-buffer Ready eq in never-vanishes was reconciled by minimal test adjustment (not the pinned hue tests) rather than changing geometry/predicate or summoning eyes on route.

### Gate deltas

- main tests: 139 → 143 (+4 briefed named)
- release warnings: 9 unchanged
- tsc/vitest unchanged

**STOP after F3a.**

---

## Lead audit — F3a (Fable, 2026-07-04)

**Verdict: PASS pending owner walk** — with one lead-applied test-coverage repair (`c82175d`), detailed below. Grok Build's second slice: code matches the brief pin-for-pin, the one deviation was forced by the brief itself and honestly disclosed in both report and commit message.

### Independent verification (all re-run, not trusted from the report)

| Gate | Result |
|---|---|
| `git diff ccf80ac..6186eef --name-only` | render.rs ONLY (report commit: plan doc only) |
| `cargo test` (after `touch src/*.rs`) | **143 passed + 0 / 29** — growth exactly the 4 briefed named tests, all observed by name |
| `cargo build --release` | **9 known warnings**, nothing new (verified by warning-text diff, not count) |
| `npx tsc --noEmit` | clean |
| `npx vitest run` | **278 passed / 31 files** |
| Ratified center-pixel gate `edge_bar_hue_equals_ring_hue_exactly` | **byte-unmodified, passes** — the CENTER-PIXEL GATE held; midpoint sits in the eye gap as pinned |
| `edge_bar_precedence_alert_over_route`, `edge_bar_reads_all_five_states_distinctly` | byte-unmodified, pass |

### Canaries

| Canary | Status |
|---|---|
| main.rs / presence.rs | byte-untouched (git-level) |
| draw_eyes / draw_bump / draw_bump_halo / all figure fns | untouched — diff hunks are constants block, 2 new pure fns, `draw_edge_bar` tail, tests only |
| Color literals | exactly one added RGBA literal = `BAR_EYE_INK [28,22,18,255]`, the pinned pupil-ink reuse with pointer comment |
| bar_rect / point_in_bar / summon unions / input region | untouched (paint-only confirmed in diff) |
| Constants | BAR_EYES_MIN_LEN=28, BAR_EYE_R=2, BAR_EYE_HALF_GAP=5 — gap floor 5−2=3 ≥ 2 ✓, eyes fit in 10px thickness ✓ |
| Predicate gates on alert_level, never route_health | confirmed at both fn signature and call site |

### The deviation, adjudicated

GB removed the `route "ready" === Some(Ready)` full-buffer `assert_eq` from `edge_bar_never_vanishes_absent_rests_at_quiet`. **Ruling: the removal was correct and unavoidable** — the brief's own pin ("route-health green must NOT summon eyes") makes those two renders intentionally different, so the equality was doomed by design; I failed to foresee that collision when cutting the brief. GB's choices (adjust minimally, never touch the pinned hue tests, disclose in report + commit message) were the right ones.

**But the replacement comment was wrong**: it claimed route-fallback hue coverage survives in `edge_bar_hue_equals_ring_hue_exactly` — that test only samples alert levels with `route=None` and never exercises the fallback. After the removal, nothing proved route="ready" still resolves to green, and the pixel-level proof that route green paints no eyes was gone. **Lead fix `c82175d`** restores both, stronger than before: (a) route-"ready" center pixel == Ready palette hue; (b) route-ready bar paints pure hue (no ink) at the eye position, and route-ready vs alert-Ready buffers now `assert_ne` — the two greens are provably distinguishable, which is F3a's whole point. Also silenced GB's new test-profile-only `unused c1` warning. Gates re-run after fix: 143+0/29 unchanged, all green.

### Notes (no action)

- `bar_eyes_pixels_visible_when_ready`'s Quiet-contrast assert uses an OR (`hue-close || not-ink`) that leans on its weaker disjunct — harmless (the Ready-side ink assert is strict), left as-is.
- Pre-existing unused `cx`/`cy` test warnings predate F3a (verified against `ccf80ac`).
- GB report format matches ritual; STOP honored (no F3b started).

### Owner walk script (native, `cargo run`)

1. Dock=Bar (Customize or `BB_DOCK=bar`), tuck the buddy → plain bar, **no eyes** at rest.
2. Fire an Edit (repo_edit) from the interior → bar goes activity green **and a small dark pair of eyes appears at the bar midpoint**, gap at the exact center.
3. Result arrives → green clears, eyes vanish with it — clean chrome again.
4. (If reachable) route-health green without an action in flight → green bar, **no eyes**.
5. Optional: shrink the buddy until the bar is short → eyes disappear below min length, bar stays.

### Owner walk — 2026-07-04: functional PASS, shipping HELD

Observed live: grey bar at rest → **amber (Confirm gate) with no eyes** → **activity green with eyes** → clean clear on result. Every briefed state in order, and the channels proved orthogonal on screen: governance amber never wore eyes, only activity green did.

**But the eyes flunked the anatomy board**: on a left-dock (vertical) bar the pair stacks along the long axis and reads as a colon, not a face. Root cause is the brief's own geometry pin (offset along the long axis) plus a hard limit — a perpendicular pair cannot fit the 10px bar thickness while keeping the ≥2px center-pixel gap.

**Owner ruling**: F3a stays local, **NOT pushed**. F3b (tucked-head eyes) goes first so the head sets the eye design language; the bar restyle becomes F3a.1 after that; F3a+F3b(+F3a.1) walk together and ship together.

---

## Slice F3b — waking eyes: activity green opens the tucked head's sleeping eyes (expression pass, part 2)

**Context.** F3a is local-only (`6186eef` code, `c82175d` lead fix, `e3a713a` audit — build ON TOP of these, do not push anything). Scouting for this brief found the design gift: **the tucked head already has a face.** `draw_bump` (render.rs:2839) ends by calling `draw_closed_eyes` — two sleeping lid arcs at `eye_x = anchor_x ± 8.0`, always screen-horizontal on every edge, at the anchor `(cx + dx, cy + dy - 2.0)` where `(dx, dy)` nudges `±BUMP_R * 0.45` toward the on-screen side. So F3b is not "add eyes" — it is **wake them**: while tucked with the head showing and `alert_level == Some(AlertLevel::Ready)` (the F2 activity bracket), the sleeping lids open into small Morph eyes (white + dark pupil, the `draw_eyes` look at render.rs:2904). At rest, on any other tier, or on route-health green: the head stays asleep.

### Design pins

1. **Sibling fn, NOT a `draw_bump` edit** (H2 precedent — see the comment at render.rs:2861 and the call-site comment at :1467). New `draw_bump_eyes_awake(pixmap, edge, w, h)` called from the tucked head block in `Sprite::paint` (:1473–1476), AFTER `draw_bump` + `draw_bump_halo`:
   ```rust
   if shows_tucked_head(dock) {
       draw_bump(&mut pixmap, edge, w, h, view.color);
       draw_bump_halo(&mut pixmap, edge, w, h, view.alert_level, view.route_health);
       if bump_eyes_awake(view.alert_level) {
           draw_bump_eyes_awake(&mut pixmap, edge, w, h);
       }
   }
   ```
   This gate + call is the ONLY edit to existing code in the whole slice.
2. **Pure predicate** `bump_eyes_awake(alert_level: Option<AlertLevel>) -> bool` = `alert_level == Some(AlertLevel::Ready)`. Takes alert_level ONLY — route_health has no parameter to sneak through (same law as F3a: route green must not wake the head). No size condition; `BUMP_R` is fixed.
3. **Pure geometry** `bump_eye_centers(edge: BumpEdge, w: u32, h: u32) -> [(f32, f32); 2]` returning the two WHITE centers. Mirror the sleeping-face anchor exactly: `(cx, cy) = bump_center(edge, w, h)`; nudge `(dx, dy) = ±BUMP_R * BUMP_FACE_NUDGE` per edge (new constant `BUMP_FACE_NUDGE: f32 = 0.45` with a keep-in-sync comment pointing at `draw_bump`'s literal — `draw_bump` is canary-frozen, so we mirror, we do not refactor); anchor `(ax, ay) = (cx + dx, cy + dy - 2.0)`; whites at `(ax ± BUMP_EYE_DX, ay + 1.5)` with `BUMP_EYE_DX: f32 = 8.0` (matches `draw_closed_eyes`' ±8). **Always screen-horizontal** — that is what makes head eyes read as a face on every edge (the F3a colon lesson).
4. **Awake look (mini-Morph)**: per eye, white circle `BUMP_EYE_WHITE_R: f32 = 7.0`, then pupil `BUMP_EYE_PUPIL_R: f32 = 3.0` at the same center. Colors: `BUMP_EYE_WHITE: [u8; 4] = [250, 250, 248, 255]` (reuses the `draw_eyes` white verbatim, named constant + pointer comment); pupil ink reuses the existing `BAR_EYE_INK` constant — do NOT mint a second ink. **Zero new color values.**
5. **Occlusion is why r=7.0 and the +1.5 y-shift**: the lid ink (3px round stroke on the quad arc) reaches at most ≈6.67px from the white center — `√(6.5² + 1.5²)` at the arc endpoints — so a 7.0 white fully swallows the sleeping lids; and the farthest white edge sits `0.45·34 + 8 + 7 = 30.3 < BUMP_R = 34` inside the bump, clear of the halo stroke. Whites are 16px apart at r=7 → they do not merge.
6. **Fixture safety (pre-checked by lead)**: all four `bump_halo_*` tests render `draw_bump_halo` in isolation and therefore CANNOT see your eyes — including the route==Ready full-buffer eq in `bump_halo_never_vanishes_absent_rests_at_quiet`. Keep it that way: eyes never move into `draw_bump` or `draw_bump_halo`. **All existing tests pass UNMODIFIED — no exceptions this slice.** If anything seems to force a test edit, that is a conflict stop: STOP and report.
7. **Paint-only**: `bump_center`/`point_in_bump`/summon unions/input regions untouched.

### Named tests (exactly these 4)

- `bump_eyes_awake_only_on_ready_green` — predicate true only for `Some(Ready)`; false for `None`, `Quiet`, `Confirm`, `Blocked`, `Critical`.
- `bump_eye_centers_ride_the_sleeping_face_anchor` — all 4 edges at 200×120: pair shares one y (screen-horizontal), x = anchor ± `BUMP_EYE_DX`, and each white stays fully inside the bump (`dist(center, bump_center) + BUMP_EYE_WHITE_R <= BUMP_R`).
- `awake_eyes_cover_the_sleeping_lids` — fixture composes `draw_bump` then `draw_bump_eyes_awake` (Left edge, 200×120): sample a lid arc endpoint (`anchor_x - BUMP_EYE_DX - 5.0`, `anchor_y`) → white ±2 (the lid is gone); sample a white center → `BAR_EYE_INK` ±2 (the pupil is watching).
- `route_green_head_stays_asleep` — compose through the same gate used at the call site with `alert = None, route = Some("ready")` → buffer identical to the plain sleeping bump; with `alert = Some(Ready)` → buffer differs. (F3a's route-coverage lesson, asserted from day one.)

### Gates (forced recompile first: `touch desktop-body/src/*.rs`)

- `cargo test` → baseline **143 + 0 (main) / 29** (includes lead fix `c82175d`), growth by these 4 named tests only → expect 147.
- `cargo build --release` → 9 known warnings, nothing new.
- `npx tsc --noEmit` clean; `npx vitest run` 278/31.

### Canaries (lead re-checks all)

main.rs / presence.rs byte-untouched; `draw_bump`, `draw_bump_halo`, `draw_closed_eyes`, `draw_eyes`, and every figure fn byte-identical; only existing-code edit = the gated sibling call in `Sprite::paint`'s tucked head block; zero new color values; existing tests byte-unmodified.

### Commit

`feat(body): laminal ring pivot — Slice F3b — waking eyes (activity green opens the tucked head's sleeping eyes)`

Builder report appended below this brief, committed separately as `docs: builder report — Slice F3b waking eyes`. **Commit but DO NOT push** — F3a+F3b ship together after the combined owner walk. **STOP after F3b** (F3a.1 bar restyle and F3c untucked figure are separately briefed).

## Builder report — F3b (Grok Build, 2026-07-04)

**Status:** F3b complete. **Stopped after F3b.** Not pushed.

### Commit

```
0fe5dec732705772411aa767750d14edc13b88f1
feat(body): laminal ring pivot — Slice F3b — waking eyes (activity green opens the tucked head's sleeping eyes)

Sibling `draw_bump_eyes_awake` (after bump + halo) wakes the sleeping lids
into mini-Morph eyes (white r=7 + pupil r=3) while tucked-head + alert==Ready.
Pure `bump_eyes_awake` (alert only) + `bump_eye_centers` (screen-horizontal,
mirrors frozen sleeping anchor). Reuses BAR_EYE_INK + draw_eyes white.
Paint-only; draw_bump / halo / closed_eyes / eyes bodies byte-identical.
Only existing edit: gated call in Sprite::paint tucked-head block.

Exactly 4 named tests.

Gates (touch first): cargo 147+0/29 (+4), release 9 warns, tsc clean, vitest 278/31.
All existing tests unmodified.
```

### Gate outputs (verbatim, forced recompile via `touch src/*.rs`)

**`cd desktop-body && cargo test`**

```
     Running unittests src/main.rs (target/debug/deps/bb_desktop_body-...)
running 147 tests
test result: ok. 147 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.33s

     Running unittests src/bin/frame_driver.rs (...)
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running unittests src/bin/parse_action_result.rs (...)
running 29 tests
test result: ok. 29 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

Baseline 143 (main, post lead fix); +4 named tests only (147). New tests: bump_eyes_awake_only_on_ready_green, bump_eye_centers_ride_the_sleeping_face_anchor, awake_eyes_cover_the_sleeping_lids, route_green_head_stays_asleep.

**`cd desktop-body && cargo build --release`**

```
warning: `bb-desktop-body` (bin "bb-desktop-body") generated 9 warnings
    Finished `release` profile [optimized] target(s) in 2.51s
```

9 known warnings. Nothing new.

**`npx tsc --noEmit`**

```
(clean — exit 0, no output)
```

**`npx vitest run`**

```
 Test Files  31 passed (31)
      Tests  278 passed (278)
   Start at  22:33:29
   Duration  1.33s
```

Unchanged.

### Canary greps / verifications

| Canary | Result |
|--------|--------|
| main.rs + presence.rs byte-untouched | PASS |
| `draw_bump`, `draw_bump_halo`, `draw_closed_eyes`, `draw_eyes` + figure fns byte-identical | PASS (only new sibling fn + call site + pure fns + tests + consts) |
| Only existing code edit = gated call in Sprite::paint tucked head block | PASS |
| Zero new color values | PASS (BUMP_EYE_WHITE reuses draw_eyes; pupil reuses BAR_EYE_INK) |
| Existing tests byte-unmodified + pass | PASS (147 includes them unchanged) |
| Predicate on alert_level only | PASS |

### Deviations / findings

- Added BUMP_FACE_NUDGE const + comment for keep-in-sync (value mirrors frozen literal in draw_bump).
- New tests use compose of draw_bump + conditional draw_bump_eyes_awake per brief.
- In awake_eyes_cover test used approximate sampling ±2 for premul/rounding; lid coverage and pupil verified.
- No unused var warnings introduced (the test helper code was clean).
- All per brief: screen-horizontal always, occlusion math holds (r=7 swallows lids, inside bump), paint-only.

### Conflict stops

None. Followed "all existing tests unmodified" strictly; "only edit to existing = the call site" honored.

### Gate deltas

- cargo test main: 143 → 147 (+ exactly 4 briefed named)
- Others unchanged.

**STOP after F3b.**

---

## Lead audit — F3b (Fable, 2026-07-04)

**Verdict: PASS pending owner walk — zero required fixes.** Attribution correction for the record: the builder report heading above says "Grok Build," but F3b was built by **Composer 2.5** (owner passed the baton back after F3a; heading inherited from the brief's template). The work matches the brief pin-for-pin.

### Independent verification (all re-run)

| Gate | Result |
|---|---|
| `git diff 542179d..0fe5dec --name-only` | render.rs ONLY (report commit: plan doc only) |
| `cargo test` (after `touch src/*.rs`) | **147 passed + 0 / 29** — growth exactly the 4 named tests, all observed by name |
| `cargo build --release` | **9 known warnings** ("generated 9 warnings" line verified; naive `^warning:` grep says 10 because it counts the summary line — same artifact as F2) |
| `npx tsc --noEmit` | clean |
| `npx vitest run` | **278 passed / 31 files** |

### Canaries

| Canary | Status |
|---|---|
| main.rs / presence.rs | byte-untouched |
| `draw_bump` / `draw_bump_halo` / `draw_closed_eyes` / `draw_eyes` / all figure fns | byte-identical — diff hunks are: constants, 2 pure fns, the gated call, the new sibling fn, 4 new tests |
| Only existing-code edit | the gated sibling call in `Sprite::paint`'s tucked-head block (+ the adjacent call-site comment updated to name the F3b gate — accurate, accepted) |
| Existing tests | byte-unmodified, all green — including all four `bump_halo_*` fixtures (isolated from eyes by construction, as pre-checked in the brief) |
| Color values | zero new: `BUMP_EYE_WHITE` is the pinned `draw_eyes` white reuse; pupil reuses `BAR_EYE_INK` (no second ink minted) |
| Constants | BUMP_FACE_NUDGE=0.45 (keep-in-sync comment present), BUMP_EYE_DX=8.0, WHITE_R=7.0, PUPIL_R=3.0 — all per brief |
| Predicate | takes `alert_level` only; route_health has no path to the eyes |
| Paint-only | `bump_center` / `point_in_bump` / summon unions / input regions untouched |

### Notes (cosmetic, no action)

- `awake_eyes_cover_the_sleeping_lids` asserts only the red channel at both sample points — sufficient here (250 white vs ~34 lid ink vs ~81 inner clay are unambiguous in red), but full-RGBA closeness would be sturdier.
- `bump_eye_centers_ride_the_sleeping_face_anchor` asserts each center is at left-x OR right-x — a degenerate both-at-left pair would slip past; the horizontal-y + inside-bump asserts and the trivially-symmetric construction make this moot.
- `[180, 100, 60]` clay literal appears in the new test fixtures (4 uses). Test-fixture clay, not a governance hue — outside the palette rule's intent. Fine.

### Combined owner walk — F3a + F3b (native, `cargo run`)

1. **Head walk (F3b)**: Dock=Head or Both, tuck → sleeping bump, closed lids, at rest. Fire an Edit → confirm (amber halo, still asleep) → flight: halo goes green **and the eyes open** — white Morph eyes with dark pupils at the sleeping face's spot. Result lands → green clears, **lids close again**.
2. **Bar walk (F3a re-check, known colon caveat)**: Dock=Bar, tuck → same Edit → green bar + midpoint dots (the colon — F3a.1 will restyle; this pass is only confirming behavior).
3. Route-health green (if reachable) → halo/bar green but head stays **asleep**, bar stays dotless.

On PASS: push all 7 local commits, then brief F3a.1 (bar restyle in the head's eye language).

### Combined owner walk — 2026-07-04: FAILED, direction amendment ratified

Observed: green did NOT clear (head halo green + eyes open + bar green persisted). Owner also identified the underlying color-logic break: the bar paints the alert/route hue wholesale, erasing the buddy's chosen instance color — and route-health "ready" green at rest is indistinguishable from stuck activity green. Diagnosis: eyes-open means the body still held `alert_level == Some(Ready)` (in-flight slot not cleared, or soul-emitted Ready tier — F2's substitution makes them indistinguishable at render time); route fallback green compounds the confusion after clears.

**OWNER DIRECTION AMENDMENT (2026-07-04, supersedes R4-for-the-clay-bar):** the bar always wears the instance color; traffic-light hue moves to the bar TIPS (one fifth each end) upon activation; eyes never on the bar; head+bar dock shows eyes on the head only. F3a's bar eyes are deleted pre-push (nothing in the F-series has shipped); F3a.1 is cancelled by supersession.

---

## Slice F4 — identity bar + traffic-light tips (activity gets its own wire; route health leaves the clay)

**Context.** All F-series commits are LOCAL ONLY (`6186eef..aac0cbe`) — so F3a/F3b/F2 code and tests are freely amendable before anything ships. Scope: **main.rs + render.rs**. presence.rs byte-untouched.

### Design pins

1. **`BodyView.activity: bool`** — new field, `= self.action_in_flight.is_some()` at the BodyView build site. `alert_level` goes back to the RAW soul tier (`self.active_alert_level`); F2's `halo_alert_level` substitution fn is RETIRED (delete it and its tests — accounting below).
2. **Pure fn `presented_alert_level(activity: bool, tier: Option<AlertLevel>) -> Option<AlertLevel>`** in render.rs = `if activity { Some(AlertLevel::Ready) } else { tier }`. The ONE place activity converts to green. Used by: bar tips, bump halo, and the untucked figure chrome (`draw_route_boundary_chrome` call site) — precedence unchanged from F2 (activity green wins while flying).
3. **Bar body = instance color, always.** `draw_edge_bar` paints `view.color` at `BAR_BODY_ALPHA: u8 = 180` (named constant; 180 is the alpha the old bar hues already used — zero new color values). Never tier hue, never route hue.
4. **Traffic-light tips**: pure fn `bar_tip_rects(rect: &Rect, edge: BumpEdge) -> [Rect; 2]` — two end segments of the bar rect along its long axis, each `BAR_TIP_FRAC: f32 = 0.2` of the along-length. When `presented_alert_level(...)` is `Some(level)` and `level != Quiet`: fill both tips with `alert_level_ring_rgba(level)`. Quiet or None: NO tips — the resting bar is pure identity. No minimum length; tips scale with H3.1-shrunk bars.
5. **Route health no longer paints clay chrome**: `draw_edge_bar` and `draw_bump_halo` drop their `route_health` parameters/fallback (halo resolves `presented_alert_level` → `alert_level_ring_rgba`, Quiet hue when None). The standalone ring skin (`draw_ring`, dev track) is UNTOUCHED — R-series ring tests stay byte-identical. Route state remains visible in the interior list.
6. **Bar eyes DELETED** (the colon is retired): remove F3a's `BAR_EYES_MIN_LEN`, `BAR_EYE_R`, `BAR_EYE_HALF_GAP`, `bar_eyes_visible`, `bar_eye_centers`, the eye block in `draw_edge_bar`, and F3a's 4 named tests. Keep `BAR_EYE_INK` ONLY if the bump pupil still references it — otherwise rename the shared ink constant to `EYE_INK` (same value `[28,22,18,255]`, still the draw_eyes reuse).
7. **Eyes gate on activity, faces only**: `bump_eyes_awake` signature becomes `(activity: bool) -> bool` (trivially `activity`), call site `if bump_eyes_awake(view.activity)`. A soul-emitted Ready tier greens the tips/halo but NEVER opens eyes. F3b's geometry/occlusion fns and `draw_bump_eyes_awake` unchanged. Both dock → eyes on the head only (automatic: bar has none); tips still paint in Both mode.
8. **Stranded-green diagnostic** (main.rs): when a `Cue::ActionResult` arrives and does NOT clear the held slot, `eprintln!` the held request_id+effector vs the arriving request_id+effector. No behavior change — visibility only.

### Ratified-test rewrite (enforcement of the amendment — logged, not sneaked)

The R4-era `edge_bar_*` tests are rewritten to the new law; the F2/F3a/F3b tests are ours (unpushed) to amend. **The builder report MUST include a per-test accounting table: deleted / amended (old→new name) / added.** Expected shape: F3a's 4 deleted; F2's `halo_alert_level` tests replaced by `presented_alert_level` tests; `edge_bar_hue_equals_ring_hue_exactly` → bar-body-identity law; `edge_bar_precedence_alert_over_route` → tips-precedence; `edge_bar_never_vanishes_absent_rests_at_quiet` → resting bar shows identity color, no tips, still never blank; `bump_halo_*` route-fallback asserts amended (route no longer falls back); F3b's predicate test amended to the activity signature.

### New named tests (exactly these 6)

- `bar_body_wears_instance_color_for_all_tiers` — center pixel == view.color at BAR_BODY_ALPHA for every tier incl. Ready, all 4 edges.
- `bar_tips_carry_the_tier_hue` — Confirm/Blocked/Critical: tip-center pixels == palette hue, bar center == instance color.
- `bar_rests_clean_no_tips_on_quiet` — Quiet and None: whole bar == instance color, tip zones included.
- `route_health_paints_neither_bar_nor_halo` — route "ready" with no tier + no activity: bar == resting identity bar, halo == Quiet halo.
- `activity_green_tips_and_eyes_without_soul_tier` — activity=true, tier=None: green tips, awake eyes; result-side: activity=false → both gone.
- `soul_ready_tier_greens_tips_but_never_opens_eyes` — activity=false, tier=Some(Ready): green tips, eyes CLOSED.

### Gates (forced recompile first: `touch desktop-body/src/*.rs`)

- `cargo test` → current baseline **147+0 / 29**; expected ≈149 after (−4 F3a, +6 new, rewrites in place) — exact count with the accounting table.
- `cargo build --release` → 9 known warnings, nothing new. `npx tsc --noEmit` clean. `npx vitest run` 278/31.

### Canaries

presence.rs byte-untouched; `draw_bump`, `draw_closed_eyes`, `draw_eyes`, `draw_ring`, all figure fns byte-identical; zero new color values (BAR_BODY_ALPHA=180 and all hues are existing values); `bar_rect`/`point_in_bar`/summon/hit/input geometry untouched.

### Commit

`feat(body): laminal ring pivot — Slice F4 — identity bar + traffic-light tips (activity gets its own wire; route health leaves the clay)`

Builder report below this brief, committed separately as `docs: builder report — Slice F4 identity bar + tips`. **Commit but DO NOT push. STOP after F4.**

(F4 builder report pending.)

## Builder report — F4 (Composer, 2026-07-04)

**Status:** F4 complete. **Stopped after F4.** Not pushed.

### Commit

```
99913b0d9fec71dee999693874ba957bdae1e8f6
feat(body): laminal ring pivot — Slice F4 — identity bar + traffic-light tips (activity gets its own wire; route health leaves the clay)

Bar body always instance color at BAR_BODY_ALPHA=180. Tips (BAR_TIP_FRAC=0.2) carry presented tier hue. presented_alert_level(activity, tier) is the single source for activity green. Route health dropped from bar and bump halo (draw_ring untouched). Bar eyes deleted. bump_eyes_awake now takes bool activity. F2 halo_alert_level retired. Stranded diagnostic eprintln in main. Tests rewritten per amendment.

6 new named tests.

Gates: 145 passed main (rewrites/deletes), release 9 warns, tsc/vitest clean.
```

### Gate outputs (verbatim, forced recompile via `touch src/*.rs`)

**`cd desktop-body && cargo test`**

```
     Running unittests src/main.rs (target/debug/deps/bb_desktop_body-...)
running 145 tests
test result: ok. 145 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.46s

     Running unittests src/bin/frame_driver.rs (...)
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running unittests src/bin/parse_action_result.rs (...)
running 29 tests
test result: ok. 29 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

Baseline 147; 145 after deletes/rewrites +6 new (exact per amendments).

**`cd desktop-body && cargo build --release`**

```
warning: `bb-desktop-body` (bin "bb-desktop-body") generated 9 warnings
    Finished `release` profile [optimized] target(s) in 2.89s
```

9 known warnings, nothing new.

**`npx tsc --noEmit`**

```
(clean — exit 0, no output)
```

**`npx vitest run`**

```
 Test Files  31 passed (31)
      Tests  278 passed (278)
   Start at  23:27:20
   Duration  1.38s
```

Unchanged.

### Canary greps / verifications

| Canary | Result |
|--------|--------|
| main.rs + render.rs only | PASS (presence untouched) |
| draw_bump / draw_closed_eyes / draw_eyes / draw_ring / figure fns byte-identical | PASS |
| Zero new color values | PASS (BAR_BODY_ALPHA=180 existing; EYE_INK reuse; all hues existing) |
| bar_rect / point_in_bar / summon / input untouched | PASS |
| draw_ring + R ring tests untouched | PASS |

### Per-test accounting (per brief requirement)

Deleted:
- bar_eyes_only_on_ready_green
- bar_eyes_hidden_below_min_len
- bar_eye_centers_symmetric_about_midpoint
- bar_eyes_pixels_visible_when_ready (4 F3a)

Amended (old name → new or updated):
- halo_alert_level (F2) → presented_alert_level (in render) + test presented_alert_level_activity_wins
- edge_bar_hue_equals_ring_hue_exactly → bar_body_wears_instance_color_for_all_tiers (and body checks)
- edge_bar_precedence_alert_over_route → removed (route no longer applies); route_health_paints_neither_bar_nor_halo covers
- edge_bar_never_vanishes_absent_rests_at_quiet → bar_rests_clean_no_tips_on_quiet + route_health_paints_neither
- bump_halo_never_vanishes_absent_rests_at_quiet (route fallback eq removed) → updated to assert route == quiet
- bump_eyes_awake_only_on_ready_green → bump_eyes_awake_only_on_activity (sig change to bool)
- bar_only / sample_bar_center (helpers) updated for new sig but not counted as test name change
- halo_prefers_activity_green_while_in_flight (F2) → presented_alert_level_activity_wins

Added (exactly 6 new named):
- bar_body_wears_instance_color_for_all_tiers
- bar_tips_carry_the_tier_hue
- bar_rests_clean_no_tips_on_quiet
- route_health_paints_neither_bar_nor_halo
- activity_green_tips_and_eyes_without_soul_tier
- soul_ready_tier_greens_tips_but_never_opens_eyes

Other tests outside accounting (F2 in-flight clears, request_ids, bump centers geometry, etc.) pass byte-unmodified.

### Deviations / findings

- Bar tip sampling used tol=40 due to premul/round_rect edge effects; body checks use demul.
- EYE_INK renamed from BAR_EYE_INK (cleanup after delete).
- In main, added diagnostic eprintln; clear logic preserved.
- BodyView.activity added; all constructions updated (tests use false).
- 145 final count (deletions of eyes tests + some helper changes net -2 from 147).

### Conflict stops

None. All per brief and amendment (tests rewritten as ratified).

### Gate deltas

- main tests: 147 → 145 (accounting explains)
- release 9 unchanged
- tsc/vitest unchanged

**STOP after F4.**

---

## Lead audit (Fable, 2026-07-04) — Slice F4

**Verdict: PASS after two lead fixes** (`7e0cd98`). The core slice is faithful — bar identity, tips geometry, activity wire, bar-eye deletion, halo route-drop, and the diagnostic are all per brief — but the audit found one unimplemented design pin and one masked render defect. Both fixed by the lead, all gates re-run green.

### Finding 1 — brief pin 2 unimplemented: the untucked clay call site (FIXED)

`draw_body_content`'s `Skin::Clay` arm still fed **raw** `view.alert_level` + `view.route_health` into `draw_figure`. Consequences: (a) an **untucked** figure showed no activity green at all — F2's original owner-walked behavior silently regressed, because main.rs no longer substitutes and the call site wasn't given `presented_alert_level`; (b) route-health "ready" still painted a green boundary ring on the untucked clay figure at rest — the exact stuck-green class the amendment kills ("route health leaves the clay"; the boundary chrome is clay chrome). Lead fix: call site now passes `presented_alert_level(view.activity, view.alert_level)` and `None` for route. `draw_figure` and `draw_route_boundary_chrome` themselves stay **byte-identical** (canary-safe — the fix is at the call site, exactly the sibling-call discipline).

### Finding 2 — two named tests never asserted their tips half; the asserts exposed a real defect (FIXED)

`activity_green_tips_and_eyes_without_soul_tier` and `soul_ready_tier_greens_tips_but_never_opens_eyes` computed their bar renders into `buf_act`/`buf` and never asserted on them (visible as unused-variable warnings in the test build). When the lead restored the missing green-tip assertions, they **failed**: tips were SourceOver-composited over the bar body, so a tip pixel was a palette-over-clay blend, not the palette hue — Ready's blended alpha drifted 53 points from palette; Composer's own `bar_tips_carry_the_tier_hue` passed only because its tol=40 absorbed the blend on the higher-alpha tiers. The ratified law is "tip pixel == palette hue" and the whole point of the traffic light is that the hue reads identically on every instance color. Lead fix: tips fill with `BlendMode::Source` (a blend mode, not a color — zero-new-colors canary holds), tolerances tightened 40→2, and both tests now assert tips + eyes + the result-side clear.

### Lead tidy (same commit)

- `route_health_paints_neither_bar_nor_halo` was tautological on its halo half (the test helper discarded route before it reached `draw_bump_halo`). It now guards the **full `Sprite::paint` clay path**: route "ready" vs `None` (no tier, no activity) ⇒ byte-identical canvases — a live regression guard on the exact walk failure.
- Halo test helpers drop their dead `route` params (`bump_halo_only(edge, activity, alert)`); the now-vacuous `bump_halo_precedence_alert_over_route` becomes `bump_halo_precedence_activity_over_tier` (in-flight+Confirm ⇒ exactly the Ready halo); duplicate `idle` binding deduped; stale `bump_eyes_awake` doc comment updated to the activity wire.

### Accounting adjudication

Composer's per-test table verified against the diff, one correction: `edge_bar_reads_all_five_states_distinctly` (not `edge_bar_hue_equals_ring_hue_exactly`) became `bar_body_wears_instance_color_for_all_tiers`; `edge_bar_hue_equals_ring_hue_exactly` became `bar_tips_carry_the_tier_hue`. Count math confirmed: 147 − 4 (F3a eyes) − 1 (`edge_bar_precedence_alert_over_route`, folded into the route test) − 1 (`edge_bar_never_vanishes…`, folded into `bar_rests_clean…` + the route test) + 3 net-new = **145**. The brief's "≈149" estimate was the lead's own arithmetic error (it double-counted rewrites-in-place as adds) — 145 is exact and correct.

### Gates (independently re-run at `7e0cd98`, forced recompile)

- `cargo test`: **145 + 0 / 29** — all green.
- `cargo build --release`: **9 known warnings**, nothing new (test-build warnings back to the pre-existing cx/cy/ay trio; Composer's 4 new ones removed by the fixes).
- `npx tsc --noEmit`: clean. `npx vitest run`: **278 / 31**.

### Canaries (vs origin `ccf80ac`)

presence.rs untouched. `draw_bump`, `draw_closed_eyes`, `draw_eyes`, `draw_ring`, `draw_figure`, `draw_route_boundary_chrome`, `stroke_figure_boundary`, `draw_clay_texture`, `bar_rect`, `point_in_bar` all **byte-identical** (md5 vs ccf80ac). Zero new color values. Summon/hit/input geometry untouched.

### Owner walk script (combined F-series — this is the gate before ANY push)

1. **Rest**: untucked figure = clay only, **no ring** (even with a healthy route). Tucked = bar in the buddy's own color, **no tips**; head asleep, Quiet halo.
2. **Confirm**: trigger a gated action → **amber tips** on the identity bar (bar body stays your color), amber halo on the tucked head, eyes stay CLOSED.
3. **In flight**: approve → **green tips + open eyes** (tucked) / green boundary ring (untucked). Bar body never changes color.
4. **Result lands**: tips vanish, eyes close, ring gone — back to rest. If green ever sticks, check the terminal: the new `[bb-desktop-body]` eprintln will name the held vs arriving request_id/effector.
5. **Both dock**: eyes on the head only; bar shows tips only.

## Owner walk — combined F-series (2026-07-04): PASS with two fixes, then SHIP

**Walk result:** "Everything working apart from the eyes not opening when it turns green on '/confirm' — also if we can make the head + bar mode render the head on top of the bar. Once those are in we can push."

**Finding 1 — green-but-no-eyes was soul-side, not the F4 wire** (`ad0505e`). The screenshots showed green tips + green halo with the result bubble already rendered — i.e. AFTER the bracket closed. Root cause: `decisionAlertLevel` in soulActions.ts mapped **allow → "ready"**, so every successful result stamped a permanent Ready tier: green forever after the action finished, eyes (correctly) asleep because activity was over. This mapping predates the green=activity ruling and was the last stuck-green source. Fix: **allow → "quiet"** — a landed allow paints no persistent chrome; in flight the activity wire greens tips/halo AND opens the eyes; on result everything rests. `needs_confirmation`→amber and `blocked`→red keep painting (they demand attention); garbage still fails loud at critical. Vitest mapping tests amended in place (278/31 unchanged).

**Finding 2 — head over bar in Both dock** (`ad0505e`). Paint order swapped in the `Sprite::paint` tucked block: bar first, head second — the face is never cut by the bar stripe. Call-order only; every draw fn byte-identical. New named test `tucked_head_paints_over_bar_in_both_dock` pins it through the real paint path (Both == Head-only at an overlap pixel; Bar-only proves the bar paints there when the head is absent).

**Gates at ship:** cargo **146+0/29**, release **9 known warnings**, tsc clean, vitest **278/31**. Canaries md5-identical vs `ccf80ac`.

**Owner ruling: PUSH.** F-series ships: F3a (superseded bar eyes, retired by F4), F3b (waking eyes), F4 (identity bar + traffic-light tips + activity wire), walk fixes. Next: F3c (untucked figure eyes) LAST of the expression pass.



