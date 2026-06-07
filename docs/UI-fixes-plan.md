# UI-fixes Branch Plan: Anchor & Center Buddy Popups (Settings Dialogs, Action Menus) to the Active Buddy Visual Area

**Branch:** `UI-fixes` (created at start of session)  
**Goal (user):** Make "buddy actions" / "Model & gateway settings" (and any other popups opened via speech bubble / panel menus) *always center in the active window*. Currently they render detached / outside the screen, especially in native unified border-dock mode with tucked buddies.

**Context from AGENTS.md / project stance (for this change):**
- This is a UI / presentation layer improvement on the existing desktop surface (v0.5 "Core Patrol" work per ROADMAP).
- It makes a trust/visibility border more usable: popups (which contain authority settings like `allowAction`, `memoryMode`, gateway config) must be inspectable and reachable *attached to their buddy*, not lost in the overlay coordinate space.
- Must preserve hitbox / input-shape reporting (already includes open dialog/menu rects via `measureHitboxes`).
- Keep changes reviewable/small. Answer the 4 PR questions on landing: What border does this make visible? (the "popup belongs to this buddy's active visual frame"), what trust decision inspectable? (settings stay with the agent that owns them), what receipt? (logs + hitbox updates), what tests? (manual sequences + pure util tests for positioning math).
- Do not expand scope into new governance or new buddies.

**Status at plan time:** Investigation complete (main thread reads + parallel explore subagent on positioning code). All analysis is read-only.

---

## 1. Root Cause Analysis (Confirmed by Code)

The popups live inside per-buddy `BuddySurface` (rendered by `BuddyHotspot` inside the large `border-dock` webview or small per-buddy webviews).

**Two main popups:**
- **Settings ("Model & gateway settings")**: `BuddySettingsDialog` → renders `<div className="buddy-settings-layer">` ( `position:fixed; inset:0` ) containing `<div className="buddy-dialog__card" role="dialog">` with hard CSS:
  ```css
  .buddy-dialog__card { position:fixed; top:50%; left:50%; transform:translate(-50%,-50%); ... }
  ```
  (buddy-surface.css:382-403). No anchor. Triggered from `BuddyPanel` model pill or action menu "settings".
- **Action menus** (⋯ "buddy actions"): `BuddyActionMenu` → renders `<div className="buddy-menu-layer" style={{top,left}}>` ( `position:fixed; z-index high` ) with JS-computed position.
  - Uses `anchor.getBoundingClientRect()` (menu button) + optional `boundaryRef` (usually `panelRef`).
  - Computes preferred (right-of-anchor or right-of-boundary), flips if overflow, then clamps with `window.innerWidth/Height` + 12px margin.
  - (BuddyActionMenu.tsx:40-87, 121-128).

**Why "detached and outside the screen":**
- In **nativeUnifiedDock** (primary target, `getCurrentWebviewWindow().label === "border-dock"`): One large transparent Tauri window whose client rect is the union of selected monitor(s) (Rust `configure_border_dock` → `calculate_bounds` from `collect_monitors` + min/max, `src-tauri/src/lib.rs:622-645`). CSS clusters/hotspots are `position:absolute` inside a `dock-stage {inset:0}` using edge classes + `--agent-slot` / `--agent-x/y` (BorderDock.tsx:1930+, BorderDock.css).
  - `getBoundingClientRect()` and `window.inner*` are in the *webview's document coordinate system* (often ~monitor size or virtual desktop union).
  - Hard `top:50% left:50%` of that large rect centers the (wide) settings card in the middle of the monitor(s) — far from a tucked buddy head on the physical right/left/top/bottom edge.
  - Menu calculations using the same large `inner*` + an edge-near `anchorRect` can produce `top/left` that place the menu in "empty" transparent areas or push it off the logical edges that map to physical screen.
- In **per-buddy / legacy windows** (small fixed 384x392 envelopes from `calculate_buddy_window_bounds`, lib.rs:654-686): The webview viewport *is* tiny. Fixed 50% + a dialog wider than ~384px causes clipping or the card appearing "detached" because the window itself only covers the head envelope.
- **panelShift** (BuddySurface:342-384) and `rectToHitbox` (81-97, which caps to `window.inner*`) + unionHitboxes already show the team is fighting viewport-vs-small-visual problems for the *panel itself*. Popups were never given the same treatment.
- No propagation of "this buddy's active visual rect" (head + open panel + bubble union) down to the popup components for anchoring.
- `layout` / `MonitorFrame[]` (fetched via Rust, stored in BorderDock state) is used for dock creation + snapping but **not passed to surfaces or used for popup clamping**.
- Browser preview works "better" because the viewport matches the actual browser window the user sees.

Additional measurement hardening already exists (double-rAF, 600ms re-report while surface visible, `headForceKey`, `onLayoutChange`, `forceLayoutRefresh` on open) — popups participate in hitboxes (`measureHitboxes` includes `dialogBox`/`menuBox` when open, lines 301-312) so input shapes cover them.

Result: Popups feel like they belong to the global stage/webview instead of "this buddy's active window/surface".

---

## 2. Definition of Success ("centres in the active window")

- **"Active window"** = the on-screen visual footprint of the triggering buddy in the current mode:
  - When the surface is interactive/full (panel visible): primarily the `panelRef` bounds (or union of cluster + panel + bubble via existing `unionHitboxes` logic).
  - When compact / docked + active (bubble or minimal): the `clusterRef` / head area + any visible bubble.
  - The popup should feel attached to *that buddy's current location on the physical desktop*, not the abstract center of the dock webview or a per-buddy tiny window.
- **Centering behavior**:
  - Settings dialog (larger, modal-ish): Prefer to center the card roughly over or immediately adjacent to the active visual rect (e.g. horizontally centered on the panel's center, vertically a bit above or to the side so it doesn't completely cover the panel). Fall back to a sensible on-screen position if the buddy is near an edge.
  - Action menus (small): Keep/improve the current "near the trigger button" logic, but bias the position using the panel/cluster bounds as the strong "inside" boundary (prefer the direction that keeps the menu overlapping or right next to the panel visual). Current code already passes `boundaryRef={panelRef}` — make it more authoritative.
- **"Always" constraints**:
  - The entire popup card/menu must remain fully on-screen (within the current webview viewport with margin, and ideally within the physical monitor the buddy lives on).
  - Must work for tucked (border) + free + browser preview + per-buddy windows.
  - Must not break existing hitbox reporting (popups must still contribute accurate rects when open so the native layer can make them clickable).
  - Opening a popup must not require undocking the buddy.
  - Visual polish: nice offsets, shadows already exist; avoid jumping; support the existing panelShift transforms.
- Non-goals (for this PR): New Rust commands for popup windows, full physical-to-logical coordinate translation service, multi-buddy simultaneous open popups with cross-anchoring, CSS Anchor Positioning (future).

---

## 3. Approaches Considered + Trade-offs

**Approach A: Pure CSS containment / relative panels (minimal JS)**
- Make `.buddy-cluster` or a new wrapper `position:relative` with a large enough "virtual" size or use `overflow:visible`.
- Render the settings-layer and menu *as absolute children* of the panel-wrap or cluster (instead of fixed layers that escape).
- Use `left:50%; top:50%; transform:translate(...)` *relative to the buddy cluster/panel* + additional margin/offset classes.
- Pros: Very little new JS; popups "belong" in the same stacking/positioning context as the panel; hitboxes naturally include them.
- Cons: The cluster is deliberately tiny (`width:var(--agent-size, 3.45rem)`) for docked stability + input shapes. Making it large enough for a 30rem dialog would bloat the hitbox union or require dynamic sizing only while dialog open (risky for the "tiny stable border" invariant that was hard-won per FIX_LIST). Per-buddy small native windows would still clip. Fixed z-index / escape behavior harder. Panel absolute offsets + transforms complicate relative children.
- Verdict: **Not recommended** as primary; could be a supplement for the menu (small).

**Approach B: JS-driven anchored `position:fixed` (extend existing menu pattern) — RECOMMENDED**
- Keep the high-z fixed layers (good for escaping the small cluster and for z-order over everything in the webview).
- Add / extract a pure utility or hook `computeAnchoredPopupPosition(params: { anchorRect: DOMRect, referenceRect?: DOMRect (the "active visual"), popupWidth, popupHeight, preferred: 'center-over' | 'adjacent-right' | ..., margin, clampTo?: {minX,maxX,minY,maxY} or 'viewport' }) : {top, left, transform?}`.
- For dialog: In `BuddySurface` (or lift slightly), when `settingsOpen`, after the open effect + double-rAF, measure the current "activeRect" = union of clusterRef + panelRef (or head if !interactive), compute center point, call the util with `preferred: 'center-over'`, get style, apply inline style (or CSS var) to the card (or to a positioned wrapper inside the layer). Remove or override the hard `top:50% left:50% translate` in CSS when the computed style is present.
- For menu: Enhance the existing `useLayoutEffect` (already does gBCR + boundary + clamp). Pass a stronger `referenceRect` (the full visual union) and prefer positioning that keeps the menu overlapping or immediately adjacent to the referenceRect on the "inside" side of the screen edge.
- Clamping sources (in priority):
  1. The buddy's referenceRect expanded by a generous margin (so popup can sit "next to" without going far).
  2. The current `window.inner*` (webview viewport) minus 12-16px.
  3. (Nice-to-have) The active monitor rect from the `layout` already known in BorderDock (pass via context or props to the surface tree; use the monitor that contains the buddy's placement).
- Re-use / trigger the existing measurement pipeline: call `onLayoutChange` (or a new `forcePopupPositionUpdate`), which already does rAFs and notifies parent for hitbox re-report. After setting popup position, force another measureAndReportHitboxes so the native shape covers the new location.
- Pros: Directly solves the coordinate-space problem by anchoring to *what the user sees as the buddy*. Builds on battle-tested menu code + all the rAF/interval/hitbox hardening from the Hermes clickability work. Pure function easy to test. Works the same in all modes. Can be incremental (dialog first, then strengthen menu).
- Cons: More JS measurement (but already happening on open). Need to be careful with timing (dialog measures itself for hitboxes after it paints).
- Implementation size: Medium (new util + hook or effect in Surface + small changes to Dialog to accept/apply position style instead of pure CSS center; pass active rect or refs + a "getActiveVisualRect" helper).
- **This is the concrete recommended path.**

**Approach C: Lift popups to BorderDock / use a portal + global "active buddy rect" registry**
- Keep a map of "current visual rect per buddyId" updated from the hotspots' measurements.
- Render dialogs/menus at the BorderDock level (or a high portal) using the registered rect for the active buddy as anchor.
- Pros: Single source of "where is buddy X visually right now"; easier global constraints.
- Cons: Loses the nice per-Surface ownership (settings state is local to the buddy surface today). Adds cross-component coupling and more state lifting. Overkill for "per buddy popups". Hitbox reporting is already per-surface.
- Verdict: **Too heavy** for this fix. Only if we later need popups that can be dragged between buddies or global HUDs.

**Approach D: Native popup windows or separate Tauri dialogs**
- On "open settings", use a Tauri command to create a small native child window or webview dialog centered over the buddy's current native bounds (Rust already knows `BuddyWindowLayout` / monitor + physical x/y).
- Pros: True OS window, independent of the overlay viewport problems; could be always-on-top relative to the buddy.
- Cons: Heavy (new Rust command + window management + lifetime + focus + hit testing complications with the main overlay). Violates the lightweight "everything is a buddy surface inside the envelope" model. Breaks the current hitbox/input-shape contract. Cross-platform (GTK vs others) pain. Not "in the active window" of the web content.
- Verdict: **Reject** for v0 of this fix. Maybe future for "detached inspector" tools.

**Other considerations / rejected micro-approaches:**
- Just "clamp the existing 50% dialog to be near the buddy rect" — still requires JS measurement of the buddy rect + mutation of the style; no win over full anchored compute.
- Rely only on CSS `anchor-name` / `position-anchor` (very new, inconsistent support, not worth it).
- Ignore per-buddy window mode (it's legacy but still supported via env + code paths) — must handle both.

---

## 4. Concrete Recommended Implementation Strategy (Phased, Small Commits)

**Phase 0 — Prep on `UI-fixes` (this branch)**
- Add a few targeted diagnostics (behind `BB_LOG_POPUP_POS=1` or always in bbLog) that dump the cluster/panel/head rects + computed popup positions + `window.inner*` + current placement when settings or menu opens. (Easy to remove later.)
- Extract or create `src/shared/popupPosition.ts` (or inside components for now) with:
  ```ts
  export type PopupPosition = { top: number; left: number; transform?: string };
  export function computeAnchoredPopupPosition(args: {
    referenceRect: DOMRect;           // the "active buddy visual" (panel or cluster union)
    popupSize: { width: number; height: number };
    strategy: 'center-over' | 'menu-adjacent';
    margin?: number;
    viewport?: { width: number; height: number }; // fallback clamp
    // optional monitorSafe?: {left,top,right,bottom}
  }): PopupPosition;
  ```
  Pure, table-testable. Start with viewport clamp + reference-anchored logic. Include the existing menu clamp/flip as special case of "menu-adjacent".

**Phase 1 — Dialog anchoring (biggest user pain)**
- In `BuddySurface.tsx`:
  - Keep `settingsDialogRef`.
  - Add local state or derive: when `settingsOpen`, schedule a position computation (after the existing double-rAF in open effects + `forceLayoutRefresh`).
  - Measure `activeRect = union of (clusterRef + panelRef when present) or head area`. (Reuse `rectToHitbox` + `unionHitboxes` helpers.)
  - Call `computeAnchoredPopupPosition({ referenceRect: activeRect, popupSize: {w: card.offsetWidth or estimated 480, h: ...}, strategy: 'center-over' })`.
  - Store the result and pass `dialogPosition` (or style object) down to `BuddySettingsDialog`.
- In `BuddySettingsDialog.tsx`:
  - Accept optional `position?: {top, left, transform?}` or `dialogStyle`.
  - On the `.buddy-dialog__card`, if position provided: apply `style={{ position: 'fixed', top: pos.top, left: pos.left, transform: pos.transform || 'none' }}` (remove reliance on the 50% CSS for the positioned case).
  - Keep the layer as `position:fixed; inset:0` (or make the layer just a portal host with no inset if we don't need a full backdrop capture).
  - The outside-click handler already works on window.
- CSS: Make `.buddy-dialog__card` rules conditional or add a modifier `.buddy-dialog__card--anchored` that doesn't force 50%/translate (or just let the inline style win). Keep the visual card styles (width, padding, etc.).
- On open + after position set: call `onLayoutChange?.()` (already wired) so hitboxes re-measure the now-positioned card (via `settingsDialogRef`).

**Phase 2 — Strengthen menu + unify**
- In `BuddyActionMenu` (or the new hook): accept or compute a `referenceRect` (full active visual) in addition to `anchor` + `boundaryRef`.
- Bias the "inside" direction using the referenceRect (e.g. if buddy is on right edge, strongly prefer leftward placement of the menu so it stays over/near the panel).
- Use the same `compute...` util for consistency.
- Keep the high z and fixed layer.

**Phase 3 — Better clamping source (use existing layout)**
- In `BorderDock.tsx`, the `layout` (monitors + bounds + activeMonitorIds) is already available and updated on dock configure.
- Pass a lightweight "safeArea" or the active monitor rect (for the buddy's edge/placement) down through Hotspot → Surface (or via a narrow context `BuddyPositioningContext`).
- In the compute util, prefer clamping to the monitor rect (converted to webview client coords if possible) + margin. Fall back to `window.inner*`.
- This gives true "stay on the buddy's physical monitor" behavior.

**Phase 4 — Polish, compact mode, measurement freshness, cross-mode**
- In compact docked + active (surfaceInteractive but small visual): the referenceRect will be the head + bubble. Center the (larger) dialog "beside or slightly inset from" the head toward screen interior, with the same clamp.
- Ensure `forceLayoutRefresh` / the 600ms interval + open effects still fire a position + hitbox update.
- Handle rapid open/close, history growth (panel size changes), panelShift (the shift affects the visual rect — measure *after* shift is applied).
- Test / guard: browser preview (normal inner = what user sees), per-buddy small windows (clamp will keep dialog inside or near the small window's client area; may still look a bit detached but better than before; document as limitation or improve envelope size later).
- Update `measureHitboxes` comment if needed; the union logic for continuous regions may want to include the dialog when open (it already does for input).
- Add a unit test file for `computeAnchoredPopupPosition` (vitest) — pure math, easy wins for "tests prove it".

**Phase 5 — Validation & cleanup**
- Manual test matrix (document in PR or a temp test note):
  1. Native BB start, Hermes tucked right edge → click head → open full → click model pill or ⋯ → settings and menu appear centered on/attached to the panel (or head if compact), fully visible, not in the middle of the monitor.
  2. Same for left/top/bottom edges.
  3. Drag buddy free → open popups — still nicely attached to the free panel.
  4. Browser preview (npm run dev) — same behavior.
  5. With multiMonitor=true or real multi-monitor setup if available.
  6. Open settings, then recall to border / self-heal — popups close or re-anchor cleanly (existing close-on-!interactive logic helps).
  7. Hitboxes: with `BB_LOG_HITBOXES=1`, the dialog/menu rects are still reported and cover the visible popup.
- Remove temp logs.
- Update relevant docs lightly (REPO_MAP or a note in FIX_LIST) if this closes a usability gap.
- Small commit: util + tests; then dialog changes; then menu unification + layout clamp; then verification.

**Files expected to change (minimal surface):**
- `components/buddy/buddy-surface.css` (minor, for anchored card variant)
- `components/buddy/BuddySettingsDialog.tsx` (accept position prop, apply to card)
- `components/buddy/BuddyActionMenu.tsx` (consume improved util)
- `components/buddy/BuddySurface.tsx` (measure active visual rect on popup open, compute + pass position, wire updates)
- `components/BorderDock.tsx` (optionally pass layout/active monitor info; already has most state)
- New: `components/buddy/useAnchoredPopupPosition.ts` (or shared util) + test
- (Optional) light context if lifting monitor info feels cleaner than prop drilling.

**Risks & Mitigations**
- Timing of measurement (card not yet in DOM or not painted when we measure for position): Use the existing "double rAF + onLayoutChange" pattern + a `requestAnimationFrame` inside the position effect; measure the *reference* (panel/cluster) which is already there, estimate popup size from CSS min/max or a hidden probe, then apply. The card's own rect for hitboxes happens after render.
- Hitbox drift after we move the fixed element: The measurement already calls gBCR on the dialogRef after the fact; forcing a re-report after style update (via the onLayoutChange path) should keep native shapes in sync.
- Performance: Only compute on open + layout triggers (already rate-limited).
- Large dialogs near screen edge: The clamp + "center on reference but shift whole card" will naturally push the card so its box stays in-bounds (implement the util to return a position for the *top-left* such that the card is visually centered on the reference point while the box satisfies the clamp rect).
- Regression on browser or legacy: The same code path + viewport fallback makes it no worse.

**How this makes a border visible (per AGENTS collaboration rule):**
- The "popup belongs to its buddy's active visual frame on the current surface" border becomes explicit and reliable.
- Trust decision (changing a buddy's memoryMode / allowAction / gateway) is now clearly scoped to the buddy you clicked, not a floating global card.
- The change produces observable "receipts" in the form of updated hitbox reports + bb logs when popups move with their buddy.

---

## 5. Open Questions for User (use ask_user_question if needed before exit)

- Clarify "active window": Is it (a) strictly the panel bounds when the panel is open, (b) the head + panel union, or (c) center the popup on the physical monitor but keep it "logically attached" via arrow or proximity to the buddy head? Current plan assumes (b) with bias toward the panel.
- Preference on dialog behavior when buddy is compact/tucked on edge: "center over the head (may cover the head)" vs "place beside the head toward screen center, sized so it doesn't require the user to reach across the monitor"?
- Do we want the settings dialog to also drive a temporary larger hitbox / "capture" so the whole dialog is reliably clickable even if parts would have been outside the previous envelope? (Current code already tries via measure when open.)
- Any other "open windows" besides settings + the action menu (⋯) that should follow the same rule? (Future Forge panels, etc. — the util will help.)
- Priority: get dialog solid first, or treat menu + dialog together?

---

## 6. Exit Criteria for the Work on This Branch

- Popups open attached/centered on their buddy's current visual (panel or head area) in the main native border-dock case.
- No popups render "outside the screen" for the normal tucked + free flows.
- Existing clickability / hitbox / self-heal / drag behavior not regressed (verified by clean `BB stop` / `BB start` test sequences).
- At least the positioning util has unit coverage.
- Change is small, reviewable, answers the 4 questions.
- Plan doc (this) + any follow-up notes committed or referenced.

**Next after plan approval:** Implement in small commits on `UI-fixes`, using the same careful restart + log + measure discipline that landed the Hermes border clickability work.

---

*Plan saved to repo on UI-fixes branch for reference.*
