# bb-desktop-body — native desktop presence body

Grew out of build-order **steps 2–3** from [docs/OVERLAY_POSTMORTEM_AND_REBUILD_PLAN.md](../docs/OVERLAY_POSTMORTEM_AND_REBUILD_PLAN.md);
it is now the real **soul-driven presence body**, not just a spike.

A standalone Rust binary that talks Wayland directly via `smithay-client-toolkit` and renders
with software `wl_shm` (tiny-skia + fontdue) — **no GTK, no WebKit, no GPU**. A `calloop` 30fps
timer drives animation alongside Wayland input; a `tungstenite` WebSocket thread connects to a
soul over the presence protocol (`src/presenceProtocol.ts`), the same one the browser body speaks.

## What it is now

- **Animated clay figure** — head, torso, arms, legs, feet; idle bob, blink, emotion-driven
  eyes/mouth. A real text **speech bubble** and a chat input.
- **Soul-driven** — applies inbound `express` / `say` / `move_to` / `hydrate` / `surface_active` /
  `output` / `action_result` / `targets_available` cues; emits `attached` / `clicked` / `grabbed` /
  `dropped` / `said` / `surface_request` / `action_request` back. The body presents and reports;
  it never decides authority (AGENTS.md law 7).
- **Surface dial** — hold the torso to bloom the surface switcher (two columns flanking the torso
  so the torso stays readable); availability is soul-pushed (`available` / `unwired` / `gated`).
- **Right-click commandeer picker** — a two-phase dial (window → Pin/Monitor/Control). P/M/C route
  through the soul-gated `commandeer` act effector via an `ActionIntent`; only Unpin of the
  already-pinned window is local. The body renders the picker; the soul authorizes and the
  `frame_driver` acts.
- **Receipt rail** — at full body length, a left-edge rail of recent `action_result` receipts.
- **Body-local settings panel** — reached from the dial's Customize entry. Colour and Size are
  editable presentation (cycle on tap); Posture and Buddy are read-only, reflecting soul/launch
  state only — the visible posture can never lie about the soul's actual authorization posture.

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

The body runs standalone (puppeting itself) if no soul is present, and connects when one appears.
For the full governed experience, run a soul too:

```bash
# Terminal 1 — the real soul (runs the action gate): ws://127.0.0.1:17387/border-buddies
npm run soul:dev

# Terminal 2 — the body. Use the SAME buddy id the soul grants effectors to.
BB_BUDDY=forge cargo run --release        # commandeer is granted to Forge
BB_OUTPUT_INDEX=1 cargo run               # second monitor
BB_MARGIN_LEFT=400 BB_MARGIN_TOP=200 cargo run

# COSMIC screen-commandeer driver (enumerate + activate + type), driven by the soul on allow:
cargo run --bin frame_driver
```

Useful env: `BB_BUDDY` (wire id, default `hermes`), `BB_PRESENCE_URL`, `BB_OUTPUT_INDEX`,
`BB_MARGIN_LEFT/TOP`, `BB_COLOR` (clay colour, also cycleable in the settings panel),
`<BUDDY>_NAME` / `<BUDDY>_PROVIDER` / `<BUDDY>_MODEL` per-buddy labels.

**Drag** the figure (whole body is a move handle). **Click** the head to open chat. **Hold** the
torso to bloom the surface dial; **right-click** to open the commandeer picker. Drag the **feet**
to resize. The dial's **Customize** entry opens the settings panel. Clicks in transparent areas
fall through to the window beneath. Ctrl+C (or close from the compositor) to exit.

## What to look for (manual verification)

- The buddy appears **above** other windows and stays there, gently bobbing and blinking.
- Dragging moves it smoothly and leaves **no trails** behind (the whole point).
- Clicking opens a **speech bubble with readable text** and a **menu card**; "Cycle mood"
  visibly changes the eyes/mouth.
- It can be placed on a second monitor with `BB_OUTPUT_INDEX`.

## Next

- **Wizard onboarding host** (`docs/WIZARD_ONBOARDING_SCRIPT.md`) — drive the body through
  Act 0+ from the soul side.
- **Governance vertical slice** — make a buddy action grade memory and emit a `GradeReceipt`,
  joining the presence body to the governance core.
- Posture remains read-only in the settings panel; making it editable is a soul-side
  request→echo follow-up (the body must reflect, never decide, posture — law 7).
