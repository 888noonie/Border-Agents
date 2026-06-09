//! Border Agents desktop presence body — wlr-layer-shell spike.
//!
//! Build-order step 2 from docs/OVERLAY_POSTMORTEM_AND_REBUILD_PLAN.md. This is a
//! deliberately small, standalone binary that de-risks the entire desktop plan by
//! proving — on the real COSMIC/Wayland/NVIDIA machine — the things the dead
//! transparent-WebKitGTK stack could never do:
//!
//!   1. A surface on the wlr **overlay layer**, above all normal windows.
//!   2. **Pixel-exact placement** via anchor + margin — the capability native
//!      Wayland denies to normal toplevels (this was Symptom B: per-buddy windows
//!      all stacked at the compositor default and ignored set_position).
//!   3. **Dragging as margin updates** (a compositor texture move), not a native
//!      window move.
//!   4. **Click-through** via a per-pixel input region: clicks outside the buddy
//!      silhouette pass through to whatever is underneath.
//!   5. **Chosen-output placement** (second monitor) via BB_OUTPUT_INDEX.
//!
//! Ghosting (Symptom A) is structurally impossible here: a per-buddy surface that
//! moves has no vacated pixels to repaint — the compositor just relocates its
//! texture. Rendering is plain software `wl_shm` (no GTK, no WebKit, no GPU); the
//! animated/skeletal renderer is step 3, this only needs a static sprite.
//!
//! Run:  cargo run --release          # primary output, top-left
//!       BB_OUTPUT_INDEX=1 cargo run   # second monitor
//!       BB_MARGIN_LEFT=400 BB_MARGIN_TOP=200 cargo run
//! Drag the buddy with the left mouse button. Ctrl+C (or close from the
//! compositor) to exit.

use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState, Region},
    delegate_compositor, delegate_layer, delegate_output, delegate_pointer, delegate_registry,
    delegate_seat, delegate_shm,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{
        pointer::{PointerEvent, PointerEventKind, PointerHandler},
        Capability, SeatHandler, SeatState,
    },
    shell::{
        wlr_layer::{
            Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
            LayerSurfaceConfigure,
        },
        WaylandSurface,
    },
    shm::{slot::SlotPool, Shm, ShmHandler},
};
use smithay_client_toolkit::reexports::client::{
    globals::registry_queue_init,
    protocol::{wl_output, wl_pointer, wl_seat, wl_shm, wl_surface},
    Connection, QueueHandle,
};

const SURFACE_WIDTH: u32 = 200;
const SURFACE_HEIGHT: u32 = 240;
// The buddy silhouette sits in the upper part of the surface; the rest is a
// transparent, click-through margin used later for the speech bubble / menu.
const HEAD_CX: f64 = SURFACE_WIDTH as f64 / 2.0;
const HEAD_CY: f64 = 86.0;
const HEAD_R: f64 = 72.0;

fn env_i32(key: &str, fallback: i32) -> i32 {
    std::env::var(key).ok().and_then(|v| v.trim().parse().ok()).unwrap_or(fallback)
}

fn main() {
    let conn = Connection::connect_to_env()
        .expect("could not connect to a Wayland compositor (is WAYLAND_DISPLAY set?)");
    let (globals, mut event_queue) = registry_queue_init(&conn).expect("registry init failed");
    let qh = event_queue.handle();

    let compositor = CompositorState::bind(&globals, &qh).expect("wl_compositor unavailable");
    let layer_shell = LayerShell::bind(&globals, &qh)
        .expect("wlr-layer-shell unavailable — compositor does not implement zwlr_layer_shell_v1");
    let shm = Shm::bind(&globals, &qh).expect("wl_shm unavailable");

    let mut app = App {
        registry_state: RegistryState::new(&globals),
        seat_state: SeatState::new(&globals, &qh),
        output_state: OutputState::new(&globals, &qh),
        shm,
        pool: None,
        compositor,
        layer: None,
        pointer: None,
        width: SURFACE_WIDTH,
        height: SURFACE_HEIGHT,
        margin_left: env_i32("BB_MARGIN_LEFT", 48) as f64,
        margin_top: env_i32("BB_MARGIN_TOP", 48) as f64,
        drag: None,
        first_configure: true,
        exit: false,
    };

    // Let outputs arrive so we can honor BB_OUTPUT_INDEX before creating the surface.
    event_queue.roundtrip(&mut app).expect("initial roundtrip failed");

    let output = pick_output(&app);
    let surface = app.compositor.create_surface(&qh);
    let layer = layer_shell.create_layer_surface(
        &qh,
        surface,
        Layer::Overlay,
        Some("bb-buddy-hermes"),
        output.as_ref(),
    );
    layer.set_anchor(Anchor::TOP | Anchor::LEFT);
    layer.set_size(app.width, app.height);
    layer.set_margin(app.margin_top as i32, 0, 0, app.margin_left as i32);
    layer.set_keyboard_interactivity(KeyboardInteractivity::None);

    // Input region = the buddy silhouette only. Everything else in the surface is
    // transparent AND click-through: pointer events there pass to the window below.
    if let Ok(region) = Region::new(&app.compositor) {
        region.add(
            (HEAD_CX - HEAD_R) as i32,
            (HEAD_CY - HEAD_R) as i32,
            (HEAD_R * 2.0) as i32,
            (HEAD_R * 2.0) as i32,
        );
        layer.wl_surface().set_input_region(Some(region.wl_region()));
    }

    layer.commit();

    app.pool = Some(
        SlotPool::new((app.width * app.height * 4) as usize, &app.shm).expect("slot pool failed"),
    );
    app.layer = Some(layer);

    eprintln!(
        "[bb-desktop-body] overlay layer surface up: {}x{} @ margin(L={}, T={}){} — drag the buddy with the left button",
        app.width,
        app.height,
        app.margin_left as i32,
        app.margin_top as i32,
        output_label(&app),
    );

    while !app.exit {
        event_queue.blocking_dispatch(&mut app).expect("dispatch failed");
    }
}

struct DragState {
    /// Pointer position within the surface at grab time. Held constant; see draw note.
    grab_x: f64,
    grab_y: f64,
}

struct App {
    registry_state: RegistryState,
    seat_state: SeatState,
    output_state: OutputState,
    shm: Shm,
    pool: Option<SlotPool>,
    compositor: CompositorState,
    layer: Option<LayerSurface>,
    pointer: Option<wl_pointer::WlPointer>,
    width: u32,
    height: u32,
    margin_left: f64,
    margin_top: f64,
    drag: Option<DragState>,
    first_configure: bool,
    exit: bool,
}

fn pick_output(app: &App) -> Option<wl_output::WlOutput> {
    let index = env_i32("BB_OUTPUT_INDEX", -1);
    if index < 0 {
        return None; // let the compositor choose the active output
    }
    app.output_state.outputs().nth(index as usize)
}

fn output_label(app: &App) -> String {
    let index = env_i32("BB_OUTPUT_INDEX", -1);
    if index < 0 {
        return " on the active output".to_string();
    }
    match app.output_state.outputs().nth(index as usize) {
        Some(output) => match app.output_state.info(&output).and_then(|i| i.name) {
            Some(name) => format!(" on output #{index} ({name})"),
            None => format!(" on output #{index}"),
        },
        None => format!(" (output #{index} not found — using active output)"),
    }
}

impl App {
    fn draw(&mut self, qh: &QueueHandle<Self>) {
        let (Some(pool), Some(layer)) = (self.pool.as_mut(), self.layer.as_ref()) else {
            return;
        };
        let (w, h) = (self.width, self.height);
        let stride = (w * 4) as i32;

        let buffer = match pool.create_buffer(w as i32, h as i32, stride, wl_shm::Format::Argb8888) {
            Ok((buffer, canvas)) => {
                paint_buddy(canvas, w, h);
                buffer
            }
            Err(err) => {
                eprintln!("[bb-desktop-body] buffer alloc failed: {err}");
                return;
            }
        };

        let surface = layer.wl_surface();
        buffer
            .attach_to(surface)
            .expect("failed to attach buffer to surface");
        surface.damage_buffer(0, 0, w as i32, h as i32);
        surface.commit();
        let _ = qh; // no per-frame callback needed: the sprite is static
    }

    /// Reposition by rewriting anchor margins. This is the whole point of the
    /// spike: moving a buddy is a margin change (a compositor texture relocation),
    /// never a buffer repaint — so the ghosting that killed the unified webview
    /// cannot occur. We add (pointer - grab) using the *live* margin each event,
    /// which tracks the cursor because the surface frame moves under the pointer
    /// after each commit (see the on-Motion math).
    fn reposition(&mut self) {
        if let Some(layer) = self.layer.as_ref() {
            let left = self.margin_left.max(0.0) as i32;
            let top = self.margin_top.max(0.0) as i32;
            layer.set_margin(top, 0, 0, left);
            layer.commit();
        }
    }
}

/// Paint a static buddy silhouette into a premultiplied-ARGB8888 `wl_shm` canvas.
/// Transparent background (alpha 0) everywhere outside the head.
fn paint_buddy(canvas: &mut [u8], w: u32, h: u32) {
    for pixel in canvas.chunks_exact_mut(4) {
        pixel.copy_from_slice(&[0, 0, 0, 0]);
    }

    let put = |canvas: &mut [u8], x: i32, y: i32, rgba: [u8; 4]| {
        if x < 0 || y < 0 || x as u32 >= w || y as u32 >= h {
            return;
        }
        let idx = ((y as u32 * w + x as u32) * 4) as usize;
        // wl_shm Argb8888 byte order is little-endian B, G, R, A; alpha is premultiplied.
        canvas[idx] = rgba[2];
        canvas[idx + 1] = rgba[1];
        canvas[idx + 2] = rgba[0];
        canvas[idx + 3] = rgba[3];
    };

    // Head — filled disc, Hermes blue.
    let r2 = HEAD_R * HEAD_R;
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            let dx = x as f64 + 0.5 - HEAD_CX;
            let dy = y as f64 + 0.5 - HEAD_CY;
            if dx * dx + dy * dy <= r2 {
                put(canvas, x, y, [0x2f, 0x7d, 0xff, 0xff]);
            }
        }
    }

    // Eyes — two cyan discs.
    let eye = |canvas: &mut [u8], cx: f64, cy: f64, rad: f64| {
        let rr = rad * rad;
        for y in 0..h as i32 {
            for x in 0..w as i32 {
                let dx = x as f64 + 0.5 - cx;
                let dy = y as f64 + 0.5 - cy;
                if dx * dx + dy * dy <= rr {
                    put(canvas, x, y, [0x7d, 0xf9, 0xff, 0xff]);
                }
            }
        }
    };
    eye(canvas, HEAD_CX - 24.0, HEAD_CY - 6.0, 13.0);
    eye(canvas, HEAD_CX + 24.0, HEAD_CY - 6.0, 13.0);
}

impl CompositorHandler for App {
    fn scale_factor_changed(&mut self, _c: &Connection, _q: &QueueHandle<Self>, _s: &wl_surface::WlSurface, _new: i32) {}
    fn transform_changed(&mut self, _c: &Connection, _q: &QueueHandle<Self>, _s: &wl_surface::WlSurface, _t: wl_output::Transform) {}
    fn frame(&mut self, _c: &Connection, qh: &QueueHandle<Self>, _s: &wl_surface::WlSurface, _time: u32) {
        self.draw(qh);
    }
    fn surface_enter(&mut self, _c: &Connection, _q: &QueueHandle<Self>, _s: &wl_surface::WlSurface, _o: &wl_output::WlOutput) {}
    fn surface_leave(&mut self, _c: &Connection, _q: &QueueHandle<Self>, _s: &wl_surface::WlSurface, _o: &wl_output::WlOutput) {}
}

impl LayerShellHandler for App {
    fn closed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _layer: &LayerSurface) {
        self.exit = true;
    }

    fn configure(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        _layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _serial: u32,
    ) {
        if configure.new_size.0 != 0 {
            self.width = configure.new_size.0;
        }
        if configure.new_size.1 != 0 {
            self.height = configure.new_size.1;
        }
        if self.first_configure {
            self.first_configure = false;
        }
        self.draw(qh);
    }
}

impl SeatHandler for App {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }
    fn new_seat(&mut self, _c: &Connection, _q: &QueueHandle<Self>, _s: wl_seat::WlSeat) {}
    fn new_capability(&mut self, _c: &Connection, qh: &QueueHandle<Self>, seat: wl_seat::WlSeat, capability: Capability) {
        if capability == Capability::Pointer && self.pointer.is_none() {
            match self.seat_state.get_pointer(qh, &seat) {
                Ok(pointer) => self.pointer = Some(pointer),
                Err(err) => eprintln!("[bb-desktop-body] could not get pointer: {err}"),
            }
        }
    }
    fn remove_capability(&mut self, _c: &Connection, _q: &QueueHandle<Self>, _s: wl_seat::WlSeat, capability: Capability) {
        if capability == Capability::Pointer {
            if let Some(pointer) = self.pointer.take() {
                pointer.release();
            }
        }
    }
    fn remove_seat(&mut self, _c: &Connection, _q: &QueueHandle<Self>, _s: wl_seat::WlSeat) {}
}

impl PointerHandler for App {
    fn pointer_frame(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _pointer: &wl_pointer::WlPointer,
        events: &[PointerEvent],
    ) {
        let on_surface = self
            .layer
            .as_ref()
            .map(|l| l.wl_surface().clone());
        let Some(surface) = on_surface else { return };

        for event in events {
            if &event.surface != &surface {
                continue;
            }
            let (px, py) = event.position;
            match event.kind {
                PointerEventKind::Press { .. } => {
                    self.drag = Some(DragState { grab_x: px, grab_y: py });
                }
                PointerEventKind::Release { .. } => {
                    self.drag = None;
                }
                PointerEventKind::Motion { .. } => {
                    if let Some(drag) = self.drag.as_ref() {
                        // Live-margin tracking: add (pointer - grab) to the current
                        // margin. The surface frame shifts under the pointer after
                        // each commit, so the next event's local position already
                        // reflects the move — this tracks the cursor without drift.
                        self.margin_left += px - drag.grab_x;
                        self.margin_top += py - drag.grab_y;
                        self.reposition();
                    }
                }
                _ => {}
            }
        }
    }
}

impl ShmHandler for App {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl OutputHandler for App {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }
    fn new_output(&mut self, _c: &Connection, _q: &QueueHandle<Self>, _o: wl_output::WlOutput) {}
    fn update_output(&mut self, _c: &Connection, _q: &QueueHandle<Self>, _o: wl_output::WlOutput) {}
    fn output_destroyed(&mut self, _c: &Connection, _q: &QueueHandle<Self>, _o: wl_output::WlOutput) {}
}

impl ProvidesRegistryState for App {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![OutputState, SeatState];
}

delegate_compositor!(App);
delegate_output!(App);
delegate_shm!(App);
delegate_seat!(App);
delegate_pointer!(App);
delegate_layer!(App);
delegate_registry!(App);
