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
