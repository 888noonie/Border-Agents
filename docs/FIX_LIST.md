# Border Agents — Active Fix List

**Purpose**: Lightweight tracker for open problems, fix attempts, and continuations on the current messy development branch (currently `first_connection`). This branch has collected multiple iterations of debugging and is intentionally kept open until core testable flows — especially the Hermes buddy + gateway chat in native unified "border" (dock) mode — can be exercised end-to-end by the developer.

**Rule**: Every entry should note:
- What border / trust / clickability / governance surface is affected.
- What the minimal reproducible test is.
- What previous attempts changed.
- The continuation needed before the branch can be cleaned up or merged.

See also: [REPO_MAP.md](./REPO_MAP.md) (especially the robustness assessment and P0 governance work, which is currently gated behind being able to test Hermes).

---

## Resolved Milestone

### Hermes buddy chat input not clickable in native unified border mode

**Status**: Resolved on 2026-06-07. Hermes can be interacted with in the desktop border surface, the local gateway can route real OpenAI-compatible provider replies, and the browser preview/extension path can connect to the same gateway.

**Summary**: In the unified native Linux border-dock window (the production target for "heads tucked on the edge"), click-through/input-shape behavior prevented reliable interaction with the Hermes chat composer. This blocked validating the gateway roundtrip and the broader UX concept.

**Previous attempt / context** (the "Catch failing in border mode" fix that was applied):

> `Catch` failing in border mode pointed to the safer fix: native Linux border mode should not be able to enter `Pass` right now.
>
> I changed components/BorderDock.tsx so unified native border mode treats `Pass` as unavailable, forces `clickThrough` back to `false` if it ever appears, and immediately runs the dock self-heal path instead of leaving Hermes stranded. The dock button now shows `Pass off` in border mode rather than letting you enter the broken state. The earlier `Set`/surface hitbox refresh fix is still in place, so settings/full chat/menu changes should keep Hermes clickable.
>
> Both `npm run build` and `cargo check --manifest-path src-tauri/Cargo.toml` passed.
>
> Please restart the dock fully with `BB stop` and `BB start`, then test this sequence:
> 1. Click Hermes head on the right edge.
> 2. Open full chat and `Set`.
> 3. Type `border check` and press `↑`.
>
> If Hermes is still not clickable after a clean restart, that means we’re down to a compositor/window-manager input-shape issue on your Linux session, and I’ll switch from app-state fixes to the native Tauri/GTK overlay layer next.

**Key safeguards currently present in code** (as of latest on this branch):
- `passThroughAvailable = !nativeUnifiedDock`
- `setClickThroughMode` early-returns + forces `false` + calls `performSelfHeal({ panic: true })` when native border mode tries to enable click-through.
- A useEffect that forces `setClickThroughMode(false)` if `clickThrough` ever becomes true while `nativeUnifiedDock`.
- On native startup: `recallBuddiesToBorder()` + explicit "startup test-ready" log with `clickThrough: false`.
- Self-heal explicitly disables pass-through on panic.
- `onActivate` (clicking head) does `setClickThroughMode(false)`.
- Partial extraction of chat UI into `components/buddy/BuddySurface.tsx` + `BuddyPanel.tsx` (with `measureHitboxes` that includes `panelBox` for the full composer/history + dialog + menu).

**Why it is still not resolved (deeper root cause)**:
The global "never enter Pass" guard + self-heal is in place.

The "Opus" gateway-gated input fix (removing the hard `disabled` on the textarea based on `gatewayOnline`, making send double as Connect, adding parked auto-send + auto-connect on become-interactive) fixed the symptom where Hermes felt permanently unclickable compared to other (non-gateway) buddies.

However, a second, more fundamental architectural decision was blocking a usable test *while the buddy stays on the border*:

In `BuddyHotspot` (the per-buddy wrapper):
- `showSurface = isFree || (renderMode !== "head" && active);`
- `surfaceInteractive = isFree;`   (strictly only when `placement.state === "free"`)

When the Hermes head is tucked on the native border (`state === "tucked"`) and you click it to activate/chat:
- `showSurface` becomes true (ambient bubble or compact surface appears).
- But `surfaceInteractive` stays false.
- In `BuddySurface.measureHitboxes`: if `!interactive` it explicitly returns *only* cluster + bubble and skips the panelBox / composer entirely (see the "Ambient (docked) mode" comment). `displayMode` is forced to "compact".
- Result: no composer textarea hitbox is ever sent to `set_input_hitboxes` for the border-dock window → even if the DOM element exists and is not `disabled={busy}`, it has no input_shape region → clicks do nothing (or go to desktop).

This design was intentional ("keeps the native border overlay tiny and stable" and "fixes the nothing clickable / crashing border-mode failure"), but it meant there was literally no supported path for a full "type a message" Hermes test while the buddy remained a head on the edge.

The previous "Opus" changes assumed the flow would involve undocking ("drag Hermes off the right edge") to reach `interactive === true`.

**Latest continuation change (this session)**:
Changed `surfaceInteractive = isFree || active;` (with detailed comment linking here). This allows the full composer + `unionHitboxes` logic + panel hitbox when the user activates chat on a tucked border buddy. Combined with the existing 600ms re-report interval (for open chat → Set → type sequence) and the double-rAF forces after showFullMode / settings changes, this should finally give a usable Hermes input directly on the border (desktop) without requiring an undock for the basic test.

The gateway auto-connect / parked message logic (from Opus) should now also be exercisable in the border-tucked + active state.

**Test sequence that must succeed** (repeat after every candidate fix + full clean restart):
1. Click Hermes head on the right edge.
2. Open full chat and `Set`.
3. Type `border check` and press `↑` (or Enter). The message should appear in history and (if gateway connected) round-trip.

A clean `BB stop` followed by `BB start` (or the equivalent VS Code task) is required before each test run. Partial HMR reloads have been observed to leave hitbox state inconsistent.

**Continuation / next actions** (what needs to happen before this item can be closed and the branch cleaned):
- Strengthen post-layout hitbox forcing: call `measureAndReportHitboxes` (or expose a `forceReport` from the surface) immediately after `setDisplayMode("full")`, after settings dialog closes/applies ("Set"), and on `focus` / `mousedown` of the composer textarea.
- Add a ResizeObserver or MutationObserver (or extra rAF + explicit `getBoundingClientRect` force) specifically around the panel/composer when it enters full mode.
- Add diagnostic logging (via `bbLog`) in native border mode: when reporting boxes for Hermes, log whether a `panelBox` was present, the rect values, and the total count sent. Mirror with logs in Rust `set_input_hitboxes` when the target window is the border-dock.
- Review the Rust `configure_border_dock` + `set_input_hitboxes` path for any special handling (or missing handling) of extended bubble/panel rects vs the fixed envelope.
- If the above app-state / JS measurement changes still do not make the textarea reliably clickable after clean restarts, follow the escalation in the previous attempt: stop trying to fix via React state + hitbox lists and move to the native Tauri/GTK overlay layer (different input shape strategy, window manager hints, or hybrid cursor event handling for the border-dock case).
- Once the input is clickable and the "border check" command can be sent + received, validate the full Hermes gateway flow (including any memoryMode / graded messages). Only then can the current accumulation of fixes on this branch be reviewed for cleanup / squashing / PR.

**Related files** (as of latest investigation):
- `components/BorderDock.tsx` (nativeUnifiedDock detection, setClickThroughMode, performSelfHeal, measureAndReportHitboxes, reportHitboxes, BuddyHotspot integration, onActivate)
- `components/buddy/BuddySurface.tsx` (measureHitboxes implementation returning cluster + bubble + panel + dialog + menu boxes, useLayoutEffect triggering onLayoutChange for displayMode/history/settingsOpen/etc.)
- `components/buddy/BuddyPanel.tsx` (the actual `<form className="buddy-panel__composer"><textarea ...>` and "Set"/settings pill)
- `components/buddy/useUiBubble.ts` + `BuddyUiBubble.tsx`
- `src-tauri/src/lib.rs` (configure_border_dock, set_input_hitboxes + region building, reset_dock_input, GTK input_shape_combine_region + scale math)
- `src/dockSelfHeal.ts`
- Scripts: `bb-stop.sh`, `bb-start.sh`, `bb-report.sh`

**Resolution**:
- Added full overlay input ownership controls (`Interact` / `Desktop`) so the user can deliberately take/release pointer ownership.
- Extracted the buddy panel/settings/menu surface into `components/buddy/`.
- Reworked hitbox reporting so active/full buddy panels, menus, and settings cards are measured and reported.
- Connected Hermes to the local gateway and an OpenAI-compatible provider adapter.
- Enabled browser preview and browser extension Hermes chat through the same gateway protocol.

**Remaining polish**:
- Reduce debug logging before release builds.
- Continue improving settings/menu placement and visual density.
- Build the deterministic memory grading primitive required by AGENTS.md before broadening into general agent behavior.

**Attempt 2026-06-06 (click-vs-drag + hitbox hardening)**:

Root cause found for "can't click my first border buddy at all": the head button's
`onPointerDown` (`startBuddyDrag`) began a drag on *every* press, so a plain tap
immediately popped Hermes off the border into a free-floating window and the
`click` that would have activated/opened the chat never fired. The press gesture
is now drag-armed but only *promoted* to a drag once the pointer travels past
`DRAG_ACTIVATION_THRESHOLD` (6px); below that it stays a click so `onActivate`
runs and the chat surface opens. `startBuddyDrag` no longer calls
`preventDefault`, and a pending-gesture cleanup ref prevents stale listeners.

Hardening applied for the follow-on "composer not clickable" symptom:
- `BuddyHotspot` now re-reports hitboxes on a 600ms interval while the chat
  surface is open (covers late paints the layout effects miss).
- `BuddySurface.measureHitboxes` adds a **union box** over head+bubble+panel in
  full mode, eliminating the dead gap between the head and the composer.
- `BuddyPanel` textarea now fires `onComposerInteract` on focus/pointerdown,
  forcing an immediate re-measure when the user reaches for the input.
- `BuddySurface` re-measures (double-rAF) on composer interaction.
- Rust `set_input_hitboxes` gained opt-in diagnostics (`BB_LOG_HITBOXES=1`)
  logging window label, applied/requested counts, scale, and the raw boxes.

`npm run build` and `cargo check` both pass. Test sequence to confirm after a
clean `BB stop` / `BB start`: tap Hermes head → it should open (not detach) →
open full chat → type `border check` → `↑`.

---


**Attempt 2026-06-06b (split docked vs undocked interaction model)**:

The inline-in-border composer kept failing/crashing, so per the owner's
direction the interaction model was split cleanly by dock state — a robust,
scalable base for future custom buddies:

- **Docked / tucked = ambient only.** `BuddySurface` now renders ONLY the speech
  bubble when `interactive === false`. No textarea, no settings, so the native
  border overlay's clickable region is just the head + small bubble.
- **Undocked / free = interactive.** The chat composer + settings + action menu
  only mount when the buddy has been dragged out (or popped out) into a free
  floating surface, where a large stable hitbox makes clicks reliable.
- `displayMode` is now *derived* from dock state (no inline compact/full toggle).
- New non-drag path: clicking the ambient bubble calls `popBuddyOut` in
  `BorderDock`, which undocks the buddy to a centered free interactive surface.
  Dragging the head past the 6px threshold does the same. The panel's collapse
  button (and a new "Dock to border" menu item) re-docks via `onManualTuck`.
- `BuddyHotspot` flags: `showSurface = isFree || (renderMode !== "head" && active)`,
  `surfaceInteractive = isFree`.

`npm run build` passes. Test: bubble shows on the border → click bubble (or drag
head out) → interactive panel appears free-floating → type `border check` → send
→ collapse/double-click to re-dock.

---

## Other / Lower Priority Items (add as discovered)


(Empty for now — add new entries above this line when new symptoms appear during the Hermes testing work.)

---

**Current status of this continuation (2026-06-06, end of day)**:
- User had to pause ("I have to give up... try to fix it so I can interact with the border buddies tmw"). Multiple clean starts, Force Pass (emergency clear), recalls, and Heal attempts were made. The emergency path and recalls are triggering (logs show the WARN and "recalled buddies to border"), but heads remain unclickable in native unified border-dock mode.
- All the previous mitigations are in (surfaceInteractive relaxation, headForceKey periodic + on-recall forcing, +4px padding on head rects, new JS "reporting head hitbox (border mode)" diagnostics, existing 600ms interval + rAF forces, Force Pass clear, no-Pass guard in normal operation, self-heal).
- Critical missing data from the final log: no `BB_LOG_HITBOXES=1` output (no `[rust ...] set_input_hitboxes window=border-dock ...` lines with actual scaled rects). The new head reporting logs were also not present in the pasted output, suggesting either the run used an older build or the head measurement path isn't firing the expected rects for the tucked Hermes head.
- The "border" we are trying to make visible/inspectable is the input-shape clickability region for the docked heads. Currently the region is either empty, wrong coords, or not taking effect for the head buttons in the border-dock window.

**Plan for tomorrow (first thing)**:
1. Clean `BB stop`.
2. `BB_LOG_HITBOXES=1 BB start` (critical — this makes Rust log every set_input_hitboxes for border-dock with the exact boxes, scale, and applied count).
3. Wait for initial recall + the new JS "reporting head hitbox (border mode)" lines.
4. Try click on visual head, Force Pass, Heal, recall.
5. Share the sections containing "reporting head hitbox", the rust set_input_hitboxes border-dock lines, and heartbeats around the clicks.
6. Based on the numbers (are head rects non-zero? on the right edge? does padding help? does Rust see applied > 0?), decide next: more padding, coordinate fix in JS measurement or Rust, force shape after window show, debug visual overlay for the reported regions, or deeper GTK input shape investigation.

This session made the failure mode much more debuggable and gave the user an emergency "Force Pass" escape hatch. Great progress on visibility even if full interaction is still blocked. The branch remains intentionally open until a reliable border-head click test exists.

**Recommended clean test flow to close the branch**:
1. Full restart: `BB stop`, then `BB start` (desktop) + separate `BB gateway` (or the dev echo server).
2. Hermes head tucked on right edge of the screen (native border-dock unified window).
3. Click the head → full interactive panel should open beside it on the border, gateway should auto-connect.
4. Click into the composer (should accept focus/typing immediately).
5. Type `border check`, press Enter.
6. You should see your message + status lines + Hermes echo reply.

When the above works end-to-end on the border without crashes or unclickable input, add a "Resolved" subsection here, update REPO_MAP.md with the success, commit the branch (with a message that covers what border this made inspectable, what trust/click decision is now reliable, what receipts the change produces if any, and the tests), then the messy first-connection branch can be closed successfully.

**Maintenance note**: When a fix allows the test sequence to pass reliably, move the entry to a "Resolved on this branch" section (or delete after the branch is cleaned), and update REPO_MAP.md. Keep entries factual and tied to the four PR questions from AGENTS.md.

Last updated: 2026-06-06 (after the surfaceInteractive relaxation for border-tucked active chat + clean BB stop).
