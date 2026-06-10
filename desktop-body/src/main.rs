//! Border Agents desktop presence body — animated body (build-order step 3).
//!
//! Step 2 proved a per-buddy surface on the wlr overlay layer with pixel-exact
//! anchor+margin placement, margin-update dragging, click-through input regions,
//! and chosen-output placement (see git history / README). Step 3 turns that
//! static sprite into an *animated body*: a time-driven face (idle bob, blink,
//! emotion-driven eyes/mouth), a real text speech bubble, and an expanding menu
//! card — all software-rendered (tiny-skia + fontdue), still no GTK/WebKit/GPU.
//!
//! The presentation state (emotion / speech / menu) is exposed through a small
//! internal API (`set_emotion`, `say`, `toggle_menu`) so step 4 can drive it from
//! presence-protocol events over the WebSocket. Until then, clicking the buddy
//! exercises it locally: click toggles the menu + a greeting; "Say hello" and
//! "Cycle mood" menu items change speech and emotion.
//!
//! Run:  cargo run --release            # active output, top-left
//!       BB_OUTPUT_INDEX=1 cargo run     # second monitor
//!       BB_MARGIN_LEFT=400 BB_MARGIN_TOP=200 cargo run
//! Drag the buddy head with the left button. Ctrl+C / close to exit.

mod presence;
mod render;

use std::time::{Duration, Instant};

use calloop::channel::Event as ChannelEvent;
use calloop::timer::{TimeoutAction, Timer};
use calloop::{EventLoop, LoopSignal};
use calloop_wayland_source::WaylandSource;
use render::{BodyView, Emotion, Sprite};
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
    Connection, Dispatch, Proxy, QueueHandle,
};
use wayland_protocols::wp::relative_pointer::zv1::client::{
    zwp_relative_pointer_manager_v1::ZwpRelativePointerManagerV1,
    zwp_relative_pointer_v1::{Event as RelativePointerEvent, ZwpRelativePointerV1},
};

const CLICK_SLOP: f64 = 5.0;

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
    // Raw pointer deltas for dragging — independent of the surface frame, so moving
    // the surface can't feed back into the motion we read. Optional: if absent we
    // simply can't drag (rather than dragging unstably).
    let relative_manager = globals
        .bind::<ZwpRelativePointerManagerV1, _, _>(&qh, 1..=1, ())
        .ok();
    if relative_manager.is_none() {
        eprintln!("[bb-desktop-body] wp_relative_pointer unavailable — dragging disabled");
    }

    let mut app = App {
        registry_state: RegistryState::new(&globals),
        seat_state: SeatState::new(&globals, &qh),
        output_state: OutputState::new(&globals, &qh),
        shm,
        pool: None,
        compositor,
        conn: conn.clone(),
        layer: None,
        pointer: None,
        relative_manager,
        relative_pointer: None,
        sprite: Sprite::new(),
        loop_signal: None,
        screen: None,
        width: render::SURFACE_W,
        height: render::SURFACE_H,
        margin_left: env_i32("BB_MARGIN_LEFT", 48) as f64,
        margin_top: env_i32("BB_MARGIN_TOP", 48) as f64,
        start: Instant::now(),
        emotion: Emotion::Neutral,
        speech: None,
        menu_open: false,
        configured: false,
        press: None,
        drag: false,
        exit: false,
    };

    // Let outputs (and their xdg-output logical size) arrive before we create the
    // surface. Two roundtrips: the first binds wl_output, the second delivers the
    // logical-size / mode events we clamp against.
    event_queue.roundtrip(&mut app).expect("initial roundtrip failed");
    event_queue.roundtrip(&mut app).expect("second roundtrip failed");

    let output = pick_output(&app);
    // Remember the target output's size so a drag can be clamped on-screen. Prefer
    // the xdg logical size; fall back to the current mode if logical size is absent.
    app.screen = output
        .clone()
        .or_else(|| app.output_state.outputs().next())
        .and_then(|o| app.output_state.info(&o))
        .and_then(|info| {
            info.logical_size
                .or_else(|| info.modes.iter().find(|m| m.current).map(|m| m.dimensions))
        })
        .map(|(w, h)| (w as f64, h as f64));
    eprintln!("[bb-desktop-body] screen bounds for clamping: {:?}", app.screen);
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
    layer.commit();

    app.pool = Some(
        SlotPool::new((app.width * app.height * 4) as usize, &app.shm).expect("slot pool failed"),
    );
    app.layer = Some(layer);

    eprintln!(
        "[bb-desktop-body] animated body up: {}x{} @ margin(L={}, T={}){} — drag the head; click for the menu",
        app.width,
        app.height,
        app.margin_left as i32,
        app.margin_top as i32,
        output_label(&app),
    );

    // calloop multiplexes Wayland input with a 30fps animation timer.
    let mut event_loop: EventLoop<App> = EventLoop::try_new().expect("event loop");
    let handle = event_loop.handle();
    app.loop_signal = Some(event_loop.get_signal());
    WaylandSource::new(conn, event_queue)
        .insert(handle.clone())
        .expect("insert wayland source");
    handle
        .insert_source(Timer::immediate(), |_deadline, _meta, app: &mut App| {
            app.tick();
            TimeoutAction::ToDuration(Duration::from_millis(33))
        })
        .expect("insert timer");

    // Presence: a soul (the dev gateway, or a real one) drives the body over a
    // WebSocket. The client runs on its own thread and forwards inbound frames here.
    // This commit only logs them; the next applies them to the body state.
    let (presence_tx, presence_rx) = calloop::channel::channel::<String>();
    handle
        .insert_source(presence_rx, |event, _meta, _app: &mut App| {
            if let ChannelEvent::Msg(text) = event {
                eprintln!("[bb-presence] inbound cue: {text}");
            }
        })
        .expect("insert presence source");
    presence::spawn(presence_tx);

    if let Err(err) = event_loop.run(Some(Duration::from_millis(33)), &mut app, |app| {
        if app.exit {
            if let Some(signal) = &app.loop_signal {
                signal.stop();
            }
        }
    }) {
        // The compositor dropping the connection is a normal way to exit, not a crash.
        eprintln!("[bb-desktop-body] event loop ended: {err}");
    }
}

struct PressState {
    target: PressTarget,
    /// Accumulated physical pointer travel since press — distinguishes click from drag.
    dist: f64,
}

#[derive(Clone, Copy, PartialEq)]
enum PressTarget {
    Head,
    MenuItem(usize),
    Outside,
}

struct App {
    registry_state: RegistryState,
    seat_state: SeatState,
    output_state: OutputState,
    shm: Shm,
    pool: Option<SlotPool>,
    compositor: CompositorState,
    conn: Connection,
    layer: Option<LayerSurface>,
    pointer: Option<wl_pointer::WlPointer>,
    relative_manager: Option<ZwpRelativePointerManagerV1>,
    relative_pointer: Option<ZwpRelativePointerV1>,
    sprite: Sprite,
    loop_signal: Option<LoopSignal>,
    screen: Option<(f64, f64)>,
    width: u32,
    height: u32,
    margin_left: f64,
    margin_top: f64,
    start: Instant,
    emotion: Emotion,
    speech: Option<String>,
    menu_open: bool,
    configured: bool,
    press: Option<PressState>,
    drag: bool,
    exit: bool,
}

fn pick_output(app: &App) -> Option<wl_output::WlOutput> {
    let index = env_i32("BB_OUTPUT_INDEX", -1);
    if index < 0 {
        return None;
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
    fn tick(&mut self) {
        self.draw();
        let _ = self.conn.flush();
    }

    fn draw(&mut self) {
        if !self.configured {
            return; // never attach a buffer before the first configure is acked
        }
        let (Some(pool), Some(layer)) = (self.pool.as_mut(), self.layer.as_ref()) else {
            return;
        };
        let (w, h) = (self.width, self.height);
        let stride = (w * 4) as i32;

        let view = BodyView {
            t: self.start.elapsed().as_secs_f32(),
            emotion: self.emotion,
            speech: self.speech.as_deref(),
            menu_open: self.menu_open,
        };

        let buffer = match pool.create_buffer(w as i32, h as i32, stride, wl_shm::Format::Argb8888) {
            Ok((buffer, canvas)) => {
                self.sprite.paint(canvas, w, h, &view);
                buffer
            }
            Err(err) => {
                eprintln!("[bb-desktop-body] buffer alloc failed: {err}");
                return;
            }
        };

        let surface = layer.wl_surface();
        buffer.attach_to(surface).expect("attach buffer");
        surface.damage_buffer(0, 0, w as i32, h as i32);
        surface.commit();
    }

    /// Reposition by rewriting anchor margins — a compositor texture move, never a
    /// repaint, so ghosting cannot occur. Driven by raw `wp_relative_pointer` deltas
    /// (physical pointer motion), so moving the surface never feeds back into the
    /// motion we read — the runaway that surface-local coordinates caused.
    fn reposition(&mut self) {
        if let Some(layer) = self.layer.as_ref() {
            // Margins may be negative — that tucks the (mostly-transparent) surface
            // partly off an edge while keeping the buddy itself on-screen.
            layer.set_margin(self.margin_top as i32, 0, 0, self.margin_left as i32);
            layer.commit();
        }
    }

    /// Keep the buddy *head* fully on-screen — not just the surface edge — so it can
    /// never slide off (the head sits centered inside a wider, mostly-transparent
    /// surface). Margins are allowed to go negative to reach the left/top edges.
    fn clamp_margins(&mut self) {
        let head = render::head_rect();
        let (sw, sh) = self.screen.unwrap_or((f64::MAX, f64::MAX));
        let min_left = -(head.x as f64);
        let max_left = (sw - (head.x + head.w) as f64).max(min_left);
        let min_top = -(head.y as f64);
        let max_top = (sh - (head.y + head.h) as f64).max(min_top);
        self.margin_left = self.margin_left.clamp(min_left, max_left);
        self.margin_top = self.margin_top.clamp(min_top, max_top);
    }

    /// Input region = only the parts that should catch the pointer; everywhere else
    /// is transparent AND click-through. Recomputed whenever the menu/bubble toggles.
    fn update_input_region(&mut self) {
        let Some(layer) = self.layer.as_ref() else { return };
        let Ok(region) = Region::new(&self.compositor) else { return };

        let mut rects = vec![render::head_rect().as_i32()];
        if self.speech.is_some() {
            rects.push(render::BUBBLE.as_i32());
        }
        if self.menu_open {
            rects.push(render::MENU.as_i32());
        }
        for (x, y, w, h) in rects {
            region.add(x, y, w, h);
        }
        layer.wl_surface().set_input_region(Some(region.wl_region()));
        layer.commit();
    }

    // --- presentation API (step 4 will call these from presence events) ---

    fn set_emotion(&mut self, emotion: Emotion) {
        self.emotion = emotion;
    }

    fn say(&mut self, text: impl Into<String>) {
        self.speech = Some(text.into());
        self.update_input_region();
    }

    fn toggle_menu(&mut self) {
        self.menu_open = !self.menu_open;
        if self.menu_open {
            self.speech = Some("Hey — I'm Hermes, living on your desktop now.".to_string());
        } else {
            self.speech = None;
        }
        self.update_input_region();
    }

    fn activate_menu_item(&mut self, index: usize) {
        match index {
            0 => {
                self.set_emotion(Emotion::Happy);
                self.say("Hello there! Good to finally stand on the desktop.");
            }
            1 => {
                let next = next_emotion(self.emotion);
                self.set_emotion(next);
                self.say(format!("Mood: {}", emotion_name(next)));
            }
            _ => {}
        }
    }

    fn on_press(&mut self, x: f64, y: f64) {
        let target = if render::point_in_head(x, y) {
            PressTarget::Head
        } else if self.menu_open {
            let mut hit = PressTarget::Outside;
            for i in 0..render::MENU_ITEMS.len() {
                if render::menu_item_rect(i).contains(x, y) {
                    hit = PressTarget::MenuItem(i);
                    break;
                }
            }
            hit
        } else {
            PressTarget::Outside
        };

        self.press = Some(PressState { target, dist: 0.0 });
        if target == PressTarget::Head {
            self.drag = true;
        }
    }

    /// Physical pointer delta from `wp_relative_pointer`. Apply it straight to the
    /// margins while dragging — no surface-frame feedback, no runaway.
    fn on_drag_delta(&mut self, dx: f64, dy: f64) {
        let Some(press) = self.press.as_mut() else { return };
        press.dist += dx.abs() + dy.abs();
        if self.drag {
            self.margin_left += dx;
            self.margin_top += dy;
            self.clamp_margins();
            self.reposition();
        }
    }

    fn on_release(&mut self) {
        let Some(press) = self.press.take() else { return };
        self.drag = false;
        if press.dist > CLICK_SLOP {
            return; // it was a drag, not a click
        }
        match press.target {
            PressTarget::Head => self.toggle_menu(),
            PressTarget::MenuItem(i) => self.activate_menu_item(i),
            PressTarget::Outside => {}
        }
    }
}

fn next_emotion(current: Emotion) -> Emotion {
    let cycle = Emotion::CYCLE;
    let idx = cycle.iter().position(|e| *e == current).unwrap_or(0);
    cycle[(idx + 1) % cycle.len()]
}

fn emotion_name(emotion: Emotion) -> &'static str {
    match emotion {
        Emotion::Neutral => "neutral",
        Emotion::Happy => "happy",
        Emotion::Thinking => "thinking",
        Emotion::Curious => "curious",
        Emotion::Alert => "alert",
        Emotion::Sleepy => "sleepy",
    }
}

impl CompositorHandler for App {
    fn scale_factor_changed(&mut self, _c: &Connection, _q: &QueueHandle<Self>, _s: &wl_surface::WlSurface, _n: i32) {}
    fn transform_changed(&mut self, _c: &Connection, _q: &QueueHandle<Self>, _s: &wl_surface::WlSurface, _t: wl_output::Transform) {}
    fn frame(&mut self, _c: &Connection, _q: &QueueHandle<Self>, _s: &wl_surface::WlSurface, _time: u32) {}
    fn surface_enter(&mut self, _c: &Connection, _q: &QueueHandle<Self>, _s: &wl_surface::WlSurface, _o: &wl_output::WlOutput) {}
    fn surface_leave(&mut self, _c: &Connection, _q: &QueueHandle<Self>, _s: &wl_surface::WlSurface, _o: &wl_output::WlOutput) {}
}

impl LayerShellHandler for App {
    fn closed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _layer: &LayerSurface) {
        self.exit = true;
        if let Some(signal) = &self.loop_signal {
            signal.stop();
        }
    }

    fn configure(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _serial: u32,
    ) {
        let (nw, nh) = configure.new_size;
        if nw != 0 && nh != 0 && (nw != self.width || nh != self.height) {
            eprintln!("[bb-desktop-body] surface resized by compositor to {nw}x{nh}");
        }
        if nw != 0 {
            self.width = nw;
        }
        if nh != 0 {
            self.height = nh;
        }
        let first = !self.configured;
        self.configured = true;
        if first {
            self.update_input_region();
        }
        self.draw();
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
                Ok(pointer) => {
                    if let Some(manager) = &self.relative_manager {
                        self.relative_pointer =
                            Some(manager.get_relative_pointer(&pointer, qh, ()));
                    }
                    self.pointer = Some(pointer);
                }
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
        let Some(surface) = self.layer.as_ref().map(|l| l.wl_surface().clone()) else {
            return;
        };
        for event in events {
            if event.surface != surface {
                continue;
            }
            let (px, py) = event.position;
            match event.kind {
                PointerEventKind::Press { .. } => self.on_press(px, py),
                PointerEventKind::Release { .. } => self.on_release(),
                // Dragging is driven by wp_relative_pointer deltas, not these
                // surface-local positions (which move with the surface frame).
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

impl Dispatch<ZwpRelativePointerManagerV1, ()> for App {
    fn event(
        _state: &mut Self,
        _proxy: &ZwpRelativePointerManagerV1,
        _event: <ZwpRelativePointerManagerV1 as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwpRelativePointerV1, ()> for App {
    fn event(
        state: &mut Self,
        _proxy: &ZwpRelativePointerV1,
        event: <ZwpRelativePointerV1 as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        if let RelativePointerEvent::RelativeMotion { dx, dy, .. } = event {
            state.on_drag_delta(dx, dy);
        }
    }
}

delegate_compositor!(App);
delegate_output!(App);
delegate_shm!(App);
delegate_seat!(App);
delegate_pointer!(App);
delegate_layer!(App);
delegate_registry!(App);
