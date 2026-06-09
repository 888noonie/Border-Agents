# bb-desktop-body — wlr-layer-shell spike

Build-order **step 2** from [docs/OVERLAY_POSTMORTEM_AND_REBUILD_PLAN.md](../docs/OVERLAY_POSTMORTEM_AND_REBUILD_PLAN.md).

A small, standalone Rust binary that de-risks the whole desktop-body plan by proving,
on the real COSMIC/Wayland/NVIDIA machine, the capabilities the dead transparent-WebKitGTK
stack could never deliver. It talks Wayland directly via `smithay-client-toolkit` and renders
with software `wl_shm` — **no GTK, no WebKit, no GPU**.

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

Drag the blue buddy head with the **left mouse button**. Clicks in the transparent lower
region of the surface should fall through to the window beneath. Ctrl+C (or close from the
compositor) to exit.

## What to look for (manual verification)

- The buddy appears **above** other windows and stays there.
- Dragging moves it smoothly and leaves **no trails** behind (the whole point).
- It can be placed on a second monitor with `BB_OUTPUT_INDEX`.

## Next (step 3)

Replace the static `paint_buddy` sprite with the animated body — sprite/Rive animation,
speech bubble, expanding menu card — and wire it to the presence protocol (`src/presenceProtocol.ts`)
so the soul drives it over the WebSocket.
