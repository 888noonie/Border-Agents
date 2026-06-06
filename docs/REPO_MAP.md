# Border Agents — Repository Map

**Purpose**: A living navigation guide to explore, understand, and extend the project without getting lost. Follow this to onboard quickly and make progress aligned with project laws.

**Top-level principle (from AGENTS.md)**:
> same vector results + different purposes = different authorized Safe Context Frames

The project makes AI trust boundaries visible and inspectable via friendly "Border Buddies" (desktop edge overlays) backed by deterministic governance.

**Current reality check**: The desktop overlay + buddy UX is advanced and Linux-hardened. The core governance primitives (MemoryPacket, MemoryGrader, SafeContextFrame, receipts, PurposePolicy) exist only in specs/docs — **not yet implemented in code**. UI contains placeholder "memoryMode" settings. Per AGENTS.md, prove the grading primitive before expanding.

**Live status update (2026-06-07)**: The first Hermes UX concept is working. Desktop Hermes can be interacted with through the border overlay, the local gateway can route real OpenAI-compatible provider replies, and the browser preview/extension path can connect to the same Hermes gateway. Significant UI extraction work has occurred since this map was written (new `components/buddy/BuddySurface.tsx`, `BuddyPanel.tsx`, `BuddyUiBubble.tsx` etc. with their own `measureHitboxes` + layout-driven reporting). The big `BorderDock.tsx` is no longer quite as monolithic. See [docs/FIX_LIST.md](./FIX_LIST.md) for the resolved blocker history and remaining polish.

**Current top blocker**: No code implementation yet for the v0.1 deterministic memory grading primitive (MemoryPacket, PurposePolicy, MemoryGrader, SafeContextFrame, PromptRenderer, GradeReceipt). The UX proof is now strong enough to return focus to the AGENTS.md law: prove the grading primitive before expanding into a general agent framework.

See FIX_LIST.md for full history of the attempt, current code locations, exact test sequence, and continuation steps.

---



## 1. High-Level Layers (Mental Model)

1. **Vision & Governance Layer** (docs/ + future `/src/core`)
   - Deterministic authorization, receipts, grades, policies.
   - LLMs suggest; policies decide. No LLM in enforcement.
   - See: AGENTS.md (non-negotiables), docs/SPEC_MEMORY_GRADING.md, docs/ARCHITECTURE.md, docs/CORE_PATROL.md

2. **Desktop Surface (Primary target)**
   - Tauri v2 + React frontend rendered in transparent frameless always-on-top windows.
   - Fixed "envelope" windows (never resize for Linux WebKitGTK ghosting avoidance).
   - Native GTK input shapes (`gdk_window_input_shape_combine_region`) for per-pixel-ish click-through on transparent areas.
   - See: src-tauri/src/lib.rs (the heart), components/BorderDock.tsx (orchestrator)

3. **Browser Surface (Secondary / preview)**
   - MV3 Chrome extension injecting similar buddies into web pages.
   - Shares profile types conceptually but duplicates logic today.

4. **Gateway / Comms (Integration point)**
   - WebSocket bridge for real LLM sessions (Hermes buddy).
   - Mock server in scripts/ for dev.
   - Protocol in src/gatewayProtocol.ts

5. **Presentation & Characters**
   - Manifest-driven personalities + inline SVG renderers (head vs full-body states).
   - Dock vs per-buddy window modes.

6. **Cross-cutting**
   - Self-healing for fragile desktop state (pointer events, hitboxes, drags).
   - Persistence via localStorage (settings, placements).
   - Shortcuts, multi-monitor awareness, HiDPI scale handling.

---

## 2. Directory Structure (Clean View)

```
Border Agents/                                    # Parent folder (no .git — do not init here)
├── Border Agents.code-workspace                  # Open this in Cursor/VS Code → correct git root
└── Border-Agents/                                # **The git repo** (cd here for all commands)
    ├── AGENTS.md                                 # **Non-negotiable rules for all work** (read first)
    ├── README.md                                 # Product story (Buddies + governance underneath)
    ├── package.json                              # Scripts: dev, desktop:dev, gateway:dev, build, clean
    ├── tsconfig.json                             # Includes src/, components/, characters/
    ├── vite.config.ts                            # Strict port 1420 on 127.0.0.1
    ├── index.html                                # Vite entry (mounts BorderDock)
    │
    ├── docs/                                     # Vision, specs, no code yet
    │   ├── ARCHITECTURE.md
    │   ├── CORE_PATROL.md
    │   ├── ROADMAP.md
    │   ├── SPEC_MEMORY_GRADING.md                # The v0.1 wedge: packets, policies, grades, frames, receipts
    │   ├── UX.md
    │   ├── REPO_MAP.md                           # This file (navigation aid)
    │   └── ROUNDTABLE_tauri_linux_overlay_challenge.md  # Linux overlay deep-dive (input shapes win)
    │
    ├── src/                                      # Shared TS logic (non-UI)
    │   ├── main.tsx                              # React root → <BorderDock />
    │   ├── buddyProfiles.ts                      # Core profiles + settings types (memoryMode etc.)
    │   ├── useBuddyGateway.ts                    # React hook wrapping client
    │   ├── gatewayClient.ts                      # WS client (hello, chat, reconnect)
    │   ├── gatewayProtocol.ts                    # Message types + parser (versioned)
    │   ├── gatewaySettings.ts
    │   ├── dockSettings.ts                       # collapsed, renderMode (head/bubble/both)
    │   ├── dockSelfHeal.ts                       # Constants + report factory for recovery
    │   └── styles.css                            # Global base (damage hack rgba(0,0,0,0.005), pointer-events:none)
    │
    ├── components/                               # React UI (heavy)
    │   ├── BorderDock.tsx                        # **~2290 LOC monster**: ALL state, drag logic, hitbox sync, per-buddy vs unified, SVGs, settings panels, self-heal, gateway wiring. The runtime heart of current UI.
    │   └── BorderDock.css                        # All visual + animation + states (.agent-hotspot, .speech-bubble, dock-chrome, idle, click-through, etc.)
    │
    ├── characters/                               # Personality + visual source of truth
    │   ├── README.md, README-Characters.md, global-idle-animations.md
    │   ├── hermes/ (manifest + png/svg head/full)
    │   ├── crab/ (manifest; "Claw")
    │   ├── owl/ (manifest; "Veritas")
    │   └── (fox/Nexus defined inline in BorderDock + profiles)
    │
    ├── src-tauri/                                # Rust native layer (platform truth)
    │   ├── src/
    │   │   ├── main.rs (thin: calls border_agents_lib::run())
    │   │   └── lib.rs (ALL logic: ~685 LOC)
    │   │       - Monitor collection + selection
    │   │       - Fixed envelope calc for buddies (384x392)
    │   │       - configure_border_dock / configure_buddy_window
    │   │       - set_input_hitboxes (GTK input_shape + HiDPI scale)
    │   │       - snap_buddy_window, nearest_dock_zone
    │   │       - create_buddy_windows (legacy per-buddy mode via BORDER_BUDDIES env)
    │   │       - reset_dock_input, etc.
    │   ├── tauri.conf.json                       # border-dock window (1x1 initial), transparent, alwaysOnTop etc.
    │   ├── Cargo.toml                            # tauri 2.9, gtk 0.18 + cairo-rs on linux only
    │   ├── capabilities/default.json
    │   └── icons/
    │
    ├── scripts/                                  # Dev ergonomics (used by VSCode tasks + npm)
    │   ├── bb-start.sh, bb-stop.sh, bb-clean.sh, bb-gateway.sh
    │   └── gateway-dev.mjs                       # Minimal WS mock server (port 17387)
    │
    ├── extensions/browser/                       # Parallel browser surface (MV3)
    │   ├── manifest.json
    │   ├── background.js, content.js
    │   ├── profiles.js, hermes.js (own SVG renderer + drag logic)
    │   ├── buddies.css, popup.html + popup.js
    │   └── (duplicates profile/settings/placement concerns)
    │
    ├── assets/                                   # Marketing/screenshots
    ├── .vscode/tasks.json                        # "BB start", "BB gateway", "BB stop", "BB clean cache", "BB browser preview"
    │
    └── (build artifacts ignored: dist/, node_modules/, src-tauri/target/)
```

**Ignored in clean views**: node_modules, dist, src-tauri/target (thousands of files).

---

## 3. Key Entry Points & How to Run

**Open the repo in Cursor**: `Border-Agents/` folder, or `Border Agents.code-workspace` at the parent level (points at `Border-Agents/`). Do not use a git repo at the parent `Border Agents/` folder.

**From repo root** (per README):
```bash
cd "$HOME/TETRATHEDRAL/Border Agents/Border-Agents"
source "$HOME/.cargo/env"
npm run desktop:dev          # or: npm run dev (browser preview)
# VS Code: use "BB start" task
```

**Gateway (for Hermes chat)**:
```bash
npm run gateway:dev          # or "BB gateway" task (ws://127.0.0.1:17387/border-buddies)
```

**Stop / clean**:
- BB stop (or `npm run desktop:stop`)
- `npm run clean:cache`

**Env flags** (in lib.rs / setup):
- `BORDER_BUDDIES=hermes,crab,...` — select which buddies get per-buddy windows
- `BORDER_BUDDIES_LEGACY_WINDOWS=1` — old per-buddy window creation path (hides unified dock)

**Browser preview**: `npm run dev` then open http://127.0.0.1:1420 (adds ?multiMonitor=true etc.)

---

## 4. Current Implementation Status (vs Roadmap & AGENTS)

**Implemented (strong)**:
- Desktop overlay engine (unified dock or per-buddy fixed envelopes).
- Full Linux input shaping + damage hack + HiDPI + scale math + self-heal (routine + panic).
- Dragging, tucking, snapping to edges, free floating, double-click tuck.
- Multiple buddies (Hermes primary with gateway, Claw, Veritas, Nexus placeholder).
- Inline SVGs for head (tucked) / body (free) states per character.
- Per-buddy settings (provider, memoryMode: off/reference_only/purpose_graded, allowAction, allowExternalShare).
- Gateway WS protocol + React hook + mock server (chat + status + placement → bubble).
- Browser extension skeleton (own rendering + drag).
- Shortcuts (pass-through, hide, recover), idle fade, click-through modes.
- Persistence (placements, settings, dock) via localStorage.
- Multi-monitor support (partial).

**Partially / Stubbed**:
- Memory modes exist in types/UI only. No actual grading or SafeContextFrame.
- "Memory graded!" messages are static/hardcoded.
- Gateway is echo + canned; no real provider routing or provenance.

**Not started (per AGENTS v0.1 scope + roadmap)**:
- MemoryPacket schema + derivation
- PurposePolicy resolution
- MemoryGrader (deterministic)
- SafeContextFrame + PromptRenderer
- GradeReceipts / ledger
- The "same chunks, different purposes → different frames" demo
- Tests for the 6+ grading invariants
- Trust Workbench (Nexus/Veritas panels beyond chat bubbles)
- Real artifact lifecycle, action borders, etc.

**Gaps vs AGENTS laws**:
- "Every governance rule needs tests" — none visible for core.
- "Authorization must be graded" + "Every grade must produce a receipt" — UI chrome only.
- "Do not expand into a general agent framework until the memory grading primitive is proven."

**Note on current branch state (updated 2026-06-07)**: The Hermes clickability/gateway issue is resolved enough to commit as a working UX concept. The next project gate is the deterministic memory grading primitive and its tests.

**Roadmap alignment** (docs/ROADMAP.md):
- v0.1 (grading proof) → mostly docs
- v0.2 (Trust Workbench) → partial UI in bubbles/settings
- v0.5 (Core Patrol workspace) → the current BorderDock is ahead here
- Later phases untouched

---

## 5. Architectural Hotspots & Data Flow (Current)

**Desktop path**:
Tauri app start → lib.rs `run()` → setup creates border-dock (or legacy buddies) → frontend loads BorderDock.tsx
- React state owns placements, settings, active, drag, messages, gateway.
- useDockHitboxRegistry + useBuddyHitbox → rAF batched → `invoke("set_input_hitboxes")`
- Buddies render <BuddyHotspot> → head button + conditional <SpeechBubble>
- SpeechBubble wires gateway chat + local settings edits.
- On layout change (bubble open, settings, drag end) → report hitboxes → Rust updates GTK region.
- Native drag (data-tauri-drag-region or startDragging) + onMoved listeners for snap in per-buddy mode.
- Self-heal interval + visibility + panic shortcut tries to restore pointer/hitboxes/expanded state.

**Gateway path**:
useBuddyGateway → BuddyGatewayClient (WS) → protocol messages (hello on open, chat, replies become bubbles via onBubble callback).

**Character data**:
- manifests for hermes/crab/owl (partial fields)
- BUDDY_PROFILES in buddyProfiles.ts (source of truth for most)
- Hardcoded buddies[] array + inline SVG components in BorderDock.tsx (Fox/Nexus too)
- Extension has its own hermes.js + profiles.js copy

**Rust owns**:
- All window creation, positioning, always-on-top, input shapes, monitor math, snapping, envelope sizing.
- Commands exposed via invoke.

**JS owns**:
- All visual state, animation classes, user interaction, persistence, gateway, rendering.

**Duplication**:
- Profile/settings shapes in TS + JS extension.
- Drag/snap/tuck math duplicated (JS unified dock vs extension vs Rust snap).
- SVG renderers duplicated (TSX vs extension hermes.js).

---

## 6. How to Make Progress Safely

1. **Always start here**: Re-read [AGENTS.md](../AGENTS.md) (stance, laws, v0.1 scope, testing rules, PR questions).
2. **Translation rule**: Vision (border/patrol) → Arch (boundaries/frames/policies) → API (plain names: MemoryGrader) → Tests (concrete only).
3. **First real work recommended**: Implement the governance core in isolation (`src/core/memory-grading.ts` or similar).
   - Types for MemoryPacket, PurposePolicy, Grade, SafeContextFrame, GradeReceipt.
   - Pure `gradeMemory(chunks, purpose, policy?)` function + receipt derivation.
   - Hardcoded example policies for the 4 demo purposes.
   - Unit tests covering the required cases (expired, may_use_for_action, strict mode, etc.).
4. Wire a **mock vector result demo** that exercises it (can live in a dev panel or CLI for now).
5. Only then: surface grades into Nexus buddy / Trust Workbench UI.
6. Small reviewable commits. Every change must answer the 4 PR questions in AGENTS.md.

**Useful commands for iteration**:
- `npm run build` (typecheck + vite)
- Desktop dev with gateway running
- Browser preview for fast UI iteration (hitboxes ignored, uses CSS pointer-events)

---

## 7. Open Questions & Watchpoints (for contributors)

- Which surface owns "the truth" for a buddy's visual + hitbox rects during drag (React getBoundingClientRect vs Rust)?
- How will real provenance/memory packets flow into buddies (gateway messages? local context store?)?
- When adding custom agents (v1.0), manifests must be the contract — no bypass.
- Linux-only deep integrations (GTK) — plan graceful fallback or compile-time guards for macOS/Windows.
- Scale of buddies: current 4 hardcoded + dock chrome; future needs roster registry + manifest loader.

**Update this map** when layers are added (e.g. after `/src/core` lands, after real grading demo).

---

*Generated from full codebase walk (2026 context). Keep this file accurate as the single source of "where everything lives and why".*

---

## 8. Feedback: Robustness & Scalability Assessment

### What is Already Strong (Foundation for Scale)
- **Platform correctness for the "impossible" UX**: The fixed-envelope + `set_input_hitboxes` via GTK `input_shape_combine_region` (with HiDPI scale, rAF batching, drag guards, self-heal) directly solves the ghosting + dead-zone problems documented in the ROUNDTABLE file. This is production-grade systems work for Linux overlays. `rgba(0,0,0,0.005)` damage hack + `will-change` / `translateZ(0)` / `pointer-events` discipline shows deep understanding.
- **Self-healing & recoverability**: Routine + panic heal, stuck-drag detection, visibility listeners, explicit "Heal" button + shortcut. Desktop overlays are inherently flaky; this makes it usable.
- **Clear product vs governance split in vision**: README + AGENTS + CORE_PATROL correctly position buddies as the delightful surface, governance as the non-negotiable invisible contract. "Friendly miniature agents" on top of deterministic receipts is a powerful differentiator.
- **Native boundary respected**: Rust owns window lifecycle, positioning, input regions, monitor math. JS owns visuals, interaction, state. Commands are narrow (good).
- **Partial extensibility hooks**: Env-driven buddy selection, manifest files started, protocol versioned, renderMode cycling, memoryMode as first-class setting (even if not wired).
- **Dev UX**: Excellent scripts + .vscode/tasks for start/gateway/stop/clean/preview. Fast browser preview iteration path.

### Critical Risks & Gaps (Must Address Before Scaling)
1. **Governance core is missing — the project's stated reason for existence** (per AGENTS.md "Do not expand... until the memory grading primitive is proven", v0.1 scope, non-negotiable laws 1-6, required demo + 6+ tests). 
   - Current "purpose_graded", "Memory graded!", Nexus role are theater. This makes all trust claims in the UI non-credible.
   - Scalability consequence: You cannot add real agents, custom policies, artifact receipts, or multi-LLM provenance without it. The whole "Border" metaphor collapses.

2. **Monolithic UI component** (`components/BorderDock.tsx` ~2290 LOC).
   - Owns: global dock state, per-buddy placements, 4 drag/tuck/snap implementations (unified vs per-buddy vs native), hitbox measurement + reporting, gateway integration, settings forms, 4+ SVG renderers, chrome controls, self-heal UI, idle timers, shortcuts, persistence effects.
   - Consequence: Extremely hard to add a 5th buddy type, a new panel (real Nexus Trust Workbench), or extract browser-shared logic. Testing surface is the whole component. Refactors will be risky.

3. **Duplication tax across surfaces** (desktop React vs browser extension).
   - Profiles, settings normalization, drag/snap math, SVG rendering, placement persistence live in 2-3 copies.
   - When you finally implement grading + receipts, you will have to port the logic (or the results) everywhere or accept divergence. Extension will bit-rot.

4. **No durable, auditable ledger**.
   - localStorage for settings/placements only. GradeReceipts (the "machine-readable evidence" required by SPEC) have nowhere to live. No append-only log, no hash chain, no export.
   - Robustness failure: "every override must produce a receipt" is impossible today. Audit, compliance, or "what did the agent actually see?" questions cannot be answered.

5. **Character & agent system half-implemented**.
   - Manifests exist for 3 but are incomplete vs the fields used in code.
   - SVGs and the `buddies` array + `BUDDY_PROFILES` are the real source of truth and are scattered/hardcoded.
   - Idle animations doc exists but is not wired to the React components (static SVGs only).
   - Consequence for scale: v1.0 "Custom Border Agents" (manifest-defined, signed, policy-scoped) will be a big rewrite instead of an extension.

6. **Gateway & integration surface too thin for real use**.
   - Good for canned demo. No notion of purpose, no packet/provenance attachment, no auth or origin, simple echo replies.
   - To support "agent_action" purpose or real retrieval grading, the wire format + client must evolve in lockstep with the core grader.

7. **Zero automated tests** (especially the governance tests mandated by AGENTS and SPEC).
   - Desktop interaction is covered only by manual use + heuristic self-heal.
   - Risk: regressions in hitbox math, snap zones, or (future) grading rules will only be found in prod-like Linux desktops.

8. **Linux-centrism + limited graceful degradation**.
   - Deep GTK/cairo usage is correct for the target, but macOS/Windows input/overlay models differ (Tauri `setIgnoreCursorEvents` behaves differently). Current code has some try/catch + browserPreview paths, but not a clean abstraction.
   - HiDPI math lives in both Rust and JS (useBuddyHitbox) — easy to drift.

9. **Rust side is one big lib.rs**.
   - Geometry, window builders, command handlers, legacy paths, monitor selection all mixed. Hard to test in isolation, easy for bounds math bugs to hide.

### Prioritized Recommendations (Aligned to AGENTS + Roadmap)
**P0 — Unblock the project's core promise (do this before new UI features)**:
- Create `src/core/` (or `src/governance/`):
  - `types.ts`: MemoryPacket, PurposePolicy, Grade, SafeContextFrame, GradeReceipt (exact shapes from SPEC_MEMORY_GRADING.md + derivation trail).
  - `grader.ts`: pure `function grade(chunks: RetrievedChunk[], purpose: string, policy?: PurposePolicy): { frame: SafeContextFrame; receipts: GradeReceipt[] }`
  - Built-in policies for the 4 demo purposes (`summarize_history`, `answer_current_policy`, `agent_action`, `external_share`).
  - `receipts.ts` helpers.
- Add **vitest** (or jest) + table-driven tests covering every required case in SPEC + AGENTS ("expired chunks...", "chunks without may_use_for_action...", "strict mode excludes...", "custom purposes cannot widen...").
- Add a tiny **mock demo harness** (new route or dev-only panel or even a node script) that loads same vector results, grades under different purposes, and prints frames + receipts. This is the "first demo requirement".
- Only after green tests + demo: wire the frame into a Nexus buddy view.

**P1 — De-risk the UI for future growth**:
- Refactor BorderDock.tsx aggressively:
  - Extract: `useBuddyPlacements`, `useHitboxCoordinator`, `Buddy` (controller + view split), `SpeechBubble`, `DockChrome`, pure placement math functions (`getSnapEdge`, `calculate...` moved or shared).
  - Introduce a `BuddySurface` interface or context so the same buddy logic can drive desktop, future "watch" surface, or extension.
  - Target: main dock file < 400 LOC.
- Make character system manifest-complete:
  - Load all from `characters/*/manifest.json` + a registry that maps id → React component (or better: data-driven rendering + CSS/animation classes).
  - Implement baseline idle bob/blink/glance from global-idle-animations.md (use framer-motion or CSS + requestAnimationFrame, respect drag state).

**P2 — Eliminate duplication & prepare for multiple surfaces + custom agents**:
- Extract shared types + pure utils into `src/shared/` (or publish internally). Extension can consume via build or as a thin copy that is diff-checked.
- Define a **BuddyEngine** or state machine that owns one buddy's lifecycle (tucked/free, bubble, settings, gateway) independent of rendering surface.
- For extension: either share more code (Vite lib build) or accept it stays demo-only until core stabilizes.

**P3 — Make receipts & memory real (durability + audit)**:
- In-memory `ReceiptLedger` + `FrameStore` in the core.
- Persist (Tauri fs plugin or simple JSON append log first) with content hashes.
- Surface a "Receipts" viewer (even minimal) in a buddy panel. This makes "inspectable" real.
- When gateway carries real retrievals, attach/grade packets there.

**P4 — Platform & operational robustness**:
- Add a small `platform.ts` abstraction: `supportsInputShapes()`, `applyHitboxes(boxes)`, `startNativeDrag()` etc. with Linux GTK path + fallbacks (global cursor tracking per Tauri issues, or full ignore for other OS).
- Pin WebKitGTK expectations in docs/bundle notes (as roundtable recommended).
- Structured logging for hitbox updates, state transitions, heal actions (helps debug across user machines).
- Error boundaries per buddy + top-level dock recovery that doesn't crash the whole overlay.

**P5 — Later scale (after v0.1 grading lands)**:
- Custom agent manifests v2: declare border, triggers, allowed actions, policy scope, receipt behavior (per CORE_PATROL + roadmap).
- Policy engine that can load user/team policies (with override receipts).
- Real provider adapters behind the gateway (or direct in core for some surfaces) that emit packets with provenance.
- Consider a small event bus (receipt emitted → any listening buddy can react) instead of direct callbacks.
- Split Rust further if more commands/geometry grow.
- Add property-based testing for snap/zone math and grader.

### Suggested Near-Term Milestones (Small, Reviewable)
1. `src/core/memory-grading.ts` + types + 6+ passing tests (the AGENTS minimum).
2. Mock demo page/panel showing 4 purposes → different frames from same chunks.
3. PR that wires one real grade into the Claw/Nexus message or a new "Grades" section in bubble (read-only at first).
4. Extract `src/shared/placement.ts` (pure math) + update both desktop and extension to use it.
5. Character loader + one idle animation wired for Hermes.

Every step should still answer the 4 questions from AGENTS.md collaboration workflow.

### Summary Verdict
The **overlay runtime is impressively robust** for a notoriously difficult desktop problem and already demonstrates scalable thinking in its hitbox/self-heal architecture. However, the **governance heart is absent**, the **UI is too centralized**, and **duplication + missing durability** will multiply pain as soon as you add real memory grading, more agents, or real users.

Follow the AGENTS "narrow wedge first" discipline: land the MemoryGrader + receipts + tests + demo *before* polishing more buddy chrome or adding Forge/Strategos. With the core in place, the existing high-quality desktop surface becomes a credible vehicle for the trust promises instead of just charming visuals.

The project has real potential to make "AI you can trust because you can see the borders" feel delightful. The map + this feedback should let you (and future contributors/AIs) make steady, lawful progress.

**Live update (2026-06-07)**: See `docs/FIX_LIST.md` for the resolved Hermes clickability/gateway milestone. A partial extraction of the buddy surface/panel/chat has begun (addressing some of the monolith risk called out above). The map remains a good high-level picture; keep it and the fix list in sync as the governance core lands.

Update this section in REPO_MAP.md as the architecture evolves.
