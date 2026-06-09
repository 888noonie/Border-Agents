# Desktop Overlay Postmortem & Rebuild Plan

**Date:** 9 June 2026
**Status:** Unified transparent-window approach declared dead. Rebuild required at the presence layer.
**Audience:** Any human or agent picking up the Border Buddies desktop work. Read this before attempting any fix in the current rendering stack — every "obvious" fix has already been tried and is documented below with its outcome.

---

## 1. The Goal

Bring the fluidity of the Browser Buddies experience to the desktop (and eventually every screen): animated buddy characters that live on a global top layer above all windows, are draggable and interactive (expanding menus, chat panels), click-through everywhere else, with an LLM ultimately controlling each character's on-screen presence and — under Core Patrol governance — acting on the web, the device, and external environments alongside the user.

## 2. The Environment (this is most of the story)

| Component | Value | Why it matters |
|---|---|---|
| OS / Desktop | Pop!_OS, COSMIC | Modern compositor, strict Wayland semantics |
| Display protocol | Native Wayland | Clients **cannot position their own toplevel windows** |
| GPU | NVIDIA RTX 4050 (hybrid with AMD iGPU) | Historically worst-case for WebKitGTK transparency artifacts |
| Webview | WebKitGTK **2.52.3** (Skia renderer) | All legacy artifact workarounds were **removed in 2.46** |
| App framework | Tauri 2 (GTK3 windows) | Webview rendering is WebKitGTK's, not ours |

## 3. The Symptoms

**Symptom A — Ghosting trails (unified window).** The current architecture renders all buddies inside one fullscreen transparent always-on-top window (`border-dock`). Dragging a buddy leaves a diagonal cascade of fully-retained stale frames (heads, settings cards) behind it. The *only* user action that clears them is resizing the window itself.

**Symptom B — Stacked windows (original per-buddy architecture).** The first build used one small window per buddy. On this machine they all spawned in a pile at the compositor's default position, ignoring every positioning call, making the mode unusable. This was the reason for the move to the unified window — which then produced Symptom A.

## 4. Root Causes (established, not speculative)

1. **WebKit's accelerated-compositing scene is unreachable from GTK.** `queue_draw` / `gdk_window.invalidate_rect` only invalidate the GTK/GDK layer. The stale buddy frames live in WebKit's internal compositing scene, which ignores GDK invalidation entirely. This is why every repaint-based fix did nothing.
2. **The legacy escape hatches are dead.** `WEBKIT_DISABLE_DMABUF_RENDERER` and `WEBKIT_DISABLE_COMPOSITING_MODE` were removed when WebKitGTK switched to Skia in 2.46. On 2.52 they are silently ignored. The codebase set the former believing it provided "transparent-window repaint stability"; it provided nothing.
3. **Native Wayland forbids client window placement.** `set_position` on a normal toplevel is a silent no-op by protocol design. The entire per-buddy positioning system (`calculate_buddy_window_bounds`, slots, snap zones) executed correctly and was discarded by the compositor. Symptom B was never a code bug and was never fixable with more iteration in the same architecture.

## 5. Attempt Log

Every attempt below was implemented, run, and observed on the target machine. **Do not re-attempt these.**

| # | Attempt | Layer | Outcome | What it proved |
|---|---|---|---|---|
| 1 | CSS compositing hints (`translateZ`, `will-change`, `translate3d`) | CSS | Failed (can worsen) | Artifact is below the CSS layer |
| 2 | `repaint_overlay_window` → `invalidate_rect(None, true)` on drag-end | Rust/GDK | Cleared ghost on *drop* only | GDK invalidation reaches the window surface but not during-drag accumulation |
| 3 | rAF-throttled `repaint_overlay_window` every drag frame (`pulseOverlayRepaint`) | Rust/GDK | **Failed** | GDK invalidation cannot clear WebKit's AC scene — confirmed root cause 1 |
| 4 | `WEBKIT_DISABLE_DMABUF_RENDERER=1` | Env | **No-op** | Removed in WebKitGTK 2.46 — confirmed root cause 2 |
| 5 | 1px resize-jiggle per repaint pulse (`jiggle_overlay_size` in `src-tauri/src/lib.rs`, alternating height base/base−1, ~90ms throttle, opt-out `BB_DISABLE_RESIZE_REPAINT=1`) | Rust/window | **Failed** | Even forced buffer reallocation — the mechanism behind "manual resize clears it" — does not clear trails when driven programmatically on COSMIC/Wayland/NVIDIA. The unified transparent webview is unrecoverable. |
| 6 | `GDK_BACKEND=x11` default (XWayland) + `BORDER_BUDDIES_LEGACY_WINDOWS=1` per-buddy mode | Env/arch | **Failed** (reported still broken) | Even the XWayland compatibility path does not rescue the current stack on this machine |
| 7 | Fixed window envelopes, "no resize operations" rule, input shapes, hidden unified dock in per-buddy mode, separate `dock-chrome` window | Various | Partial mitigations only | Good design elements worth keeping; none address root causes |

**Verification note on attempt 6:** the startup log line `GDK_BACKEND=x11 set: forcing XWayland...` (via `bb_events_log`) confirms the backend actually applied. If a future session revisits this, check that line first, and check whether the failure observed was Symptom A (unified mode running) or Symptom B (per-buddy mode running) — they have different meanings. `xprop` clicking a buddy window will hang on a native Wayland surface and return data on an XWayland one.

## 6. Verdict

**Stop building the presence layer on a transparent WebKitGTK surface.** Three independent root causes converge on the same conclusion: WebKitGTK transparent overlays on modern Wayland + NVIDIA are unsupported terrain, and every workaround tier (CSS, GDK, env vars, window tricks, backend switching) has been exhausted. The webview can keep powering *panels and chat UI in normal windows*; it cannot be the thing that floats on the desktop.

## 7. What to Build

### 7.1 The architecture: one soul, many bodies

Permanently separate the buddy's **brain** from its **body**:

- **Soul (agent runtime):** a headless daemon hosting N buddies — LLM loop, persona state, memory, tool belt. The existing Hermes gateway grows into this. The LLM controls its character by emitting *presence events as tool calls* (`move_to`, `express(emotion)`, `say(text)`).
- **Body (presence surface):** a dumb puppet that renders presence events and emits interaction events (clicked, dragged, dropped-on). One small surface per buddy. Never one big surface for all of them — per-buddy surfaces make ghosting structurally impossible (moving a surface is a compositor texture move; no vacated pixels exist to repaint).
- **Presence protocol:** a small typed event schema (position, pose, emotion, speech, attention ⟷ interaction events) over the existing WebSocket. **This protocol is the product.** Once it exists, the same soul possesses a browser-extension body, a desktop body, a phone body — and one day a kinetic one — by swapping renderers.

### 7.2 The desktop body: wlr-layer-shell, native renderer

The correct Wayland primitive for "pixel-locked, top-layer, any-screen artifacts" is **wlr-layer-shell** (the protocol used by waybar, notification daemons, lock screens; supported by COSMIC):

- Each buddy = one layer surface on the **overlay layer** of a chosen output.
- Position = anchor (e.g. top-left) + **pixel margins**. Exact placement, by protocol right — the "windows locked pixels apart" capability that normal toplevels are denied.
- Dragging = pointer delta → update margins (not native window moves).
- Click-through = per-pixel **input regions** (set the input region to the buddy's silhouette/menus; everything else passes through).
- Keyboard = `on_demand` interactivity for chat input.

**Renderer choice — the load-bearing decision:** do **not** put WebKitGTK inside the layer surfaces (it drags root cause 1 back in). Render buddies natively:

- **Recommended:** a small Rust renderer per surface — `smithay-client-toolkit` (or `gtk4-layer-shell` + GTK snapshot drawing) with **wgpu or Skia/femtovg** drawing sprite/skeletal animation. Buddies are character art + speech bubbles + menu cards: a sprite renderer's home turf. Animation data (sprite sheets or Rive/Lottie via `rive-rs`/`vello`) keeps designers in familiar tools.
- The rich UI (full chat panel, trust workbench, settings) stays in **normal Tauri windows** — summoned by the buddy, placed by the compositor, no transparency tricks needed. Webview where webviews work; native where they don't.

**Cross-platform mapping (same protocol, thin renderers):** macOS `NSPanel` at `.screenSaver` level; Windows `WS_EX_LAYERED | WS_EX_TOPMOST` (per-pixel hit-testing via `WM_NCHITTEST`); Android overlay permission (`TYPE_APPLICATION_OVERLAY`); browser = the existing extension.

### 7.3 Effectors (acting on the world)

Per-environment action modules, all exposed as tools to the soul:
- **Web:** the browser extension, and/or CDP/Playwright for full-page control.
- **Device:** computer-use pattern — screenshot + accessibility tree in, pointer/keyboard out.
- **External:** MCP servers for APIs, services, home automation.

### 7.4 Core Patrol (governance) — the chokepoint, not a feature

No effector accepts a command except from the governor. Every tool call passes through one mediation point providing:
- **Capability grants:** scoped, time-boxed, default-deny ("Hermes may read this tab for 10 minutes").
- **Receipts:** signed, append-only audit of every action (`receiptLedger.ts` / `liveGovernance.ts` on `governance_core` are the seed).
- **Approval tiers:** observe / act-with-notification / act-with-approval / never — plus a global kill switch (revoke all grants, freeze all effectors).

Building this *before* effectors get powerful is what makes "authorised tasks" credible. Voice (realtime speech in/out) and other channels attach later as new event types on the same bus — touching neither renderer nor governance.

## 8. Build Order

1. **Presence protocol v0** — typed event schema between gateway and any body; refactor the browser-extension buddies to speak it (proves the protocol against the body that already works).
2. **Layer-shell spike** — one native Rust layer surface on COSMIC: draw a static buddy sprite, drag it via margin updates, input region for click-through, second monitor placement. This de-risks the whole desktop plan in a few hundred lines. *(Fallback if COSMIC layer-shell misbehaves: same renderer drawing into per-buddy XWayland windows — but spike layer-shell first.)*
3. **Animated body** — sprite/Rive animation, speech bubble, expanding menu card in the native surface; summon a normal Tauri window for the full chat panel.
4. **Wire the soul** — gateway drives the new body over the presence protocol; LLM emits presence events as tool calls.
5. **Governance vertical slice** — one real effector action end-to-end: buddy opens a URL with a grant, a receipt, and an approval prompt.
6. **Then:** more effectors, voice channel, additional platform bodies.

## 9. What to Keep from the Current Codebase

- Slot/edge/snap geometry (`calculate_buddy_window_bounds`, `DOCK_ZONES`) — re-targeted at layer-surface margins.
- Fixed-envelope + input-silhouette concept — becomes layer-shell input regions.
- Gateway, `useBuddyGateway`, governance modules (`receiptLedger.ts`, `liveGovernance.ts`), buddy persona/UI components (for the panel windows and the browser body).
- Diagnostics pattern (`bb_events_log`).

**Retire:** the unified `border-dock` transparent window; `jiggle_overlay_size`; the dead `WEBKIT_DISABLE_DMABUF_RENDERER` block; repaint pulsing in `DesktopBorderDock.tsx`.
