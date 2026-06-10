# Wizard Onboarding Script — design (pre-wire)

**Status:** design only. No wiring this session. This is the content + flow the
Step 4 presence wire and the settings webview panel must support.
**Branch:** `presence-layer`. **Date:** 2026-06-10.

## Premise

> The first body you meet is a **Wizard**: it onboards you, and thereafter is the
> single go-to surface for any setup / settings / UX tweak.

Architectural commitments (locked):

- The Wizard is a **soul/persona + a script**, *not* a special body. It puppets the
  same dumb animated body every other soul uses (`set_emotion`/`say`/`toggle_menu`
  seam in `desktop-body/src/main.rs`). No renderer fork — "one soul, many bodies"
  stays intact.
- The **body is the face/launcher**. Greets, emotes, points, demonstrates placement.
- The **forms live in a normal opaque webview panel window** (React, reuses existing
  UI). The overlay-rendering ban is on *transparent overlay* surfaces only; a normal
  panel window is fine.
- **First run = linear script** (Acts below). **Every run after = same panel opened
  as a settings hub** (jump to any section). This is what makes the Wizard the
  permanent "go-to," not a one-shot.

## Personas & handoff

The Wizard is a lightweight **Host** persona (neutral, calm, brief) — *not* Hermes.
Hermes is a model-owned companion (Grok); onboarding shouldn't be branded to a
provider the user hasn't connected yet. When onboarding completes, the Host **hands
off**: it summons the real companion soul (Hermes by default) and dismisses itself.

> Open question (small): build a dedicated `host` character manifest, or run the Host
> as an unbranded default skin of the generic body. Recommend: generic body skin for
> now (no new asset), give it a name in copy only.

## Cue & feedback vocabulary (fixed by `src/presenceProtocol.ts`)

To body: `move_to(position)` · `express(emotion[,intensity])` · `say(text)` ·
`attention(...)` · `hydrate(snapshot)`.
Emotions: `neutral happy thinking curious alert sleepy`.
From body: `clicked grabbed dragged dropped summoned dismissed`.

Each Act below lists: **body cues**, **panel action**, **user input**, **real config
target**, **advancing body→soul event**, and the **governance receipt** it emits
(Phase 3 tie-in — every setting write produces a receipt via `receiptLedger.ts`).

---

## Act 0 — First contact  (no forms)

- **Body:** `move_to(anchored: right edge)` fade-in → `express(curious)` →
  `attention(user)` → `say("Hi — I'm your setup host. Two minutes to get you wired up. Ready?")`.
- **Panel:** closed. Bubble shows one affordance: **“Let's set up”** (and a quiet
  “Skip for now”).
- **User input:** click buddy / click “Let's set up”.
- **Config target:** none.
- **Advancing event:** `clicked` → Host opens the panel at Act 1.
- **Receipt:** none (no state written).

## Act 1 — Connect your engine  (Hermes provider)

- **Body:** `express(thinking)` while a test call runs → `express(happy)` on success /
  `express(alert)` + `say("That key didn't take — want to retry?")` on failure.
- **Panel section `connect`:** provider dropdown `xai | openrouter | lmstudio | ollama`
  (prefills `HERMES_API_BASE` per provider, per `.env.example`), API key, model,
  optional system-prompt override. **“Test connection”** button.
- **User input:** provider creds + Test.
- **Config target (real):** `.env` → `HERMES_PROVIDER`, `HERMES_API_BASE`,
  `HERMES_API_KEY`, `HERMES_MODEL`, `HERMES_SYSTEM_PROMPT`.
- **Advancing event:** panel “connection ok” → Host says success line, advances.
- **Receipt:** `credential.stored` (record *that* a provider key was set + which
  provider/base/model; **never the key value**).

## Act 2 — Choose your posture  (Work / Play / Private — NEW sugar)

Friendly wrapper over the real governance knobs (`PURPOSE_POLICIES`, `RenderMode`).
Proposed mapping (the Wizard's only net-new concept):

| Posture     | Feel                  | Default render_mode | Grades in prompt        | Sensitivity ceiling | Agent actions |
|-------------|-----------------------|---------------------|-------------------------|---------------------|---------------|
| **Private** | Locked down           | `strict`            | `trusted` only          | `public`            | ask every time |
| **Work**    | Balanced (default)    | `annotated`         | `trusted`,`limited`     | `internal`          | ask on high-risk |
| **Play**    | Relaxed, low-friction | `clean`             | `trusted`,`limited`     | `internal`          | ask on high-risk |

- **Body:** `express` mirrors the pick — Private→`alert`, Work→`neutral`, Play→`happy`.
  `say` a one-line consequence (“Private: nothing leaves without your say-so.”).
- **Panel section `posture`:** three cards (Private/Work/Play) with the consequence
  copy; “Advanced” disclosure reveals the raw purpose×render-mode table for power users.
- **User input:** pick one (Work preselected).
- **Config target:** new `user_posture` default that seeds policy lookups; maps onto
  existing `PURPOSE_POLICIES` / `RenderMode` — does **not** replace them.
- **Advancing event:** panel “posture set”.
- **Receipt:** `posture.set` (posture + resolved render_mode + grade allowance — fully
  deterministic, matches the “every override = a receipt” law).

## Act 3 — Place your buddies  (delight beat — exercises `move_to`)

- **Body:** as the user picks an edge, the body literally **`move_to(anchored: <edge>)`**
  to demo it live; `express(curious)`. Picking a monitor re-anchors on that output.
- **Panel section `placement`:** which characters enabled (`hermes`,`owl`,`crab`),
  per-character border side, target monitor (output index).
- **User input:** toggles + edge/monitor.
- **Config target (real):** character enable set + `border_position` per manifest;
  body placement = output index + anchor/margin (today `BB_OUTPUT_INDEX`,
  `BB_MARGIN_*` — Wizard should write these to a config the body reads, not env).
- **Advancing event:** `dropped` (if the user drags the demo body) or panel “next”.
- **Receipt:** `placement.set`.

## Act 4 — Where to find me  (no forms — teaches the gesture)

- **Body:** `move_to(anchored: <chosen edge>)` tuck → `say("Tug me out whenever you
  need me. Click me to reopen this anytime.")`.
- **User input:** optional drag (the teaching moment).
- **Advancing event:** `grabbed`→`dragged`→`dropped` confirms they learned it; else a
  timed advance.
- **Receipt:** none.

## Act 5 — Done & handoff

- **Body:** `express(happy)` → `say("You're set. Bringing in your companion now.")`.
- **Panel section `summary`:** read-only list of what was set, each row linking to its
  **receipt** (this is the first real consumer of the Trust Workbench receipt view).
- **Handoff:** Host emits `dismissed` for itself; soul layer `summon`s the default
  companion (Hermes) which `hydrate`s into the same body.
- **Receipt:** `onboarding.completed`.

---

## Re-entry (the "go-to" contract)

After first run, `clicked` on the buddy opens the **same panel as a settings hub**
(section nav: Connect · Posture · Placement · Receipts), not the linear Act flow.
First-run vs hub is decided by an `onboarding.completed` receipt existing.

## Wiring contract this implies (for Step 4, next session)

1. Body already exposes `set_emotion`/`say`/`toggle_menu`; Step 4 maps inbound
   `express`/`say`/`move_to`/`hydrate` → those, and emits
   `clicked`/`grabbed`/`dragged`/`dropped`/`summoned`/`dismissed` from pointer logic.
2. The **panel ↔ soul** channel reuses the existing gateway WebSocket
   (`ws://127.0.0.1:17387`, `scripts/gateway-dev.mjs`). The onboarding Host replaces
   the dev echo soul as the first scripted driver.
3. Settings writes must each go through `receiptLedger.ts` so the Act receipts above
   are real, not cosmetic.

## Open decisions (carry forward)

- Host persona: dedicated `host` character asset vs unbranded body skin. *Lean: skin.*
- Where the body reads placement/posture config (a `config.json` the Rust body loads
  vs continuing to use env). Needed before Act 3 can actually move buddies on save.
- Posture table values above are a proposal — confirm before they become policy.
