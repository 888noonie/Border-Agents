# bb-desktop-body — native desktop presence body

Build-order **steps 2–3** from [docs/OVERLAY_POSTMORTEM_AND_REBUILD_PLAN.md](../docs/OVERLAY_POSTMORTEM_AND_REBUILD_PLAN.md).

A small, standalone Rust binary that de-risks the whole desktop-body plan by proving,
on the real COSMIC/Wayland/NVIDIA machine, the capabilities the dead transparent-WebKitGTK
stack could never deliver. It talks Wayland directly via `smithay-client-toolkit` and renders
with software `wl_shm` (tiny-skia + fontdue) — **no GTK, no WebKit, no GPU**.

- **Step 2 (spike):** a static sprite on the overlay layer — placement, drag, click-through, output.
- **Step 3 (animated body):** a time-driven face (idle bob, blink, emotion-driven eyes/mouth),
  a real text **speech bubble**, and an expanding **menu card**. A `calloop` 30fps timer drives
  animation alongside Wayland input. The presentation state (`set_emotion` / `say` / `toggle_menu`)
  is the seam step 4 wires to presence-protocol events over the WebSocket.

## What it proves

| # | Capability | Why it matters |
|---|---|---|
| 1 | Surface on the wlr **overlay layer** | Floats above all normal windows, any screen |
| 2 | **Pixel-exact** anchor + margin placement | Solves Symptom B — native toplevels couldn't position themselves |
| 3 | **Drag = margin update** | A compositor texture move, not a window move |
| 4 | **Click-through** via input region | Clicks outside the buddy silhouette pass through |
| 5 | **Chosen-output** placement | Second-monitor support via `BB_OUTPUT_INDEX` |

Ghosting (Symptom A) is **structurally impossible**: a per-buddy surface that moves has no
vacated pixels to repaint — the compositor just relocates its texture.

## Run

```bash
cargo run --release                       # active output, top-left
BB_OUTPUT_INDEX=1 cargo run               # second monitor
BB_MARGIN_LEFT=400 BB_MARGIN_TOP=200 cargo run
```

**Drag** the blue buddy head with the left button. **Click** the head to open the menu +
greeting bubble; the menu items change the buddy's speech and cycle its mood. Clicks in the
transparent areas fall through to the window beneath. Ctrl+C (or close from the compositor) to exit.

## What to look for (manual verification)

- The buddy appears **above** other windows and stays there, gently bobbing and blinking.
- Dragging moves it smoothly and leaves **no trails** behind (the whole point).
- Clicking opens a **speech bubble with readable text** and a **menu card**; "Cycle mood"
  visibly changes the eyes/mouth.
- It can be placed on a second monitor with `BB_OUTPUT_INDEX`.

## Next (step 4 — wire the soul)

Add a presence-protocol WebSocket client: feed inbound `express` / `say` / `move_to` / `hydrate`
events into `set_emotion` / `say` / margin updates, and emit `clicked` / `grabbed` / `dragged` /
`dropped` / `summoned` / `dismissed` back to the gateway — the same protocol the browser body
already speaks (`src/presenceProtocol.ts`).
