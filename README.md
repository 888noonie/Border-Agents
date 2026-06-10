# Border Buddies

**Your friendly miniature AI companions living at the edge of your screen.**

Imagine a team of unique, customizable little characters — heads peeking playfully from the borders of your workspace. They bob, glance, and wait patiently until needed. When something important happens (memory graded, claim verified, artifact ready to share), the right agent pops up with a cheerful, expandable speech bubble.

**Tap or drag any head** to bring them front and center for quick actions, receipts, or celebrations.

---

**Make your AI tools feel visible, personal, and *delightful*.**

Border Buddies is the warm, personality-first experience: cute miniature companions that visually represent your LLM chats, agents, subscriptions, projects, and workflows. One buddy might represent Codex, another Claude, another Grok, and others can belong to custom agents or specific projects.

Border Agents remains the edge-native governance layer underneath. Deep deterministic governance (memory grading, Safe Context Frames, purpose policies, receipts, artifact hashing) powers the buddies to be truly trustworthy companions rather than just cute mascots.

The UX — friendly, reactive, and human — is what makes reliability and safety *feel good* and addictive to use.

## The Principle

«A system can possess knowledge without being authorized to act upon it.»

AI work should not silently cross from possibility into use. Border Agents makes every crossing visible, inspectable, and governed — through delightful characters.

## Architecture — one soul, many bodies

A buddy is two separable things:

- **The soul** — a headless agent runtime (an LLM, an agent platform, a scripted
  host). It reasons, decides, and acts.
- **The body** — a dumb presence surface that *renders* the soul: where it sits,
  how it feels, what it says. The same soul can wear many bodies (a native desktop
  buddy, a browser content-script buddy, one day a phone), each implementing a small
  typed **presence protocol** (`src/presenceProtocol.ts`).

Why split them? Portability (porting a body is ~5 cues + ~6 events, not a whole
runtime) and — more importantly — **trust**. The boundary is a law:

> **Bodies present; souls act.** Screen perception and screen action are *governed
> effectors of the soul*, routed through Core Patrol with receipts — never
> capabilities of the body. The body only expresses what the soul does; it never
> reads or acts on the screen itself. (AGENTS.md non-negotiable law 7.)

The native desktop body is a pure-Rust renderer on a `wlr-layer-shell` overlay
surface (`desktop-body/`, no GTK/WebKit/GPU) — the outcome of the overlay rebuild
documented in `docs/OVERLAY_POSTMORTEM_AND_REBUILD_PLAN.md`. Normal panel windows
(settings, Trust Workbench) still use the Tauri webview; only *transparent overlay*
surfaces were the dead end.

## Postures — Work · Play · Private

One choice sets your whole stance, as friendly sugar over the governance core
(`src/core/userPosture.ts`). A posture can only ever *tighten* trust, never widen
it: **Private** clamps every purpose to trusted/public/strict; **Work** is the
balanced baseline; **Play** keeps Work's exact authorization but lowers interaction
friction. Medium- and high-risk actions always ask for confirmation, in every
posture — "Play" relaxes interruptions, never the trust boundary.

## The Wizard — your go-to setup buddy

The first body you meet is a **Wizard**: it onboards you (connect your model, pick a
posture, place your buddies) and stays the single go-to surface for any later setup
or UX tweak. It's a scripted *soul persona* driving the ordinary body, with forms in
a normal panel window — design in `docs/WIZARD_ONBOARDING_SCRIPT.md`.

## Core Buddies — Your Default Team

Each buddy is a miniature, expressive character with its own personality.

| Agent      | Border Guarded       | Role |
|------------|----------------------|------|
| **Nexus**  | Memory → Context     | Grades retrievals, shows trusted/limited items |
| **Veritas**| Context → Claim      | Verifies facts, attaches evidence receipts |
| **Forge**  | Intent → Action/Code | Reviews tool calls & code changes |
| **Strategos** | Idea → Plan       | Checks plans, risks, sequencing |
| **Nova**   | Draft → Artifact     | Polishes & packages creations |
| **Aether** | Mess → Structure     | Synthesizes ideas into systems |
| **Conductor** | Unresolved → Decision | Orchestrates approvals & handoffs |

**Key UX Features:**
- Heads tucked in screen borders (customizable position & visibility)
- Expandable speech bubbles with quick-reply actions
- Drag to summon full panel
- Celebration micro-moments (e.g. crab dance on safe share)
- Full customization: appearance, voice lines, triggers

## Run On Pop!_OS Desktop

**The native presence body** (the buddy on your screen edge) lives in `desktop-body/`
and renders on a `wlr-layer-shell` overlay surface. In VS Code, run the tasks:

```text
BB body start      # build + launch the native buddy
BB body restart    # rebuild and relaunch
BB body stop       # stop it
```

Or directly:

```bash
cd "$HOME/TETRATHEDRAL/Border Agents/Border-Agents/desktop-body"
cargo run --release
# BB_OUTPUT_INDEX=1 cargo run --release   # second monitor
```

**The Tauri panel host** (settings, Trust Workbench, and other normal windows —
*not* the transparent overlay, which the rebuild retired) runs with:

```bash
source "$HOME/.cargo/env"
npm run desktop:dev          # VS Code task: BB start  /  stop: BB stop
npm run desktop:build        # installable bundle
```

**The dev gateway** (a stand-in "soul" speaking the presence protocol) and the
**browser-only preview**:

```bash
npm run gateway:dev          # VS Code task: BB gateway
npm run dev                  # browser preview  (VS Code task: BB browser preview)
```

Governance ensures they are reliable. Playfulness ensures you'll love having them around.
