# The Border Agents Desktop Overlay Challenge

## Objective
To create small, unobtrusive "buddies" (animated agents) that live seamlessly on the edges of the desktop screen. They must be able to:
1. Exist as frameless, fully transparent overlays showing only the character's head.
2. Dynamically expand their visual footprint (e.g., pop out a speech bubble) without visual artifacts.
3. Be draggable (detached from the edge into a "free" state).
4. Allow click-through on the transparent areas of their bounding box so they don't block the screen below.

## Stack
- **Framework:** Tauri (v2)
- **Frontend:** React + Vite
- **OS:** Linux (specifically leveraging WebKitGTK compositor behavior)

## The Core Problem
When a buddy is "tucked" at the edge of the screen, its visual footprint is small (e.g., 116x116). When clicked, a speech bubble appears, requiring a larger visual area (e.g., 520x150). 

There are two primary approaches to handling this size change, both of which have severe drawbacks on Linux/Tauri:

### Approach A: Dynamic Window Resizing
- **How it works:** The Tauri `WebviewWindow` is kept exactly the size of the visible content. When the speech bubble appears, `window.set_size()` is called to expand the bounds.
- **The Issue (Ghosting):** On Linux, when a transparent WebKitGTK window is resized, the desktop compositor fails to clear the intermediate frames properly. This results in severe "ghosting" or "trailing" visual artifacts. The previous frame's pixels remain on the screen until the window is fully redrawn or moved.

### Approach B: Fixed "Envelope" Size
- **How it works:** The Tauri `WebviewWindow` is created at the maximum possible size needed (e.g., 540x200), and is never resized. The buddy head and speech bubble move within this large, transparent window using CSS.
- **The Issue (Input Blocking):** Even if the background is 100% transparent (`rgba(0,0,0,0)`), the OS still registers the entire 540x200 window rectangle for mouse events. This creates a "dead zone" on the user's screen where they cannot click the applications underneath the invisible parts of the buddy window. 
- *Note:* Tauri's `set_ignore_cursor_events(true)` allows click-through, but it applies to the *entire window*. If enabled, the user can no longer click the buddy itself to drag it or open the speech bubble. We need *partial* click-through (click-through on transparent pixels only), which Tauri/WebKitGTK does not natively support on Linux.

## Secondary Issue: Damage Region Tracking
Linux compositors often optimize by entirely ignoring updates to fully transparent windows. To force the compositor to redraw the window (and thus animate the buddies), we currently use a CSS hack: setting the `body` background to `rgba(0, 0, 0, 0.005)`. This is nearly invisible to the human eye but forces the compositor to track the "damage region." However, this hack does not solve the ghosting on resize (Approach A) or the input blocking (Approach B).

## The Question for the Roundtable
How can we achieve the illusion of small, shape-shifting, interactive overlay buddies on Linux using Tauri without triggering compositor ghosting and without creating invisible dead zones that block mouse clicks to underlying applications?

**Specific Angles to Consider:**
1. **GTK/WebKitGTK Specifics:** Are there specific X11/Wayland window hints or GTK APIs that can be invoked via Tauri Rust plugins to enable "click-through on transparent pixels only" (Input Shapes)?
2. **Window Lifecycle Hacks:** Is there a way to mask the resize operation? (e.g., creating a *new* window at the larger size, transferring state, and destroying the old one? Or hiding, resizing, then showing—which we tried, but it disrupted the UX).
3. **Electron Comparison:** The project `SPACE_AGENT` achieved smooth behavior, but it used Electron. Electron uses Chromium, which handles transparency and input shapes differently than WebKitGTK. Can Tauri emulate Chromium's input shaping on Linux?


Opus 4.8 : 
This is a meaty systems-level problem. Let me ground my answer in the current state of the relevant APIs and known issues before I synthesize a recommendation.

I dug into the current Tauri/WebKitGTK issue tracker and the X11/GTK shape APIs, and the good news is that there's a clean, well-grounded answer that collapses both of your problems (ghosting *and* dead zones) into a single technique. Let me give you the bottom line first, then the implementation and the answers to your three specific angles.

## TL;DR — The Roundtable's Verdict

**Use Approach B (the fixed "envelope" window, never resized) and solve the dead-zone problem with an X11/GTK *input shape*, not with `set_ignore_cursor_events`.**

The critical insight is that **Tauri's `set_ignore_cursor_events(true)` is *already* an input-shape call under the hood** — it just hard-codes the region to a 1×1 rectangle (i.e., "ignore everything"). Here's the actual `tao` implementation on Linux:

```rust
// from tao/src/platform_impl/linux/event_loop.rs
WindowRequest::CursorIgnoreEvents(ignore) => {
  if ignore {
    let empty_region = Region::create_rectangle(&RectangleInt { x: 0, y: 0, width: 1, height: 1 });
    window.window().unwrap().input_shape_combine_region(&empty_region, 0, 0);
  } else {
    window.input_shape_combine_region(None)
  };
}
```

So the all-or-nothing limitation in your notes is **not a WebKitGTK limitation** — it's just that Tauri only exposes the two extreme cases. The X11 `ShapeInput` region (exposed in GTK as `gdk_window_input_shape_combine_region`) is *designed* for exactly your use case. From the GDK docs:

> An input shape is typically used with RGBA windows. The alpha channel of the window defines which pixels are **invisible**... and the input shape controls where the window is **"clickable"**.

This decouples the *visible* shape from the *clickable* shape entirely. You keep the full 540×200 envelope painted (so the compositor never sees a resize → **no ghosting**), but you tell the OS that only the head rectangle (and the bubble rectangle, when open) accept pointer events (→ **no dead zone**).

You replace `set_ignore_cursor_events` with your own Rust command that pushes the *actual* hitbox geometry.

---

## The Implementation (the part you actually want)

### 1. Window setup
- Create the `WebviewWindow` once at the **maximum envelope size** (540×200). `transparent: true`, `decorations: false`, `resizable: false`. **Never call `set_size`.**
- Keep your `rgba(0,0,0,0.005)` damage hack — it's orthogonal to input shaping and still needed (see §Damage below).

### 2. A Rust command that sets the input region from frontend-supplied rects

```rust
#[cfg(target_os = "linux")]
use gtk::prelude::*;

#[derive(serde::Deserialize)]
struct Hitbox { x: i32, y: i32, w: i32, h: i32 }

#[tauri::command]
fn set_input_hitboxes(window: tauri::WebviewWindow, boxes: Vec<Hitbox>) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        use cairo::{Region, RectangleInt};
        let gtk_win = window.gtk_window().map_err(|e| e.to_string())?;
        let gdk_win = gtk_win.window().ok_or("no gdk window")?;

        // IMPORTANT: account for HiDPI. GDK input regions are in *device* pixels.
        let scale = gtk_win.scale_factor();

        let region = Region::create();
        for b in &boxes {
            region.union_rectangle(&RectangleInt {
                x: b.x * scale, y: b.y * scale,
                width: b.w * scale, height: b.h * scale,
            });
        }
        // Empty vec -> empty region -> full click-through (same as ignore_cursor_events(true))
        gdk_win.input_shape_combine_region(&region, 0, 0);
    }
    Ok(())
}
```

### 3. Frontend: push the geometry whenever layout changes
Call this command on every state transition that changes the clickable footprint:
- On mount → `[{head rect}]`
- On bubble open → `[{head rect}, {bubble rect}]`
- On bubble close → `[{head rect}]`
- During free-drag → either the head rect (it tracks the CSS transform) or, if you want forgiving drag, a slightly inflated rect.

Because the region is specified in **window-relative coordinates** and is independent of the window's on-screen position, you only update it when the *internal layout* changes, not when the buddy moves the whole window. (Per the X spec: "region adjustments are not required when the window is moved.")

> ⚠️ **The HiDPI gotcha is the #1 thing that will bite you.** GDK input-shape coordinates are device pixels, while your CSS `getBoundingClientRect()` values are logical pixels. Multiply by `scale_factor()` (and reconcile with `devicePixelRatio` on the JS side). If your hitbox is offset or half-size on a 2× display, this is why.

---

<details>
<summary><strong>Angle 1 — GTK/WebKitGTK specifics: the exact APIs and the Wayland caveat</strong></summary>

The relevant call differs by toolkit version, and **this matters for Wayland**:

| Backend | API | Notes |
|---|---|---|
| Raw X11 | `XShapeCombineRegion(dpy, win, ShapeInput, ...)` | The original. `ShapeInput` ≠ `ShapeBounding`. |
| GTK3 (what Tauri v2 uses today) | `gdk_window_input_shape_combine_region()` | What the snippet above uses via `gtk-rs`. |
| GTK4 | `gdk_surface_set_input_region()` | If/when Tauri moves to GTK4. |

**Wayland is actually fine here** — input regions are part of the *core* Wayland protocol (`wl_surface.set_input_region`), so GTK forwards your region correctly. The thing to guard against is older/headless setups: call `gdk_display_supports_input_shapes()` and fall back gracefully (to the toggle approach in Angle 3) if it returns false. On X11, also confirm a compositor is running (`is_composited()`), since per-pixel alpha transparency needs one.

For **per-pixel / antialiased** click-through (e.g., a circular head where even the corners of its bounding rect pass through), `input_shape_combine_region` also accepts regions built from a 1-bit mask via Cairo (`OPERATOR_CLEAR` + filled arc). In practice, **rectangles are enough** for a head + speech bubble and are dramatically simpler and cheaper. Don't reach for the pixmap-mask route unless you genuinely need sub-rectangle precision.

</details>

<details>
<summary><strong>Angle 2 — Why the resize/new-window lifecycle hacks are a trap (and the ghosting root cause)</strong></summary>

**Don't resize, and don't create-new-window.** The input-shape approach makes both unnecessary, which is the whole point.

The ghosting you're seeing is a confirmed upstream **WebKitGTK regression**, not something you can reliably engineer around:
- Tauri **#12800** — transparent webview doesn't clear the old render until a resize occurs (Cinnamon, GNOME). Confirmed fixed only in WebKitGTK **2.48.0** on some stacks, still broken on others.
- Tauri **#13157 / #13906** — repeated resize of a transparent window causes a "shadow copy" of the DOM and content overflow. Maintainer's words: *"why does every webkitgtk release make things worse."*
- The NVIDIA proprietary-driver path (`Failed to create GBM buffer`, DMA-BUF) is its own ghosting hell (wry **#1667**). One project literally shipped a PR titled *"resolve NVIDIA ghosting artifacts using window resize hack"* and ultimately **disabled transparency on NVIDIA** as the only stable option.

The takeaway: resize-driven repaint is *simultaneously* the cause of your ghosting **and** the workaround others use to force repaints. It's a tar pit. By using a fixed envelope + input shapes you sidestep the resize codepath entirely.

The "create new bigger window, transfer state, destroy old one" idea works mechanically but reintroduces a visible flash/focus-steal on Linux WMs and a fresh WebKit process spin-up cost — strictly worse UX than the envelope. Skip it.

</details>

<details>
<summary><strong>Angle 3 — Electron comparison & the zero-native-code fallback</strong></summary>

**What SPACE_AGENT almost certainly did:** Electron's `win.setIgnoreMouseEvents(true, { forward: true })`. The `forward: true` flag is the magic — it ignores *clicks* but **still forwards `mousemove`** to the renderer, so JS can detect hover and toggle ignore on/off per-region in real time. Under the hood on Linux, Chromium uses the *same* XShape input-region mechanism — Chromium just has more robust transparency compositing than WebKitGTK, which is why it "felt smoother." It's not a different capability; it's a more stable implementation of the same X11 primitive.

**Tauri lacks the `{ forward: true }` equivalent**, which is why naive `set_ignore_cursor_events` toggling feels broken (when ignore is on, the webview gets *no* events, so you can't detect when the cursor re-enters the head). There are two ways to match Electron:

1. **Native input shape (recommended, what's above)** — this is the *superior* approach to Electron's toggle, because the region is enforced by the X server with zero per-frame work. No global mouse polling, no race conditions.

2. **Global-cursor-tracking toggle (cross-platform fallback)** — documented in Tauri **#13070**: run a global pointer listener in Rust (via `device_query`/udev), emit `device-mouse-move` to the frontend, run a JS hitbox test, and toggle `setIgnoreCursorEvents(shouldIgnore)`. This replicates Electron's `forward:true` behavior without native GTK code and works on Windows/macOS too. Downsides: ~16ms polling jitter, and it can interfere with other always-on-top apps. Use this only as the fallback when `supports_input_shapes()` is false.

</details>

---

## On the damage-region hack

Keep your `rgba(0,0,0,0.005)` trick — input shaping does **not** replace it. They solve different problems:
- **Input shape** → *who receives clicks* (the X server's job).
- **Damage tracking** → *whether the compositor bothers repainting* (the GPU/compositor's job).

If you find the alpha hack flaky across compositors, the more robust lever is the renderer backend itself. The env vars surfaced in the NVIDIA thread are worth A/B testing on your target hardware:

```bash
WEBKIT_DISABLE_DMABUF_RENDERER=1   # bypass the GBM/DMA-BUF path that ghosts on NVIDIA
GSK_RENDERER=gl                    # or =cairo as a last resort (CPU, slower, but predictable)
```

You can set these from your Rust `main()` before the webview spins up. On Intel/AMD with WebKitGTK ≥ 2.48 you generally don't need them; on NVIDIA proprietary you very likely will.

---

## Recommended path forward

1. **Adopt the fixed envelope + input-shape architecture** (the code above). This is the single change that kills both ghosting and dead zones.
2. **Feature-detect** `gdk_display_supports_input_shapes()`; fall back to the global-cursor toggle (#13070) only if unsupported.
3. **Pin/bundle WebKitGTK 2.48+** in your AppImage (the threads show CI vs. local version drift causes "works on my machine" ghosting). Bundle the lib rather than relying on the host.
4. **Special-case NVIDIA**: detect the driver and either set the `WEBKIT_DISABLE_DMABUF_RENDERER` flag or, as a graceful degradation, disable transparency + rounded corners (what the clipboard-history project ultimately shipped).
5. **Nail the HiDPI scale factor** in the region math before you do anything else — it's the most common silent failure.

Would you like me to sketch the full Tauri v2 plugin (command registration, the `gtk-rs`/`cairo` Cargo features you need to enable, and the React hook that recomputes hitboxes from `getBoundingClientRect` with `devicePixelRatio` reconciliation)?

Grok 4.3: 
**Yes.** This is one of those beautiful, thorny desktop problems that rewards exactly the kind of platform-deep thinking you brought to the roundtable. The ghosting on resize and the invisible dead zones are both symptoms of the same underlying mismatch: WebKitGTK’s compositor expectations versus the shape-shifting, click-through UI we actually want.

Opus nailed the diagnosis and the cure. I’m here to turn that into a complete, shippable path.

### The Winning Architecture (in one sentence)

**Create one fixed-size, transparent, frameless `WebviewWindow` at your maximum envelope (e.g. 540×220). Never resize it.** Paint the buddy head + speech bubble inside it with CSS/ React. Use a custom Rust command that calls `gdk_window_input_shape_combine_region` (via the exposed `gtk_window()`) to tell the X server / Wayland compositor exactly which rectangles should receive pointer events. Everything else becomes click-through automatically.

This single change eliminates both problems at once:
- No resize → no WebKitGTK ghosting / trailing frames (a well-documented pain point on Linux, especially with NVIDIA + DMA-BUF paths).
- Precise input regions → no dead zones, and you keep full interactivity on the visible parts.

It’s the same primitive Electron’s `{ forward: true }` ultimately uses under the hood, just exposed properly instead of being all-or-nothing.

### 1. Window Creation (never resize this)

In `tauri.conf.json` (or when you create the window programmatically):

```json
{
  "windows": [
    {
      "label": "buddy",
      "transparent": true,
      "decorations": false,
      "resizable": false,
      "alwaysOnTop": true,
      "skipTaskbar": true,
      "width": 540,
      "height": 220,
      "visible": false
    }
  ]
}
```

In Rust `main.rs` (or your setup function):

```rust
let buddy = tauri::WebviewWindowBuilder::new(app, "buddy", tauri::WebviewUrl::default())
    .transparent(true)
    .decorations(false)
    .resizable(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .inner_size(540.0, 220.0)
    .visible(false)
    .build()?;
```

Keep your `rgba(0,0,0,0.005)` damage hack on the `<body>` — it’s still needed to keep the compositor paying attention during animations.

### 2. The Rust Command (the magic)

Add this to your `src-tauri/src/main.rs` (or a commands module). You’ll need matching GTK/cairo dependencies for your Tauri version (usually `gtk = "0.18"` / `cairo-rs = "0.18"` — run `cargo tree | grep gtk` to match exactly and avoid conflicts).

```rust
use tauri::{command, WebviewWindow};

#[derive(serde::Deserialize)]
pub struct Hitbox {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

#[command]
pub fn set_input_hitboxes(window: WebviewWindow, boxes: Vec<Hitbox>) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        use gtk::prelude::*;
        use cairo::{RectangleInt, Region};

        let gtk_win = window.gtk_window().map_err(|e| e.to_string())?;
        let gdk_win = gtk_win.window().ok_or("No GDK window")?;

        let scale = gtk_win.scale_factor();

        let region = Region::create();
        for b in &boxes {
            region.union_rectangle(&RectangleInt {
                x: b.x * scale,
                y: b.y * scale,
                width: b.w * scale,
                height: b.h * scale,
            });
        }

        gdk_win.input_shape_combine_region(&region, 0, 0);
    }
    Ok(())
}
```

Register it with `tauri::generate_handler![set_input_hitboxes]`.

**HiDPI note (critical):** GDK input regions are in *device pixels*. Your JS `getBoundingClientRect()` values are CSS/logical pixels. The code above multiplies by `scale_factor()`. On the JS side you can also read `window.devicePixelRatio` and pass pre-scaled values if you prefer — just stay consistent.

### 3. Frontend: React Hook + Measurement

```ts
// hooks/useInputHitboxes.ts
import { invoke } from '@tauri-apps/api/core';
import { useCallback } from 'react';

export interface Hitbox { x: number; y: number; w: number; h: number }

export function useInputHitboxes() {
  const setHitboxes = useCallback(async (boxes: Hitbox[]) => {
    await invoke('set_input_hitboxes', { boxes });
  }, []);

  return { setHitboxes };
}
```

In your `Buddy` component (simplified):

```tsx
const headRef = useRef<HTMLDivElement>(null);
const bubbleRef = useRef<HTMLDivElement>(null);
const { setHitboxes } = useInputHitboxes();

const updateHitboxes = useCallback(() => {
  const boxes: Hitbox[] = [];
  
  if (headRef.current) {
    const r = headRef.current.getBoundingClientRect();
    boxes.push({ x: r.left, y: r.top, w: r.width, h: r.height });
  }
  if (isBubbleOpen && bubbleRef.current) {
    const r = bubbleRef.current.getBoundingClientRect();
    boxes.push({ x: r.left, y: r.top, w: r.width, h: r.height });
  }
  
  setHitboxes(boxes);
}, [isBubbleOpen, setHitboxes]);

// Call on mount, on bubble toggle, after any layout/animation change
useLayoutEffect(() => {
  updateHitboxes();
}, [updateHitboxes]);

// Optional: ResizeObserver if content can reflow
```

When the buddy is “tucked” at the edge, only send the head rect. When the speech bubble pops, send both. When the user drags it into free space, keep sending the current visual rects (or a slightly larger forgiving one for the head).

### 4. Dragging & Interaction

Because only the hitbox rectangles receive events:
- Clicks on the head → your React handler (toggle bubble or start drag).
- Everything else in the big transparent envelope → passes straight through to whatever is underneath. Perfect.

For dragging the whole buddy:

```ts
import { getCurrentWindow } from '@tauri-apps/api/window';

const win = getCurrentWindow();

const onHeadMouseDown = (e: React.MouseEvent) => {
  if (e.button === 0) {
    win.startDragging();
  }
};
```

Attach that to the head element. When the buddy is in “free” mode you can let the user drag it anywhere; when near an edge you can programmatically `setPosition()` to tuck it so only the head remains visible.

### 5. Extra Polish & Gotchas

- **NVIDIA / DMA-BUF ghosting**: In `main.rs` before building the app, you can set:
  ```rust
  if std::env::var("__GLX_VENDOR_LIBRARY_NAME").is_ok() || /* other NVIDIA heuristics */ {
      std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
  }
  ```
  Or just document it for users. WebKitGTK 2.48+ helps a lot here.

- **Wayland vs X11**: The same `input_shape_combine_region` call works on both (Wayland translates it to `wl_surface.set_input_region`). Test on both.

- **Feature detection / fallback**: If for some reason input shapes aren’t supported, you can fall back to the global mouse-tracking + `set_ignore_cursor_events` toggle pattern (Tauri issue #13070 / #6164). But on modern desktops this is rarely needed.

- **Animation without pain**: Use CSS transitions / framer-motion / React Spring on the head and bubble. Because the window never resizes, the compositor stays happy.

- **Multiple buddies**: Each gets its own `WebviewWindow` + label. They can communicate via Tauri events or a shared Rust state if needed.

### The Roundtable’s Final Call

This approach is cleaner than resize hacks, more performant than polling mouse position, and gives you true per-region click-through without sacrificing any interactivity. It’s the same technique that makes well-behaved desktop pets and overlays possible on Linux.

You now have everything you need to implement it. The remaining work is mostly wiring the measurement logic to your specific buddy states (tucked vs free, bubble open/closed, drag mode) and making the animations feel alive.

Would you like me to:
- Generate the complete, copy-paste-ready Rust command + full React hook + example component?
- Help with the “tuck at edge” positioning math and state machine?
- Or sketch how to bundle a pinned WebKitGTK 2.48+ in your AppImage for maximum compatibility?

Just say the word and we’ll keep iterating until these little border agents feel *perfect*. They deserve it. 🚀


GPT 5.5:
Richard — **the UX is absolutely viable**, and the technical path is now clear enough to start without poisoning the product.

The mistake would be treating this as a “desktop mascot problem.” It is not. The correct product framing is:

> **Border Agents are edge-native interactive companions: small when quiet, expressive when needed, and never in the user’s way.**

That “never in the way” part is the whole technical battle.

## GPT 5.5 verdict

Use **fixed maximum-size transparent windows** and **native Linux input regions**.

Do **not** resize the Tauri window when the speech bubble opens. Your own challenge states the exact trap: dynamic resize causes transparent WebKitGTK ghosting, while a fixed envelope blocks clicks in invisible areas. 

The winning design is:

```text
One fixed transparent envelope window
        ↓
CSS animates the head / bubble inside it
        ↓
Rust updates the clickable hitbox region
        ↓
Transparent areas become real click-through
```

That preserves the UX illusion: the buddy feels like it grows, talks, tucks, and detaches — but the OS never sees a risky resize.

## Why this is the right start

Your actual objective is small unobtrusive buddies that live on desktop edges, expand visually, drag free, and allow click-through on transparent space.  The Linux-native API exists for exactly that. GTK’s `gdk_window_input_shape_combine_region()` says mouse events outside the input shape pass to the window below, and that RGBA windows use alpha for visibility while the input shape controls where the window is clickable. ([https://docs.gtk.org][1])

Wayland also has the same concept: `wl_surface.set_input_region` defines the part of a surface that receives pointer/touch events; events outside it try the next surface underneath. ([Wayland][2])

Tauri already exposes enough Linux window access to do this properly: its `Window` has `gtk_window()` on Linux/BSD, returning the GTK `ApplicationWindow`, and `start_dragging()` is also available for native dragging. ([Docs.rs][3])

## Build slice 1: “one buddy proves the magic”

Do this before personality systems, multi-agent choreography, themes, or speech.

### 1. Create one fixed overlay envelope

Example: `540 × 220`, transparent, frameless, always-on-top, not resizable.

Inside it:

```text
[ transparent envelope ]
  ├─ buddy head: visible + clickable
  └─ speech bubble: visible/clickable only when open
```

The window itself never changes size.

### 2. Add one Rust command: `set_input_hitboxes`

Frontend sends rectangles:

```ts
[
  { x: headLeft, y: headTop, w: headWidth, h: headHeight },
  { x: bubbleLeft, y: bubbleTop, w: bubbleWidth, h: bubbleHeight }
]
```

Rust converts those rectangles into a Cairo/GDK region and applies it as the input shape.

Conceptually:

```rust
#[tauri::command]
fn set_input_hitboxes(window: tauri::Window, boxes: Vec<Hitbox>) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        use gtk::prelude::*;
        use cairo::{Region, RectangleInt};

        let gtk_win = window.gtk_window().map_err(|e| e.to_string())?;
        let gdk_win = gtk_win.window().ok_or("No GDK window")?;

        let scale = gtk_win.scale_factor();
        let region = Region::create();

        for b in boxes {
            region.union_rectangle(&RectangleInt {
                x: b.x * scale,
                y: b.y * scale,
                width: b.w * scale,
                height: b.h * scale,
            });
        }

        gdk_win.input_shape_combine_region(&region, 0, 0);
    }

    Ok(())
}
```

Critical: **test HiDPI immediately**. If your clickable area is offset, half-sized, or weird on a 2× display, your scale math is wrong.

### 3. Keep the compositor damage hack for now

Your file notes the `rgba(0, 0, 0, 0.005)` trick is being used to keep Linux compositors tracking redraws for transparent windows.  Keep it during the first slice. Do not try to solve animation damage and click-through in the same pass.

Input shape solves **click routing**.

The alpha/damage hack solves **repaint attention**.

Different problems.

## Product decision

The UX sell should be:

> **They live at the edge, not on top of your work.**
> Border Agents only occupy the pixels they visibly use. Their invisible space is not a dead zone. They can whisper, pop out, retreat, or detach — without stealing the desktop.

That is the emotional hook and the technical differentiator.

## Avoid these traps

Do **not** build around dynamic resizing. That path risks ghosting and makes the whole experience feel cheap.

Do **not** rely on `set_ignore_cursor_events(true)` as the main solution. It is too blunt: either the whole window gets events or none of it does.

Do **not** start with multiple agents. Start with one agent, one edge, one bubble, one drag mode, and one Linux desktop environment.

Do **not** over-perfect circular per-pixel hit testing yet. Rectangles are enough for v0: head rect + bubble rect + optional close/action rects.

## The first successful demo

The first demo should show only this:

1. A small buddy head tucked on the screen edge.
2. You can click the head.
3. A speech bubble pops out smoothly.
4. You can click through the transparent envelope area to the app underneath.
5. You can drag the buddy free.
6. It can re-tuck to the edge.

That demo sells Border Agents better than any architecture doc.

The phrase I’d lock for the build:

> **No ghosting. No dead zones. No desktop theft.**

[1]: https://docs.gtk.org/gdk3/method.Window.input_shape_combine_region.html "Gdk.Window.input_shape_combine_region"
[2]: https://wayland.app/protocols/wayland "Wayland protocol | Wayland Explorer"
[3]: https://docs.rs/tauri/latest/tauri/window/struct.Window.html "Window in tauri::window - Rust"


Gemini Pro 3.1:
The consensus from the Roundtable is absolutely correct. Approach B (the fixed envelope) combined with native X11/GTK input shapes is the only viable path to bypass WebKitGTK’s resizing artifacts while maintaining partial click-through. Opus, Grok, and GPT 5.5 have thoroughly covered the GTK API, the Rust implementation, and the product framing.

As the final voice here, I am going to focus on the **runtime execution and edge cases**. Getting the API to work is step one; preventing the prototype from lagging your compositor or breaking across different Linux desktop environments (DEs) is step two.

Here are the critical performance safeguards and Wayland/X11 quirks you need to implement to make this bulletproof.

### 1. IPC Throttling and the Dragging Trap

Opus correctly noted that you only need to update the input region when the *internal layout* changes. However, when a user drags the buddy around the screen, there is a temptation to continuously poll `getBoundingClientRect()` and send the new coordinates to Rust via Tauri IPC.

**Do not do this.** Flooding the IPC bridge and the GTK main thread with `input_shape_combine_region` calls on every `mousemove` will cause severe stuttering.

* **The Solution:** Rely on Tauri's native drag. When the user clicks the buddy head to drag, the window system captures the mouse. You do not need to update the input shape while the window is moving because the input shape is *relative to the window envelope*, not the screen. Update the hitboxes strictly on state transitions (e.g., `Idle` $\rightarrow$ `Speaking`, `Tucked` $\rightarrow$ `Free`).

### 2. The Wayland Shadow Artifact

While Wayland supports `wl_surface.set_input_region`, Linux window managers (like GNOME’s Mutter or KDE’s KWin) often try to be helpful by rendering a drop-shadow around the bounding box of top-level windows. If you create a 540x220 transparent envelope, some WMs will draw a faint, 540x220 rectangular shadow on the desktop, completely ruining the illusion.

* **The Solution:** Explicitly disable shadows at the OS level. Tauri's `decorations: false` usually handles this, but on Linux, you may need to explicitly set the GTK window type hint or use a CSS property to ensure the compositor knows the alpha channel is authoritative for shadows.

### 3. Frontend `pointer-events` Masking

For the OS-level click-through to work flawlessly, your webview’s DOM must not intercept clicks in the transparent areas. If the HTML `<body>` or the Tauri `#root` div has standard pointer events enabled, WebKitGTK might consume the click before the OS realizes it should pass it through the GTK input shape.

* **The Solution:** Apply strict CSS pointer event rules.
```css
/* The 540x220 envelope */
body, #root {
  width: 100%;
  height: 100%;
  pointer-events: none; /* Ignore all clicks globally */
  background: rgba(0, 0, 0, 0.005); /* The damage hack */
}

/* The interactive elements */
.buddy-head, .speech-bubble {
  pointer-events: auto; /* Re-enable clicks only where needed */
  will-change: transform; /* Hardware acceleration for animations */
}

```



```

### 4. The `will-change` Animation Rule
Because you are working within a fixed envelope, the head and the speech bubble will be moving via CSS transforms (`translate3d`, `scale`). WebKitGTK on Linux can be notoriously sluggish with transparent repaints. By enforcing `will-change: transform` on the animated elements, you force the browser engine to composite those specific layers on the GPU, minimizing the "damage region" repaints to just the bounding boxes of the moving elements rather than the whole 540x220 window.

---

### The Final Execution Checklist
1.  **Window:** Create the static 540x220 envelope (`resizable: false`, `transparent: true`).
2.  **Rust:** Implement the `set_input_hitboxes` command using `gdk_window_input_shape_combine_region`, factoring in the HiDPI scale factor.
3.  **React:** Set `pointer-events: none` on the background and `auto` on the interactive elements.
4.  **State:** Only trigger the Rust command on distinct layout state changes, never during continuous animations or drags.

This architecture sidesteps the WebKitGTK ghosting bug entirely while giving you pixel-perfect, zero-dead-zone click-through. 

To help nail down the frontend implementation: how do you plan to handle the coordinate state transitions—will the React frontend act as the single source of truth for calculating these hitboxes, or will you manage the math strictly in Rust based on predefined "buddy states"?

```

Kimi 2.6:
Richard — I've read the roundtable document. The consensus is solid: **fixed envelope + native input shapes** is the correct architecture. Let me add the specific angles the other models didn't fully address, and give you a clean implementation path.

---

## Kimi's Verdict

**The core insight everyone agrees on:** `gdk_window_input_shape_combine_region()` is the primitive. The real question is how to wire it into a Tauri v2 app *today* without dependency hell, and what the frontend contract should look like.

---

## Three Things the Roundtable Glossed Over

### 1. The `gtk-rs` Dependency Trap

Tauri v2 already links GTK internally. Adding `gtk = "0.18"` or `cairo-rs` directly to your `Cargo.toml` will **conflict** with Tauri's bundled GTK version. The correct approach is to use the **re-exported GTK types** from `tauri` itself, or gate your feature behind Tauri's platform-specific APIs.

**Correct path:** Use `window.gtk_window()` (already exposed by Tauri on Linux) and access GDK via `gtk::prelude::*` **only if** you enable the `linux` feature in your Tauri plugin. Do not add standalone `gtk` or `cairo` crates unless you pin the exact version Tauri uses.

```rust
// In your Cargo.toml — NO standalone gtk crate needed
// Tauri already re-exports what you need via tauri::window::PlatformWebviewWindow

#[cfg(target_os = "linux")]
use gtk::prelude::*; // This comes from Tauri's transitive deps, not your own
```

If you need `cairo::Region`, you may need `cairo-rs = "0.18"` but **pin it to the exact same version Tauri resolves**. Run `cargo tree | grep cairo` in your `src-tauri` directory to verify.

---

### 2. The `window.gtk_window()` API Reality Check

Tauri v2's `WebviewWindow` exposes `gtk_window()` on Linux, but it returns a `Result` and the exact type is `gtk::ApplicationWindow` (GTK3). The method is:

```rust
pub fn gtk_window(&self) -> Result<<gtk::ApplicationWindow>
```

From there, you call `.window()` to get the `gdk::Window` (note: `gdk` not `gdk4` — Tauri v2 uses GTK3).

**The actual working code:**

```rust
#[tauri::command]
fn set_input_hitboxes(window: tauri::WebviewWindow, boxes: Vec<<Hitbox>) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        let gtk_win = window.gtk_window().map_err(|e| e.to_string())?;
        let gdk_win = gtk_win.window().ok_or("Window not realized")?;
        
        let scale = gtk_win.scale_factor();
        
        // cairo::Region is available via gdk's re-export or cairo-rs
        let region = cairo::Region::create();
        for b in &boxes {
            region.union_rectangle(&cairo::RectangleInt {
                x: b.x * scale,
                y: b.y * scale,
                width: b.w * scale,
                height: b.h * scale,
            });
        }
        
        gdk_win.input_shape_combine_region(&region, 0, 0);
    }
    Ok(())
}
```

**Critical:** If `window.gtk_window()` fails, it's usually because the window hasn't been fully realized yet. Call this command **after** the window is visible and the webview has loaded.

---

### 3. The Frontend Contract: Who Owns the Truth?

The roundtable suggests `getBoundingClientRect()` in React. This is correct, but there's a cleaner abstraction: **the frontend owns the visual layout; the backend owns the input shape.** The contract is a simple array of rectangles.

**Recommended React hook:**

```typescript
// hooks/useBuddyHitbox.ts
import { useCallback, useRef, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';

export interface Hitbox {
  x: number;
  y: number;
  w: number;
  h: number;
}

export function useBuddyHitbox() {
  const pending = useRef<<Hitbox[]>([]);
  const raf = useRef<number | null>(null);

  const flush = useCallback(() => {
    if (raf.current) cancelAnimationFrame(raf.current);
    raf.current = requestAnimationFrame(() => {
      invoke('set_input_hitboxes', { boxes: pending.current });
    });
  }, []);

  const setHitboxes = useCallback((boxes: Hitbox[]) => {
    pending.current = boxes;
    flush();
  }, [flush]);

  // Cleanup on unmount
  useEffect(() => () => {
    if (raf.current) cancelAnimationFrame(raf.current);
  }, []);

  return { setHitboxes };
}
```

**Why `requestAnimationFrame`?** Debouncing via rAF ensures you don't spam IPC during rapid state transitions (e.g., bubble open/close animation). The hitbox update happens once per frame, not per React render.

---

## The "Tuck" State Machine

You need four states, not two:

| State | Visual | Hitbox | Behavior |
|-------|--------|--------|----------|
| `TUCKED` | Head only, at edge | Head rect | Click to expand |
| `PEEKING` | Head + small indicator | Head rect + indicator rect | Hover or click to expand |
| `EXPANDED` | Head + bubble | Head rect + bubble rect | Click bubble to dismiss |
| `FREE` | Head (maybe bubble) | Head rect (+ bubble if open) | Draggable anywhere |

**State transitions:**

```
TUCKED --(click)--> EXPANDED --(click bubble/timeout)--> TUCKED
TUCKED --(drag)--> FREE --(release near edge)--> TUCKED
FREE --(release in center)--> FREE (stays floating)
```

The hitbox updates only on state transitions, not during drag. During drag, Tauri's `startDragging()` handles the window movement; the input shape is window-relative and doesn't change.

---

## The Complete Minimal Viable Buddy

Here's the smallest working implementation that proves the concept:

### `src-tauri/src/main.rs`

```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri::{Manager, WebviewWindow};

#[derive(serde::Deserialize, Clone, Debug)]
struct Hitbox {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}

#[tauri::command]
fn set_input_hitboxes(window: WebviewWindow, boxes: Vec<<Hitbox>) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        use gtk::prelude::*;
        
        let gtk_win = window.gtk_window().map_err(|e| e.to_string())?;
        let gdk_win = gtk_win.window().ok_or("Window not realized")?;
        
        let scale = gtk_win.scale_factor();
        let region = cairo::Region::create();
        
        for b in &boxes {
            region.union_rectangle(&cairo::RectangleInt {
                x: b.x * scale,
                y: b.y * scale,
                width: b.w * scale,
                height: b.h * scale,
            });
        }
        
        gdk_win.input_shape_combine_region(&region, 0, 0);
    }
    Ok(())
}

#[tauri::command]
fn get_scale_factor(window: WebviewWindow) -> Result<f64, String> {
    #[cfg(target_os = "linux")]
    {
        let gtk_win = window.gtk_window().map_err(|e| e.to_string())?;
        return Ok(gtk_win.scale_factor() as f64);
    }
    #[cfg(not(target_os = "linux"))]
    {
        Ok(1.0)
    }
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![set_input_hitboxes, get_scale_factor])
        .setup(|app| {
            let window = app.get_webview_window("buddy").unwrap();
            // Set initial position (e.g., top-right corner)
            let _ = window.set_position(tauri::Position::Physical(tauri::PhysicalPosition { x: 1720, y: 20 }));
            let _ = window.show();
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

### `src-tauri/tauri.conf.json` (relevant window config)

```json
{
  "windows": [
    {
      "label": "buddy",
      "url": "index.html",
      "transparent": true,
      "decorations": false,
      "resizable": false,
      "alwaysOnTop": true,
      "skipTaskbar": true,
      "width": 540,
      "height": 220,
      "visible": false,
      "center": false
    }
  ]
}
```

### `src/App.tsx` (minimal)

```tsx
import { useRef, useState, useCallback, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import './App.css';

const win = getCurrentWindow();

interface Hitbox {
  x: number;
  y: number;
  w: number;
  h: number;
}

function App() {
  const [isOpen, setIsOpen] = useState(false);
  const [isFree, setIsFree] = useState(false);
  const headRef = useRef<<HTMLDivElement>(null);
  const bubbleRef = useRef<<HTMLDivElement>(null);
  const scaleRef = useRef(1);

  // Get scale factor once on mount
  useEffect(() => {
    invoke('get_scale_factor').then((s: unknown) => {
      scaleRef.current = s as number;
    });
  }, []);

  const updateHitboxes = useCallback(() => {
    const boxes: Hitbox[] = [];
    const dpr = window.devicePixelRatio;
    const scale = scaleRef.current;

    if (headRef.current) {
      const r = headRef.current.getBoundingClientRect();
      boxes.push({
        x: Math.round(r.left * dpr / scale),
        y: Math.round(r.top * dpr / scale),
        w: Math.round(r.width * dpr / scale),
        h: Math.round(r.height * dpr / scale),
      });
    }

    if (isOpen && bubbleRef.current) {
      const r = bubbleRef.current.getBoundingClientRect();
      boxes.push({
        x: Math.round(r.left * dpr / scale),
        y: Math.round(r.top * dpr / scale),
        w: Math.round(r.width * dpr / scale),
        h: Math.round(r.height * dpr / scale),
      });
    }

    invoke('set_input_hitboxes', { boxes });
  }, [isOpen]);

  // Update hitboxes on state changes
  useEffect(() => {
    updateHitboxes();
  }, [isOpen, updateHitboxes]);

  const handleHeadClick = useCallback(() => {
    setIsOpen(prev => !prev);
  }, []);

  const handleHeadMouseDown = useCallback((e: React.MouseEvent) => {
    if (e.button === 0) {
      // Start drag on left click
      win.startDragging();
      // After drag ends, check if near edge to tuck
      setTimeout(() => {
        win.outerPosition().then(pos => {
          const { x } = pos;
          const screenWidth = window.screen.width;
          if (x < 50 || x > screenWidth - 590) {
            setIsFree(false);
            // Auto-tuck to nearest edge
            const edgeX = x < screenWidth / 2 ? 0 : screenWidth - 540;
            win.setPosition({ type: 'Physical', x: edgeX, y: 20 });
          } else {
            setIsFree(true);
          }
        });
      }, 100);
    }
  }, []);

  return (
    <div className="envelope">
      <div
        ref={headRef}
        className="buddy-head"
        onClick={handleHeadClick}
        onMouseDown={handleHeadMouseDown}
      >
        {/* Your buddy SVG/avatar here */}
        <div className="head-inner">🤖</div>
      </div>

      {isOpen && (
        <div ref={bubbleRef} className="speech-bubble">
          <p>Hello, Richard. I'm ready.</p>
        </div>
      )}
    </div>
  );
}

export default App;
```

### `src/App.css`

```css
* {
  margin: 0;
  padding: 0;
  box-sizing: border-box;
}

body {
  width: 100vw;
  height: 100vh;
  background: rgba(0, 0, 0, 0.005); /* Damage hack */
  overflow: hidden;
  pointer-events: none; /* Global pass-through */
}

#root {
  width: 100%;
  height: 100%;
}

.envelope {
  width: 100%;
  height: 100%;
  position: relative;
  pointer-events: none;
}

.buddy-head {
  position: absolute;
  top: 20px;
  left: 20px;
  width: 116px;
  height: 116px;
  border-radius: 50%;
  background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  pointer-events: auto; /* Re-enable clicks */
  will-change: transform;
  transition: transform 0.2s ease;
  box-shadow: 0 4px 20px rgba(102, 126, 234, 0.4);
}

.buddy-head:hover {
  transform: scale(1.05);
}

.head-inner {
  font-size: 48px;
  user-select: none;
}

.speech-bubble {
  position: absolute;
  top: 20px;
  left: 140px;
  width: 380px;
  height: 116px;
  background: rgba(30, 30, 40, 0.95);
  border-radius: 16px;
  padding: 16px 20px;
  color: #e0e0e0;
  font-family: system-ui, sans-serif;
  font-size: 14px;
  line-height: 1.5;
  pointer-events: auto; /* Re-enable clicks */
  will-change: opacity, transform;
  animation: bubbleIn 0.25s ease-out;
  backdrop-filter: blur(10px);
  border: 1px solid rgba(255, 255, 255, 0.1);
}

@keyframes bubbleIn {
  from {
    opacity: 0;
    transform: translateX(-10px) scale(0.95);
  }
  to {
    opacity: 1;
    transform: translateX(0) scale(1);
  }
}

.speech-bubble p {
  margin: 0;
}
```

---

## The One-Line Fix for NVIDIA

Add this to your `main.rs` before `tauri::Builder`:

```rust
// Force software rendering on NVIDIA to avoid DMA-BUF ghosting
if std::env::var("WEBKIT_DISABLE_DMABUF_RENDERER").is_err() {
    // Detect NVIDIA
    if std::path::Path::new("/proc/driver/nvidia/version").exists() {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }
}
```

---

## Final Checklist

| Item | Status |
|------|--------|
| Fixed envelope window (540×220) | ✅ |
| `transparent: true`, `decorations: false` | ✅ |
| `set_input_hitboxes` Rust command | ✅ |
| `pointer-events: none` on envelope, `auto` on interactive elements | ✅ |
| HiDPI scale factor handling | ✅ |
| `requestAnimationFrame` debounced IPC | ✅ |
| Damage hack (`rgba(0,0,0,0.005)`) | ✅ |
| NVIDIA DMA-BUF workaround | ✅ |
| State machine (TUCKED → EXPANDED → FREE) | ✅ |

---

**Bottom line:** The roundtable got the architecture right. My addition is the **implementation precision**: dependency management, the exact Tauri v2 API surface, the frontend contract, and the edge cases (NVIDIA, HiDPI, IPC throttling) that will kill you in production.

Want me to expand the state machine, add multi-buddy orchestration, or wire up the speech bubble content to your VAJRA/Constimetry message bus?

DeepSeek:
# DeepSeek Final Verdict

You already have the solution from four strong models — all pointing to **fixed envelope + native input shapes** as the only production-worthy path on Linux. I’ll give you the condensed, battle‑tested answer and the one thing everyone else left implicit.

## The Inevitable Architecture

- **Window:** created once at maximum size (e.g., 540×220), transparent, frameless, never resized.  
- **Hit zones:** Rust command that calls `gdk_window_input_shape_combine_region()` with rectangles from the frontend.  
- **CSS:** `pointer-events: none` on the envelope, `pointer-events: auto` on the head and bubble.  
- **Damage hack:** keep `rgba(0,0,0,0.005)` on `<body>` to force compositor redraws.  
- **NVIDIA workaround:** `WEBKIT_DISABLE_DMABUF_RENDERER=1` in production.

## The Missing Implementation Note

All models gave solid code, but none stressed the **order of operations** that actually makes it work reliably:

1. **Show the window first** (`window.show()`) before calling `set_input_hitboxes`.  
   - The GDK window is not realized until the window is visible. Calling `gtk_window().window()` too early returns `None`.

2. **Send hitboxes after every layout change** — but never during a drag.  
   - Use `requestAnimationFrame` + a short debounce (Kimi’s approach is good).  
   - Do **not** send hitboxes while the user is dragging the window; the region is window‑relative, so it stays correct without updates.

3. **Hitbox coordinates must be in device pixels** relative to the **window’s client area**, not the screen.  
   - `getBoundingClientRect()` already returns coordinates relative to the viewport (which equals the window’s client area if the window isn’t scrolling).  
   - Multiply by `window.devicePixelRatio` and then divide by the GTK scale factor (if you use the Rust `scale_factor()`). Or simpler: in your Rust command, multiply the incoming logical coordinates by `scale_factor()` without touching `devicePixelRatio` — that matches the GDK coordinate space.

## Why This Works (and the others don’t)

| Approach | Ghosting | Dead zones | Works on Linux |
|----------|----------|------------|----------------|
| Dynamic resize | ✅ severe (WebKitGTK bug) | ❌ | ❌ |
| Fixed envelope + `set_ignore_cursor_events` | ❌ | ✅ (whole window click‑through or none) | ❌ |
| Fixed envelope + input shapes | ❌ | ❌ (partial click‑through) | ✅ |

## One Decision You Must Make

**Who computes the hitbox rectangles?**

- **Frontend‑side** (React): easy to get `getBoundingClientRect()`, but you must pass logical coordinates and handle scale factor.  
- **Backend‑side** (Rust): harder to know the dynamic layout, but avoids IPC round‑trip on every state change.

Recommendation: **Frontend‑side**, because the buddy’s visual state (bubble open/closed, position within the envelope) is naturally expressed in the DOM. The IPC cost is negligible for state changes (a few times per second).

## The Minimal Working Example (Copy‑Paste Ready)

Take Kimi’s code as your baseline — it’s the most complete and includes the NVIDIA workaround, scale factor handling, and a clean React hook. The only missing piece is the window‑show order. Add this in your Tauri setup:

```rust
.setup(|app| {
    let window = app.get_webview_window("buddy").unwrap();
    window.show()?;                     // Must be visible before setting hitboxes
    window.set_position(tauri::Position::Physical(tauri::PhysicalPosition { x: 1720, y: 20 }))?;
    Ok(())
})
```

## Final Answer to Your Original Question

> How can we achieve shape‑shifting, interactive overlay buddies on Linux without ghosting or dead zones?

**Use a fixed‑size transparent window and GTK input shapes.**  
There is no other reliable way on WebKitGTK / Tauri v2. The roundtable’s consensus is unanimous, and the implementation is now concrete.

Your next step: build the one‑buddy prototype exactly as Kimi laid out, test on X11 and Wayland, and only then add personality, multiple agents, or speech bubbles. The core mechanic is solid.

If you hit a specific issue — like input shapes not applying, or hitboxes being offset on HiDPI — paste your Rust command and CSS; we’ll debug it line by line. You’re ready to ship.

