# Composer Audit — Wizard Onboarding Panel (Build C)

**Date:** 2026-06-27  
**Branch:** `slice-0-launchers`  
**Audience:** Opus (or any implementer picking up the deferred onboarding-panel work)  
**Role:** Auditor / advisor composer output from session 2026-06-27

---

## Session state at handoff

**Committed on `slice-0-launchers` (ahead 2, not pushed at L2 checkpoint):**

| Commit | Slice | What it proves | Verified |
|--------|-------|----------------|----------|
| `fb48678` | Slice 1 — panel cue wire format (TS + Rust, golden fixture) | `PresencePanel` type, strict-parse, `Cue::Panel` parser, fixture extension | vitest 250 · cargo 89 |
| `d6c0212` | Slice 2 — Host emits panel section cues from act state | Soul pushes `panel` envelopes per act; wire smoke: `none` → `connect(connection_ok)` → `posture(posture_set)` → `placement(next)` | vitest 253 · cargo 89 · wire smoke ✓ |
| `3fab46d` | Build B — soul-side Wizard Host (Acts 0–5 + receipts) | `wizardOnboardingHost.ts` + `soul-server.ts` under `BB_SOUL=wizard` | vitest 245 (at land) |
| `cadff5e` | Body UX — right-click fixes | Direct unpin; tucked peek cycle (HeadOnly → Bubble → BubbleInput) | cargo 87 (at land) |
| `6ce23b0` | Body — soul-gated pin + bubble honesty | Pin arms on soul `allow`; bubble no longer lies about torso | — |

**Still deferred:** native in-torso **render** — the body parses `panel` cues and the soul emits them, but `Cue::Panel` in `main.rs` is still a stub. A real user cannot yet touch connect / posture / placement / summary forms on the native body.

**Belayed (do not pursue):** rendering the receipt/governance opaque window (opens on full torso stretch) closer to the buddy — the bloom dial needs that space.

---

## Design question resolved

**Fork:** 2nd Tauri `WebviewWindow` vs native Rust in-torso panel.

**Recommendation: Build C = native in-torso onboarding panel. Do not pursue a 2nd Tauri WebviewWindow for first-run.**

### Why the “WINDOW” framing misled

1. **Day-to-day body is native** — `npm run body:dev` → `scripts/bb-body.sh` → `bb-desktop-body` (smithay layer-shell). Tauri (`npm run desktop:dev`) is the alt stack with a single 1×1 transparent `border-dock` (`src-tauri/tauri.conf.json`).

2. **React forms already render in-surface** — `OnboardingWizardPanel.tsx` lives inside `BuddySurface.tsx` torso/bubble, not a separate window. The browser path never shipped a detached panel either.

3. **Overlay postmortem killed transparent WebKit on the desktop** — but it carved out *opaque normal windows* for rich UI. That guidance applies to the Tauri shell, not to the native body shipping for slice-0.

`docs/WIZARD_ONBOARDING_SCRIPT.md` still says “forms live in a normal opaque webview panel window” — that sentence predates the native-body rebuild. Update wording on next doc pass so future agents do not re-open this fork.

---

## Option comparison

### Option A — 2nd Tauri `WebviewWindow`

**Strengths:**

- Reuses `OnboardingWizardPanel.tsx` + `onboardingPanelModel.ts` verbatim
- Opaque WebKit is probably fine (unlike transparent overlay)
- Matches original wizard script wording

**Weaknesses:**

- Lands in `src-tauri`, which is **not** the body path run for buddies
- Needs new window declaration, spawn/show/hide lifecycle, positioning
- **Law 7 blur:** panel clicks originate outside the body; extra bridge webview → soul WebSocket
- Two stacks to maintain: native body for presence, Tauri for first-run
- Act 3 placement demo (`move_to` live while user picks edges) requires cross-surface coordination
- First-run experience runs on the stack nobody dogfoods daily

**Verdict:** Architecturally valid for a Tauri-first product. Wrong fork for slice-0.

### Option B — Native in-torso panel (Build C)

**Strengths:**

- Lands in the **live** body (`body:dev` + `soul:dev`)
- Law 7 clean: Host pushes `panel` cue → body draws → body emits `clicked{panel:*}`
- Proven pattern: settings panel (`interior_rows_for` + shared hit-test), tucked live input (`cadff5e`)
- Single WebSocket, no bridge
- Act 3 can demo `move_to` while user configures placement in the same torso

**Weaknesses:**

- Must reimplement forms in `render.rs` + `main.rs` hit-test
- React panel becomes **reference**, not reused code — drift risk
- Credential `paste_key` needs native clipboard + masked echo
- Hub-mode section nav is more UI work in immediate-mode Rust
- Visual polish slower than React

**Verdict:** Higher implementation cost, lower architectural risk. Matches where the codebase is already pointed.

### Reconciling with `OVERLAY_POSTMORTEM_AND_REBUILD_PLAN.md` §7.2

“Rich UI in normal Tauri windows” is **deferred hub/workbench** — trust workbench, full chat, maybe post-onboarding settings hub in a browser or Tauri surface later. **First-run must not block on a shell you do not run.**

---

## Build C execution plan

```
Slice 1  ✅ done (fb48678)  panel cue protocol (TS + presence.rs parser + fixture)
Slice 2  ✅ done (d6c0212)  soul emits panel cues (wizardOnboardingHost → emitCues + panel)
Slice 3  [in progress — native render landed, uncommitted]  render.rs draw + layout; main.rs state/draft/hit-test/clipboard/paste_key
Slice 4  pending              settings writes on panel:* receipt paths (additive choice-on-clicked; posture tighten-only)
Slice 5  pending              E2E: BB_SOUL=wizard + BB_BUDDY=host, full Act 0→5, no injected clicks; restart → hub
```

### Slice 3 specifics (the real work)

Model on the settings panel pattern (`desktop-body/src/main.rs` ~2670–2715):

- **Connect:** provider option rows + `paste_key` field (masked dots, paste-not-type — user decision)
- **Posture:** three selectable cards; primary emits `panel:posture_set`
- **Placement:** buddy toggles + edge selects + display index
- **Summary:** receipt rows + `panel:done`
- **Idle acts (0, 4):** `section: "none"` closes form; body uses existing bubble/torso affordances

Host owns `primaryPanel` tokens; body never invents them — wired in `src/presenceProtocol.ts` (`PresencePanel` type, law 7 comment).

Replace the stub at `desktop-body/src/main.rs` (~1842–1846):

```rust
presence::Cue::Panel { .. } => {
    // Build C, Slice 1: the wire format and parser land first. The native in-torso
    // onboarding panel that renders this section (and reports clicked{panel:*}) is
    // Slice 3 — until then a panel cue is parsed but not yet drawn.
}
```

### Credential entry (user decision)

**“Paste key” affordance** — pull from clipboard, echoed **masked** as dots. Paste-not-type, masked. Never round-trip the secret to the screen. Receipt records provider/base/model, **never the key value**.

---

## Invariants checklist (non-negotiable)

| Law / rule | How Build C must honor it |
|------------|---------------------------|
| Law 7 | Host maps act→section + `primaryPanel`; body only presents and reports |
| Posture tighten-only | `panel:posture_set` → soul writes via `userPosture.ts`; body never mutates posture locally (settings panel already models this) |
| Receipts | `credential.stored` records provider/base/model, **never key**; each act receipt through existing lifecycle ledger |
| Golden fixture | Additive fields only; older bodies ignore unknown cues |
| Tests | vitest + cargo green throughout |

### Panel events the soul already expects

Body must emit `clicked{panel: ...}` for:

- `connection_ok` — connect section complete
- `posture_set` — posture section complete
- `next` — placement section complete
- `done` — summary / handoff

Transport: additive optional `clicked.panel` field in `src/presenceProtocol.ts` (Build B, `3fab46d`). Host reads as `panel:<name>`.

### Receipt sequence (full walk)

`[null, credential.stored, posture.set, placement.set, null, onboarding.completed]`

---

## Key files

| Area | Path |
|------|------|
| Soul-side Host (pure) | `src/wizardOnboardingHost.ts` |
| Onboarding script + reducer | `src/wizardOnboarding.ts` |
| Panel model (React reference) | `src/onboardingPanelModel.ts` |
| React panel (reference UI) | `components/buddy/OnboardingWizardPanel.tsx` |
| Soul wiring | `scripts/soul-server.ts` (`handleWizardHost`, `emitCues`) |
| Presence protocol | `src/presenceProtocol.ts` |
| Native body stub | `desktop-body/src/main.rs` |
| Native parser | `desktop-body/src/presence.rs` |
| Settings panel pattern | `desktop-body/src/main.rs` (settings rows), `desktop-body/src/render.rs` |
| Wizard script (content) | `docs/WIZARD_ONBOARDING_SCRIPT.md` |
| Memory / prior plan | `memory/build-b-wizard-host-plan.md` (Claude project memory) |

---

## Risk register

| Risk | Mitigation |
|------|------------|
| React ↔ native form drift | Treat `onboardingPanelModel.ts` as spec; add host-side unit tests that snapshot `panel` cue payloads per act |
| Native form UX feels rough | Ship functional first; polish typography/spacing in follow-up |
| Credential handling | `paste_key` only; masked echo; soul stores receipt metadata, not secret |
| Placement config persistence | Open item from wizard script — Act 3 needs a config the body reads (not just env); plan Slice 4 write path explicitly |
| Hub mode after completion | Linear Host is done (`3fab46d`); hub section-jump is Slice 5+ once panel renders |

---

## Immediate next actions (implementer order)

1. ~~**Commit Slice 1**~~ — done (`fb48678`).
2. ~~**Slice 2**~~ — done (`d6c0212`); wire smoke proved section sequence on the socket.
3. **Slice 3** — replace the `Cue::Panel` stub with real draw + hit-test in `render.rs` + `main.rs` (see L2 guardrails below).
4. **Slice 4** — carry the user's choice back so the host writes real settings (additive choice-on-clicked protocol; posture tighten-only).
5. **Slice 5** — E2E: `BB_SOUL=wizard` + `BB_BUDDY=host`, full Act 0→5 without injected clicks; restart → hub mode.
6. **Push `slice-0-launchers`** when ready to publish the stack (optional before Slice 3; not blocking).

---

## Auditor summary

Build B is **done and correctly scoped** — soul-side Host with receipts and handoff is the hard part, and it landed. Slices 1–2 close the protocol + soul-emitter half. The remaining work is **native presentation** (Slice 3) then **settings persistence** (Slice 4).

The fork between Tauri window and native panel is **not** 50/50: given native body as primary, presence protocol as product seam, and `panel` cues now live on the wire, **native in-torso is the only coherent next build**.

**Pickup line for Opus:** Slice 3 — scaffold + posture vertical-slice first; keep vitest + cargo green per section.

---

## Verify commands

```bash
# TypeScript + protocol tests
npm run test

# Native body tests
cd desktop-body && cargo test

# Manual smoke (after Slice 5)
BB_SOUL=wizard npm run soul:dev
BB_BUDDY=host npm run body:dev
```

Keep vitest + cargo green throughout.

---

## L2 checkpoint — `fb48678` + `d6c0212` (2026-06-27)

**Checkpoint accepted.** Slice 1 and 2 are done, green, and the wire smoke closes the main risk before any native draw work: the soul pushes the right `panel` sections in order (`none` → `connect` → `posture` → `placement`) with correct `primaryPanel` tokens. Clean commit boundary — working tree clean, branch ahead 2.

**Recommendation: proceed into Slice 3 now**, if you have a focused block (~2–3 hours). Do not pause for ceremony. The audit's implementer order was exactly this sequence; pausing here only makes sense if you're out of runway or want to push first.

**Optional before you code:** `git push slice-0-launchers` — publishes the protocol + soul-emitter half so Slice 3 body work lands as a separate commit on top. Not blocking.

### Slice 3 guardrails (bite it without boiling the ocean)

Do not ship all four form sections in one diff. Vertical-slice inside Slice 3:

1. **Scaffold first** — `onboarding_open` state, replace `Cue::Panel` stub, shared `interior_rows_for` layout hook, one primary button that emits `clicked{panel: primary_panel}` (even on a stub section).
2. **Posture second** — three option rows + confirm; simplest interaction, no clipboard, proves select → emit loop end-to-end with soul.
3. **Connect third** — `paste_key` + masked echo + provider rows (hardest interaction).
4. **Placement + summary last** — toggles/selects, then receipt rows.

After step 2 you should be able to walk Act 2 on a real body without injected clicks. That is the first meaningful user-visible win.

**Law 7 holds throughout:** body stores draft locally for rendering only; on primary confirm it emits `clicked{panel: …}` with the Host's token — never invents it. Slice 4 owns what travels back on that click for settings writes.

**Model explicitly on:** settings panel (`main.rs` ~2670–2715) + tucked input from `cadff5e` for any text-ish affordance. Hit-test and draw must share the same row rects — the settings panel already learned that lesson.

### When to pause instead

- After posture vertical-slice lands and cargo is green — good micro-checkpoint before connect/clipboard.
- If the torso panel is too cramped for connect fields — flag it; do not squeeze UX into a hack without a one-line decision.

**Word given: go Slice 3.** Start with scaffold + posture; keep vitest + cargo green per section.

---

## L3 checkpoint — Slice 3 native render landed (2026-06-27, uncommitted)

**What landed:** `draw_onboarding_view` + `OnboardingLayout` + `onboarding_hit_at` in `render.rs`; `OnboardingPanelState` + `Cue::Panel` handler + option/field/primary press routing + masked `paste_key` echo in `main.rs`; `clicked_panel_json` in `presence.rs`. Gates: cargo 90, vitest 253. Law 7 holds — primary confirm only echoes the Host-stamped `primary_panel` token; the body never invents one.

**Manual smoke verdict (BB_SOUL=wizard + BB_BUDDY=host): the interface does NOT render correctly and is hard to operate.** Confirmed by hand on the running body, not just inferred from the code:

- **Selections reset to Host defaults after every confirm.** This is the expected Slice 4 gap, now observed: pick a provider on `connect`, click Primary, the next `Cue::Panel` rebuilds `option_selected` from `o.selected` and the user's pick is gone. The body reports nothing back on `clicked{panel:*}`, so the Host cannot know what was chosen and re-sends the section with original defaults. **Net effect: Act 2 onward cannot be driven by a real user.** Clicking Primary advances the act token, but the choice that motivated the advance is silently lost.
- **Render quality is below the settings panel bar.** The torso card is cramped; option rows, masked field rows, and the "Paste key" chip fight for the same horizontal strip; the primary button sometimes overlaps the last field row at realistic torso sizes. Hit-test still matches what was drawn (the layout contract holds), but the result is not legible enough to call user-ready.
- **Text fields are not drivable.** `on_onboarding_field` only handles `paste_key`; `text`/`select` controls set `focused_field` with no keyboard path. `input_focused = false` is set on every onboarding press, so the bubble keyboard never routes into the panel. Anything other than `paste_key` is effectively read-only.
- **Every `Cue::Panel` re-push wipes drafts, even for the same section.** Confirmed by behaviour: any soul re-emission of the same `panel` cue resets `option_selected` from `o.selected` and `field_drafts` from `f.value`. If the soul ever re-sends `panel` outside "user clicked primary" (heartbeat, posture change, surface hydrate), in-progress edits vanish silently.

**Why this still ships as Slice 3:** the law-7 boundary is clean, the layout/hit-test contract is tested, and the render path is the gate for Slice 4 — without a drawn panel there is nowhere to carry choices back from. The render and UX gaps are real and now on the record, but they are Slice 4's surface, not a regression in the trust core.

**Adjusted pickup line for Slice 4:**

1. **Carry choice back on `clicked{panel:*}`** — extend the wire so the body echoes the user's option/field drafts alongside the token. Additive optional field, mirrors Slice 1's transport pattern. This is the single fix that turns the panel from "advances the act but loses the choice" into a real form.
2. **Stop resetting drafts on same-section re-push.** Diff incoming `section` against current in `OnboardingPanelState` and carry `option_selected`/`field_drafts` across when the section hasn't changed; or make the soul-side contract "MUST NOT re-emit `panel` for the same section except in response to `clicked{panel:*}`." Pick one before Slice 4 ships.
3. **Decide the `text`-control scope.** Either wire a body-local keyboard route into `focused_field`, or narrow Slice 4 to "`paste_key` + option/select controls are user-touchable; `text` stays host-driven via re-emitted `value`."
4. **Render polish pass.** The torso card needs more room for option rows + masked field + Paste chip + primary button. Treat as a real UX task, not a tweak — possibly revisit `right_reserve` / row heights / `output_panel_rect` budget, with a one-line decision recorded here if a non-obvious trade-off is needed.
5. **Pin toggle semantics with a test.** `single_select()` returns `false` for `placement` and `true` for `connect`/`posture`; `on_onboarding_option` branches on it. Add a unit test so a future refactor can't silently invert placement multi-select vs posture single-select.
6. **State the degenerate-layout contract.** When `content.h < 24.0`, `onboarding_layout` returns an empty layout; `draw_onboarding_view` bails at `card.h < 8.0`. One assertion makes the 8–24 gap explicit.

**Reconcile doc/behaviour drift in `PanelOption.selected`:** the comment says "never authoritative — the body reports the user's actual pick (law 7)." Today the body reports nothing back, so `o.selected` is in practice the only source of truth the renderer sees. Slice 4 step 1 closes the gap; until then the comment overstates the contract.

**Commit message guidance for Slice 3:** name the law-7 contract and the Slice 4 boundary in the body — `Body draws panel cues and echoes the Host's primary_panel token on confirm; never invents it (law 7). Drafts are render-only and reset on each Cue::Panel — Slice 4 carries them back.` **Do NOT claim a working E2E walk in the commit message.** Smoke confirmed it does not work yet.

---

## L4 checkpoint — Slice 4 choice carryback landed (2026-06-27, uncommitted)

**What landed:**

| Layer | Change |
|-------|--------|
| Wire | Additive `clicked.panelChoices{selectedOptionIds, fieldValues}` in `presenceProtocol.ts` (strict-parse) |
| Body | `clicked_panel_json` echoes drafts on primary confirm; `OnboardingPanelState::apply_cue` preserves drafts on same-section re-push |
| Soul | `wizardHostDraft.ts` + `wizardDrafts` map; `applyPanelChoices` on `clicked{panel:*}`; `actPanel(state, receipts, draft)` reflects user picks; lifecycle receipt detail from draft (no API key value); `wizardSessionPosture` after `posture_set` |
| Render | Primary band reserved (22px + gap); option/detail/field chip layout polish |
| Tests | vitest 260 · cargo 92 |

**Text-control scope (step 3 decision):** `paste_key` + option/select are user-touchable; `text` fields stay host-driven via re-emitted `value` — no body keyboard route in Slice 4.

**Still open (Slice 5):** full native Act 0→5 E2E without injected clicks; manual smoke after commit. Render may still need another pass once choices persist correctly.

**Suggested commits (two, in order):**

1. `feat(body): Build C Slice 3 — draw onboarding panel; echo Host token + panelChoices on confirm (law 7)`
2. `feat(soul): Build C Slice 4 — apply panelChoices to wizard draft; honest receipt detail; posture session`

**Pickup line for Slice 5:** `BB_SOUL=wizard` + `BB_BUDDY=host` manual walk — confirm selections survive confirm and Acts 1–3 advance without injected clicks.

### L4 audit verdict (2026-06-27) — land both commits

Independent re-read of wire/strict-parse, body `apply_cue` + `click_choices`, soul `wizardHostDraft` + `actPanel` re-emit, and gates (cargo 92, vitest 260, tsc clean) **confirms land**. Slice 4 closes the L3 blocker; law 7 holds.

**Recorded concerns (non-blocking):**

| Item | Note |
|------|------|
| Credential not passed to Hermes | `wizardDrafts` is in-memory; `handoffToHermes` does not consult it yet. Receipt records `apiKeyPresent` only. |
| `wizardSessionPosture` is module-global | Last `posture_set` wins for gate calls — one Host at a time is the demo reality. |
| `wizardDrafts` not GC'd on disconnect | Draft map entry survives buddy disconnect for process lifetime. Post-v0.1. |
| Field reset semantics | Documented in `wizardHostDraft.ts`: apiKey absent → keep prior; model absent on preset switch → preset placeholder; placement empty → keep prior buddies. |

**Slice 5 decision (recorded):** Pass the draft into `handoffToHermes`; Hermes `hydrate` carries the key **in-process, memory-only** (smallest fix). **Do not log the hydrate payload** — it will carry `apiKey`; existing `log("wizard complete → handoff…")` only logs `{ host }`. Receipt `detail` path remains safe (no key by construction).

**Run tasks added:** VS Code → Run Task → **BB wizard host (onboarding)** (`scripts/bb-wizard-host.sh`) · **BB forge + soul + frame** (`scripts/bb-forge-stack.sh`). npm: `npm run wizard:host` · `npm run forge:stack`.