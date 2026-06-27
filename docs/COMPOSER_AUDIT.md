# Composer Audit — Wizard Onboarding Panel (Build C)

**Date:** 2026-06-27  
**Branch:** `slice-0-launchers`  
**Audience:** Opus (or any implementer picking up the deferred onboarding-panel work)  
**Role:** Auditor / advisor composer output from session 2026-06-27

---

## Session state at handoff

**Committed (not pushed):**

| Commit | What it proves |
|--------|----------------|
| `3fab46d` | Build B soul-side is real: `wizardOnboardingHost.ts` + `soul-server.ts` under `BB_SOUL=wizard`, Acts 0–5, lifecycle receipts, `clicked.panel?` transport, headless E2E walk + handoff |
| `cadff5e` | Body UX polish: direct unpin, tucked peek cycle (HeadOnly → Bubble → BubbleInput) |
| `6ce23b0` | Soul-gated pin + bubble honesty |

**Quality gates at last session:** tsc clean · vitest 245 · cargo 87

**Still deferred:** anything a real user can touch for connect / posture / placement / summary. Form acts only advance on injected `panel:*` clicks today. A real user can drive Act 0 (`clicked`) and the `find_me` timeout, but not the form sections.

**In flight (uncommitted at audit time, ~436 lines):** Build C Slice 1 — the `panel` presence cue wire format:

- `src/presenceProtocol.ts` — `PresencePanel`, strict-parse validators
- `desktop-body/src/presence.rs` — `Cue::Panel { ... }` parser
- `desktop-body/src/main.rs` — stub handler (“parsed but not yet drawn”)
- Tests + golden fixture extension (`fixtures/presence-v0.json`, `presenceProtocol.test.ts`)

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
Slice 1  [in progress, uncommitted]  panel cue protocol (TS + presence.rs parser + fixture)
Slice 2  soul emits panel cues       wizardOnboardingHost maps act→section; soul-server calls emitCues + panel
Slice 3  native render + input       render.rs draw_onboarding_view; main.rs state/draft/hit-test/clipboard
Slice 4  settings writes             on panel:* receipt paths: credential (no key value), posture (tighten-only), placement
Slice 5  E2E verify                  BB_SOUL=wizard + BB_BUDDY=host, full Act 0→5, no injected clicks; restart → hub
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

1. **Commit Slice 1** — panel protocol + parser + tests (was uncommitted at audit time).
2. **Slice 2** — teach `emitCues` / `handleWizardHost` to push `panel` envelopes when `currentAct(state).panelSection !== "none"`.
3. **Slice 3** — replace the `Cue::Panel` stub with real draw + hit-test in `render.rs` + `main.rs`.
4. **Slice 4** — wire settings writes on receipt paths (posture tighten-only intact).
5. **Slice 5** — E2E: `BB_SOUL=wizard` + `BB_BUDDY=host`, full Act 0→5 without injected clicks; restart → hub mode.
6. **Push `slice-0-launchers`** when ready to publish the stack.

---

## Auditor summary

Build B is **done and correctly scoped** — soul-side Host with receipts and handoff is the hard part, and it landed. The deferred piece is **presentation**, not governance.

The fork between Tauri window and native panel is **not** 50/50: given native body as primary, presence protocol as product seam, and Slice 1 already encoding `panel` cues, **native in-torso is the only coherent next build**.

**Pickup line for Opus:** commit Slice 1, then implement Slice 2 (soul emits `panel` cues from act state).

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