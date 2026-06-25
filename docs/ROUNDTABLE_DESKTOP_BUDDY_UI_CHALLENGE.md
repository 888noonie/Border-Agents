# Roundtable Challenge: One Desktop Buddy UI

## Competition Brief

Border Agents is inviting top AI systems to compete for bragging rights:

> **Design an ultra-modern, intuitive GUI for one desktop buddy, based on the two supplied screenshots.**

The winning design will be credited in the implementation commit and in supporting docs if it is used.

Repository:

```text
https://github.com/888noonie/Border-Agents
```

Current status:

- UI changes are not committed yet.
- The app currently has a Rust native desktop body and a TypeScript governance soul.
- There should be **one desktop buddy** on screen, not a swarm of disconnected widgets.
- That one buddy can cycle through many roles, surfaces, providers, gateways, and tool hosts.

Use the two supplied screenshots as visual input:

1. **Image 1:** current/earlier desktop buddy with speech bubble, torso card, input, and Review button.
2. **Image 2:** newer perimeter-switcher buddy with edge controls, quick links, torso output, and Aether local-chat identity.

You may propose a complete visual redesign if it improves the product. The buddy does not need to stay clay/orange/Morph-like, but it must remain a legible, lovable, screen-respecting presence.

## Product Goal

Design the best possible **single on-screen buddy interface** for Border Agents: a tiny desktop companion that keeps the user's space organized while giving governed control over a fast-changing AI landscape.

The buddy should make it easy to:

- Switch between personas/categories such as:
  - **Forge:** code, build, repo work, terminal hosts.
  - **Veritas:** claims, policy, receipts, evidence.
  - **Nexus:** gateways, accounts, browser buddy links, connectors.
  - **Aether:** private local LLM / LM Studio / Ollama.
  - **Nova:** creative artifact generation.
  - **Hermes/Wizard:** onboarding, guidance, setup.
- Highlight the active browser buddy, tools, gateway, model, and posture.
- Launch host services after confirmation, for example:
  - `/codex`
  - `/a0`
  - `/hermes`
  - `/ollama`
  - `/lmstudio`
  - `/claude-code`
- Provide access to:
  - Browser buddy link / active browser tools.
  - Gateways and provider routes.
  - Governance receipts and Core Patrol decisions.
  - API keys and provider settings.
  - General settings.
  - UX toggles.
  - Voice, vision, TTS/STT, and local model options.
- Stay out of the way when idle.
- Become obvious and inspectable when the user must confirm, block, review, or choose a route.

## Core Philosophy

Border Agents is not a generic agent launcher. It is a visible interface and governance layer for AI work.

The design must respect these laws:

1. **Similarity is not authority.**
2. **Relevant does not mean allowed.**
3. **Retrieval must be preserved.**
4. **Authorization must be graded.**
5. **Prompt context must be purpose-aware.**
6. **Every grade/action must produce a receipt.**
7. **Bodies present; souls act.**

The desktop buddy is a body. It may render, show mood, collect user input, and request actions. It must not silently read the screen, click the screen, launch providers, write files, or perform tools by itself. Those are soul/Core Patrol decisions and require receipts.

Design implication:

- The buddy can look magical.
- The authorization model must be plain and inspectable.
- The face can express a decision.
- The body can request a tool.
- The soul decides whether that request runs.

## Challenge Question

How do we fit all of this into **one small desktop buddy** without turning it into clutter?

Design a UI that answers:

- Where does the user see the active persona/category?
- Where does the user see the active model/provider/gateway?
- Where does the user see the active browser buddy/tools?
- Where do governed actions appear?
- Where do settings live without becoming a giant panel?
- How does the buddy switch between local/private, browser, repo, terminal, voice, and vision surfaces?
- How does the buddy stay small while still giving power users deep control?
- How does the design scale when providers are suspended, degraded, legally blocked, rate-limited, or dishonest in their claims?

## Required Screen Modes

Design at least three screen modes. Each mode must correspond to real actions or states, not just decoration.

### 1. Tucked

Lowest visual footprint. The buddy is parked at a screen edge as a small bump/tab.

Must show:

- Identity or color cue.
- Alert level if something needs attention.
- One gesture/click to summon.

Use case:

- User is working and wants the buddy out of the way.

### 2. Docked

Compact active mode. The buddy is visible with face, torso/status, and lightweight controls.

Must show:

- Current persona/category.
- Current posture: Work / Play / Private.
- Current provider/gateway/model.
- Alert state.
- Input or command affordance.
- Mode switcher or cycle controls.

Use case:

- User is chatting, switching surfaces, confirming actions, or checking status.

### 3. Expanded / Framed / Host Mode

Larger active mode for real work.

Possible interpretations:

- Stretch torso into a larger output panel.
- Frame a browser/tool window.
- Attach to a terminal host.
- Open a governed settings/workbench panel.
- Display receipts, route details, or active browser tools.

Must show:

- What surface is active.
- What route/provider powers it.
- What the buddy is allowed to do.
- What is blocked or pending confirmation.

Use case:

- User is actively controlling tools, reviewing receipts, connecting gateways, or running local/private models.

## Alert / Trust Levels

Design a five-level alert system that can be read at a glance. It should map onto face/body/chrome without becoming noisy.

Suggested levels:

1. **Quiet:** tucked or idle, no attention needed.
2. **Ready:** output or reply is available.
3. **Confirm:** user must explicitly approve before action.
4. **Blocked:** action refused or surface unwired.
5. **Critical:** privacy/security boundary, route downgrade, provider failure, or policy danger.

The exact labels can change, but the design must distinguish:

- Normal idle.
- Thinking/running.
- Needs confirmation.
- Blocked/refused.
- Provider degraded/unavailable.

## Physical Control Layers

The screenshots already suggest a body with parts. You may keep, refine, or replace this, but the design should consider physical affordances:

- **Face:** emotional/trust state layer.
  - Happy/ready.
  - Curious/needs confirmation.
  - Alert/blocked.
  - Thinking/running.
  - Sleepy/tucked.
- **Torso:** main surface/output/status.
  - Text.
  - Image.
  - File.
  - Session card.
  - Receipts.
  - Provider/gateway state.
- **Legs:** stretch/resize control.
  - Drag legs to lengthen/shorten torso.
  - Stretch into expanded mode.
  - Squash back to compact mode.
- **Arms:** optional action handles.
  - Point to active target.
  - Grab/hold a framed window.
  - Confirm/cancel gestures.
  - Toolbelt/menu reveal.
- **Eyes:** optional vision/image layer.
  - Image inspection.
  - Screen attention indicator.
  - Vision enabled/disabled.
- **Ears:** optional audio input layer.
  - STT listening state.
  - Whisper/Vokel-style voice pipeline.
  - Push-to-talk.
- **Mouth:** optional speech output layer.
  - TTS active/paused.
  - Voice reply.
  - Mute.

If you use eyes/ears/mouth as controls, make the controls obvious and reversible. The buddy must not imply it is listening, seeing, or acting unless the soul has explicitly enabled that governed effector.

## Required Surface Switcher

The buddy needs a way to cycle through surfaces/categories. The current prototype uses a perimeter ring with:

- N/E/S/W arrows.
- Corner quick-links.
- A trailing `+` for customization.
- Border controls for paste/review/edit.

You may redesign this. Requirements:

- One-handed, low-friction switching.
- Clear active surface.
- Clear unavailable/unwired surfaces.
- A way to reach customization/settings without clutter.
- A way to expose browser buddy tools and full browser buddy link.
- A way to show provider/gateway routing.

Surface examples:

```text
session
private_local_chat
browser_buddy
forge_code
veritas_receipts
nexus_gateways
claude_code
codex
agent_zero
ollama
lm_studio
hermes_wizard
voice
vision
settings
governance
```

## Settings Challenge

The design must explain how one small buddy can contain or link to:

- General settings.
- UX toggles.
- Alert-level preferences.
- Provider settings.
- API keys.
- Gateway routing.
- Browser buddy link.
- Active browser tools.
- Local model settings.
- Terminal host launchers.
- Voice/STT/TTS settings.
- Vision/image settings.
- Receipts and governance logs.

Avoid a giant always-open settings panel. Prefer progressive disclosure:

- Tucked: tiny signal.
- Docked: essential controls.
- Expanded: surface-specific controls.
- Workbench/settings: deep configuration.

## Governance Requirements

Every proposed interaction must identify whether it is:

- Presentation only.
- A request to the soul.
- A Core Patrol/gate decision.
- A confirmed action.
- A blocked action.
- An execution receipt.

Examples:

- Clicking `/codex` should request a host launch, not silently launch it.
- Switching to private local chat should confirm once, then converse locally.
- A route downgrade from local to cloud must be visible and require confirmation.
- An unwired surface should look blocked/unfinished honestly.
- A browser buddy link must show which browser/tool context is active.

## Deliverables

Please provide:

1. **One hero concept**
   - Name the design.
   - Explain the core metaphor.
   - Describe how it differs from the screenshots.

2. **Annotated UI mock**
   - Text description is acceptable.
   - ASCII layout, wireframe, SVG, HTML/CSS, or image prompt is welcome.
   - Include tucked, docked, and expanded modes.

3. **Interaction map**
   - What does each body part/control do?
   - How does cycling work?
   - How do settings open?
   - How do browser tools appear?
   - How do terminal hosts launch after confirmation?

4. **State matrix**
   - Idle.
   - Thinking.
   - Reply ready.
   - Needs confirmation.
   - Blocked.
   - Provider unavailable.
   - Private/local active.
   - Browser buddy active.
   - Terminal host active.

5. **Governance map**
   - Which actions are presentation-only?
   - Which actions require soul authorization?
   - Which actions require confirmation?
   - Which actions produce receipts?

6. **Implementation notes**
   - How this could fit the current Rust body and TS soul.
   - What protocol cues may be needed.
   - What should stay out of scope.

## Judging Rubric

100 points total:

- **Clarity and intuition:** 20
  - Can a user understand active mode, active provider, and current trust state quickly?
- **Screen respect:** 15
  - Does the buddy stay useful without occupying too much space?
- **Governance legibility:** 20
  - Are confirmation, blocked states, receipts, and route changes visible and honest?
- **Surface scalability:** 15
  - Can it handle many personas, tools, gateways, settings, and hosts without collapsing?
- **Visual quality:** 15
  - Does it feel modern, delightful, and coherent?
- **Implementation realism:** 10
  - Could this be built incrementally in the current Rust/TS architecture?
- **Inventiveness:** 5
  - Does it reveal a new, better way to think about the buddy?

Bonus credit:

- Elegant browser buddy integration.
- Great five-level alert system.
- Brilliant use of arms/eyes/ears/mouth without violating governance.
- Excellent private/local model UX.
- Strong plan for provider failures, route degradation, and legal/platform instability.

## Response Template

Use this structure:

```markdown
# Design Name

## One-Sentence Pitch

## Visual Concept

## Tucked Mode

## Docked Mode

## Expanded / Host Mode

## Body-Part Controls

## Surface Switcher

## Browser Buddy + Tools

## Settings / Gateways / Governance

## Voice / Vision / Terminal Hosts

## Five-Level Alert System

## Governance Map

## Implementation Notes

## Why This Should Win
```

## Final Instruction To Contestants

Be bold. The screenshots are a starting point, not a cage.

The best design will make one small buddy feel like:

- a trustworthy control surface,
- a live route/status indicator,
- a governed launcher,
- a receipt-aware assistant,
- and a delightful presence that gives the user control as the AI platform landscape shifts beneath them.

The buddy may be cute, strange, premium, minimal, robotic, clay-like, glassy, cosmic, or something entirely new.

But it must be honest.

