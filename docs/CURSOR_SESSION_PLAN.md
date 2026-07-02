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

**Remaining gate — owner walk (clay default):** launch plainly (no `BB_SKIN`) → full
figure wearing the halo; click E → amber halo + confirm bubble; confirm → green halo;
tuck → sleeping bump with hued halo ring; summon by clicking the bump. Optionally
`BB_SKIN=ring`: tuck → bar, click anywhere along it → summons. Walk pass ⇒ push
`75e8db6..HEAD` (R4 + docs + H1 + H2).
