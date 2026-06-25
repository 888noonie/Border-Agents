//! Border Agents desktop presence body — animated body (build-order step 3).
//!
//! Step 2 proved a per-buddy surface on the wlr overlay layer with pixel-exact
//! anchor+margin placement, margin-update dragging, click-through input regions,
//! and chosen-output placement (see git history / README). Step 3+ turns that
//! static sprite into an *animated clay figure* (a Morph-style buddy: head as the
//! dock/drag handle, slender stretchable torso, speech bubble + chat input that
//! auto-face inward) — all software-rendered (tiny-skia + fontdue), still no
//! GTK/WebKit/GPU.
//!
//! The presentation state (emotion / speech / chat) is exposed through a small
//! internal API (`set_emotion`, `say`, `toggle_chat`) driven by presence-protocol
//! events over the WebSocket. Mood belongs to the soul (`express` cues), never
//! to a local button.
//!
//! Run:  cargo run --release            # active output, top-left
//!       BB_OUTPUT_INDEX=1 cargo run     # second monitor
//!       BB_MARGIN_LEFT=400 BB_MARGIN_TOP=200 cargo run
//!       BB_COLOR="#7c5cff" cargo run    # recolour the clay
//! Drag the body to move/dock; click the head to chat; drag the feet to stretch.

mod presence;
mod render;

use std::{
    collections::VecDeque,
    io::Write,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

use calloop::channel::Event as ChannelEvent;
use calloop::timer::{TimeoutAction, Timer};
use calloop::{EventLoop, LoopSignal};
use calloop_wayland_source::WaylandSource;
use render::{BodyView, BumpEdge, Emotion, Facing, PerimeterId, Sprite, TorsoAction};
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState, Region},
    delegate_compositor, delegate_keyboard, delegate_layer, delegate_output, delegate_pointer,
    delegate_registry, delegate_seat, delegate_shm,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{
        keyboard::{KeyEvent, KeyboardHandler, Keysym, Modifiers},
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
    protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_shm, wl_surface},
    Connection, Dispatch, Proxy, QueueHandle,
};
use wayland_protocols::wp::relative_pointer::zv1::client::{
    zwp_relative_pointer_manager_v1::ZwpRelativePointerManagerV1,
    zwp_relative_pointer_v1::{Event as RelativePointerEvent, ZwpRelativePointerV1},
};

const CLICK_SLOP: f64 = 5.0;
const SURFACE_BLOOM_HOLD: Duration = Duration::from_millis(250);
const BTN_LEFT: u32 = 0x110;
const BTN_RIGHT: u32 = 0x111;
const INPUT_PASTE_MAX_CHARS: usize = 8_000;
/// Pre-hydrate fallback seed only. The authoritative ordered surface list (with per-surface
/// availability) arrives soul-pushed on `hydrate` and supersedes this — see `ordered_surfaces`.
/// Kept so the perimeter still cycles on a fresh body before the first hydrate lands.
const SURFACE_ORDER: &[&str] = &["session", "private_local_chat", "claude_code", "live_hermes", "agent_zero", "customize"];
const SURFACE_QUICK: &[&str] = &["session", "private_local_chat", "claude_code", "agent_zero"];
const RECEIPT_RAIL_CAP: usize = 20;

/// Drop the buddy with its head within this many pixels of a screen edge and it tucks
/// against that edge.
const TUCK_THRESHOLD: f64 = 40.0;

/// Index of the next *cyclable* surface in `order` from `start`, walking by `delta` and
/// skipping `unwired` entries so the arrows never dead-end on a surface that can't activate.
/// `None` when `order` is empty or every surface is unwired. Pure so it is unit-testable
/// without a live `App`.
fn next_cyclable_index(order: &[presence::SurfaceDescriptor], start: usize, delta: isize) -> Option<usize> {
    let len = order.len() as isize;
    if len == 0 {
        return None;
    }
    let start = start as isize;
    for step in 1..=len {
        let idx = (start + delta * step).rem_euclid(len) as usize;
        // Skip unwired surfaces (can't activate) and launchers (those open a tool, not a
        // surface — reachable only via the bloom dial, never the arrow cycle).
        if order[idx].availability != "unwired" && !order[idx].is_launcher() {
            return Some(idx);
        }
    }
    None
}

fn rotate_surfaces_for_bloom(order: Vec<presence::SurfaceDescriptor>, active_surface: &str) -> Vec<presence::SurfaceDescriptor> {
    if order.is_empty() {
        return order;
    }
    let start = order.iter().position(|s| s.id == active_surface).unwrap_or(0);
    (0..order.len()).map(|offset| order[(start + offset) % order.len()].clone()).collect()
}

fn env_i32(key: &str, fallback: i32) -> i32 {
    std::env::var(key).ok().and_then(|v| v.trim().parse().ok()).unwrap_or(fallback)
}

/// Per-frame trace logging (every compositor resize, every inbound cue) is gated behind
/// `BB_DEBUG` — it floods stderr during drags/animation, which is just noise in normal
/// use (and enough volume to get a supervised process killed). Off by default.
fn debug_log_enabled() -> bool {
    use std::sync::OnceLock;
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("BB_DEBUG").map(|v| !v.is_empty() && v != "0").unwrap_or(false)
    })
}

/// Parse `BB_COLOR` ("#C96D3C" or "C96D3C") into the clay colour; falls back to
/// the Morph terracotta default. Easy per-buddy recolouring without a rebuild.
fn env_color(key: &str) -> [u8; 3] {
    let Some(raw) = std::env::var(key).ok() else { return render::CLAY_DEFAULT };
    let hex = raw.trim().trim_start_matches('#');
    if hex.len() != 6 {
        return render::CLAY_DEFAULT;
    }
    match u32::from_str_radix(hex, 16) {
        Ok(v) => [(v >> 16) as u8, (v >> 8) as u8, v as u8],
        Err(_) => render::CLAY_DEFAULT,
    }
}

fn buddy_env_key(buddy: &str, suffix: &str) -> String {
    format!("{}_{}", buddy.trim().to_ascii_uppercase().replace('-', "_"), suffix)
}

fn buddy_env(buddy: &str, suffix: &str) -> Option<String> {
    std::env::var(buddy_env_key(buddy, suffix)).ok().map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

/// Pure margin clamp for the figure bounding box: returns the (left, top) margins that keep at
/// least `keep` pixels of `fig` visible inside a screen of size `(sw, sh)`. Margins may go
/// negative to reach the left/top edges. When the screen is smaller than `keep`, the valid
/// range collapses so the result still degrades to a sane in-bounds value. Extracted from
/// `App::clamp_margins` so the geometry is unit-testable without a live `App`.
fn clamp_figure_margins(
    margin_left: f64,
    margin_top: f64,
    fig: render::Rect,
    (sw, sh): (f64, f64),
    keep: f64,
) -> (f64, f64) {
    let min_left = keep - (fig.x + fig.w) as f64;
    let max_left = (sw - keep - fig.x as f64).max(min_left);
    let min_top = keep - (fig.y + fig.h) as f64;
    let max_top = (sh - keep - fig.y as f64).max(min_top);
    (margin_left.clamp(min_left, max_left), margin_top.clamp(min_top, max_top))
}

fn copy_to_clipboard(text: &str) -> Result<(), String> {
    let trimmed = text.trim_end();
    if trimmed.is_empty() {
        return Err("nothing to copy".to_string());
    }

    if run_clipboard_command("wl-copy", &[], trimmed).is_ok() {
        return Ok(());
    }
    run_clipboard_command("xclip", &["-selection", "clipboard"], trimmed)
        .map_err(|err| format!("install wl-copy or xclip ({err})"))
}

fn run_clipboard_command(program: &str, args: &[&str], text: &str) -> Result<(), String> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|err| err.to_string())?;

    match child.stdin.take() {
        Some(mut stdin) => stdin.write_all(text.as_bytes()).map_err(|err| err.to_string())?,
        None => return Err("clipboard command did not open stdin".to_string()),
    }

    let status = child.wait().map_err(|err| err.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{program} exited with {status}"))
    }
}

fn read_clipboard_text() -> Result<String, String> {
    if let Ok(text) = read_clipboard_command("wl-paste", &["--no-newline"]) {
        return Ok(text);
    }
    read_clipboard_command("xclip", &["-selection", "clipboard", "-out"])
        .map_err(|err| format!("install wl-paste or xclip ({err})"))
}

fn read_clipboard_command(program: &str, args: &[&str]) -> Result<String, String> {
    let output = Command::new(program)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .map_err(|err| err.to_string())?;
    if !output.status.success() {
        return Err(format!("{program} exited with {}", output.status));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn sanitize_paste(text: &str) -> String {
    text.chars()
        .filter_map(|ch| {
            if ch == '\n' || ch == '\r' || ch == '\t' {
                Some(' ')
            } else if ch.is_control() {
                None
            } else {
                Some(ch)
            }
        })
        .take(INPUT_PASTE_MAX_CHARS)
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

enum TorsoSurface {
    Session,
    Text { title: String, body: String },
    // A decoded provider image is held separately on `App.torso_image`; this variant
    // just marks that the torso is in image mode and carries its caption/hint.
    Image { title: String, caption: String, hint: String },
    ImageStub { title: String, caption: String, hint: String },
    FileStub { title: String, caption: String, hint: String },
}

enum TorsoSurfaceSnapshot {
    /// Retained as a rollback fallback — the idle state now snapshots `Passport` instead (see
    /// `snapshot_torso_output`). Kept constructible-on-demand so reverting is a one-line change.
    #[allow(dead_code)]
    Session {
        name: String,
        provider: String,
        model: String,
        gateway: String,
        status: String,
        note: String,
    },
    /// The idle/status passport ledger that supersedes `Session` (fits the 142px torso).
    Passport {
        persona_label: String,
        posture: String,
        provider: Option<String>,
        locality: Option<String>,
        route_health: Option<String>,
        output_preview: Option<String>,
    },
    Text {
        title: String,
        body: String,
    },
    // The decoded image lives on App.torso_image and is lent into `as_render` as a
    // separate (disjoint) field borrow, so this owned snapshot doesn't borrow self.
    Image {
        title: String,
        caption: String,
        hint: String,
    },
    ImageStub {
        title: String,
        caption: String,
        hint: String,
    },
    FileStub {
        title: String,
        caption: String,
        hint: String,
    },
}

#[derive(Clone, Debug, PartialEq)]
struct ReceiptRailEntry {
    effector: String,
    decision: String,
    ts: u64,
    summary: Option<String>,
    route_label: Option<String>,
    executed: Option<bool>,
    receipt_id: String,
}

impl ReceiptRailEntry {
    fn glyph(&self) -> &'static str {
        receipt_status_glyph(&self.decision, self.executed)
    }

    fn time_hms(&self) -> String {
        format_ts_hms(self.ts)
    }

    fn detail_text(&self) -> String {
        let time = self.time_hms();
        let mut parts = Vec::new();
        if let Some(summary) = self.summary.as_deref().filter(|s| !s.trim().is_empty()) {
            parts.push(summary.trim().to_string());
        }
        parts.push(format!("{} {} at {time}.", self.decision, self.effector));
        parts.push(format!("receiptId: {}", self.receipt_id));
        parts.join(" ")
    }
}

fn receipt_status_glyph(decision: &str, executed: Option<bool>) -> &'static str {
    match decision {
        "allow" if executed == Some(false) => "☑",
        "allow" => "✅",
        "needs_confirmation" => "⏳",
        _ => "❌",
    }
}

/// Deterministic seconds-of-day marker from the cue timestamp. It is not localized wall time.
fn format_ts_hms(ts: u64) -> String {
    let seconds = if ts > 10_000_000_000 { ts / 1000 } else { ts };
    let seconds = seconds % 86_400;
    let h = seconds / 3600;
    let m = (seconds % 3600) / 60;
    let s = seconds % 60;
    format!("{h:02}:{m:02}:{s:02}")
}

fn push_receipt_rail_entry(entries: &mut VecDeque<ReceiptRailEntry>, entry: ReceiptRailEntry) {
    entries.push_front(entry);
    while entries.len() > RECEIPT_RAIL_CAP {
        entries.pop_back();
    }
}

impl TorsoSurfaceSnapshot {
    fn as_render<'a>(
        &'a self,
        image: Option<&'a render::TorsoImage>,
    ) -> render::TorsoOutput<'a> {
        match self {
            TorsoSurfaceSnapshot::Session { name, provider, model, gateway, status, note } => {
                render::TorsoOutput::Session(render::SessionCard {
                    name,
                    provider,
                    model,
                    gateway,
                    status,
                    note,
                })
            }
            TorsoSurfaceSnapshot::Passport { persona_label, posture, provider, locality, route_health, output_preview } => {
                render::TorsoOutput::Passport(render::PassportCard {
                    persona_label,
                    posture,
                    provider: provider.as_deref(),
                    locality: locality.as_deref(),
                    route_health: route_health.as_deref(),
                    output_preview: output_preview.as_deref(),
                })
            }
            TorsoSurfaceSnapshot::Text { title, body } => {
                render::TorsoOutput::Text(render::TextCard { title, body })
            }
            TorsoSurfaceSnapshot::Image { title, caption, hint } => {
                let _ = (title, caption, hint);
                render::TorsoOutput::Image(render::ImageCard { image })
            }
            TorsoSurfaceSnapshot::ImageStub { title, caption, hint } => {
                render::TorsoOutput::ImageStub(render::MediaStubCard { title, caption, hint })
            }
            TorsoSurfaceSnapshot::FileStub { title, caption, hint } => {
                render::TorsoOutput::FileStub(render::MediaStubCard { title, caption, hint })
            }
        }
    }
}

fn classify_torso_surface(text: &str) -> TorsoSurface {
    let trimmed = text.trim();
    if let Some(caption) = trimmed.strip_prefix("[image]") {
        return TorsoSurface::ImageStub {
            title: "Image output".to_string(),
            caption: caption.trim().if_empty("Provider image placeholder"),
            hint: "Swap this stub for real image payload rendering when the provider sends media metadata.".to_string(),
        };
    }
    if let Some(caption) = trimmed.strip_prefix("[file]") {
        return TorsoSurface::FileStub {
            title: "File output".to_string(),
            caption: caption.trim().if_empty("Provider file placeholder"),
            hint: "Use this slot for documents, receipts, or other downloadable provider artifacts.".to_string(),
        };
    }
    if trimmed.starts_with("![") {
        return TorsoSurface::ImageStub {
            title: "Image output".to_string(),
            caption: "Markdown image placeholder".to_string(),
            hint: "The torso can reserve image space before we wire a richer payload channel.".to_string(),
        };
    }
    TorsoSurface::Text {
        title: "Text output".to_string(),
        body: trimmed.to_string(),
    }
}

trait IfEmpty {
    fn if_empty(self, fallback: &str) -> String;
}

impl IfEmpty for &str {
    fn if_empty(self, fallback: &str) -> String {
        if self.is_empty() {
            fallback.to_string()
        } else {
            self.to_string()
        }
    }
}

/// Decode a base64 image payload. Tolerates a `data:…;base64,` URL prefix (so an
/// inlined data URL from a provider reply decodes too) and surrounding whitespace.
fn decode_base64(data: &str) -> Option<Vec<u8>> {
    use base64::Engine;
    let trimmed = data.trim();
    let payload = trimmed
        .rsplit_once("base64,")
        .map(|(_, rest)| rest)
        .unwrap_or(trimmed);
    base64::engine::general_purpose::STANDARD.decode(payload).ok()
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
        buddy: std::env::var("BB_BUDDY").unwrap_or_else(|_| "hermes".to_string()),
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
        height: render::Layout::initial().surface_h(),
        margin_left: env_i32("BB_MARGIN_LEFT", 48) as f64,
        margin_top: env_i32("BB_MARGIN_TOP", 48) as f64,
        start: Instant::now(),
        emotion: Emotion::Neutral,
        speech: None,
        torso_surface: TorsoSurface::Session,
        torso_image: None,
        name_label: String::new(),
        provider_label: String::new(),
        model_label: String::new(),
        gateway_label: String::new(),
        presence_status: "Connecting".to_string(),
        session_note: "Speech bubble carries quick updates. Usage and richer provider output land in the torso.".to_string(),
        awaiting_reply: false,
        chat_open: false,
        configured: false,
        press: None,
        drag: false,
        exit: false,
        presence_out: None,
        tucked: None,
        frame_target: None,
        pinned_to_target: false,
        pinned_offset: None,
        pinned_bubble_w: env_i32("BB_PINNED_BUBBLE_W", 248) as f32,
        keyboard: None,
        modifiers: Modifiers::default(),
        input_text: String::new(),
        input_focused: false,
        pending_effector: None,
        receipt_rail: VecDeque::new(),
        active_surface: "session".to_string(),
        surfaces: Vec::new(),
        surface_bloom_open: false,
        active_posture: "work".to_string(),
        active_provider: None,
        active_locality: None,
        active_route_health: None,
        route_flash_until: None,
        facing: Facing::Right,
        body_len: render::BODY_LEN_DEFAULT,
        color: env_color("BB_COLOR"),
    };
    app.init_hermes_surface();

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
    // OnDemand: the surface takes keyboard focus when the user clicks it (e.g. the
    // input box), and the compositor returns focus when they click elsewhere. Keys are
    // still only consumed while `input_focused`, so this never hijacks global typing.
    layer.set_keyboard_interactivity(KeyboardInteractivity::OnDemand);
    layer.commit();

    app.pool = Some(
        SlotPool::new((app.width * app.height * 4) as usize, &app.shm).expect("slot pool failed"),
    );
    app.layer = Some(layer);

    eprintln!(
        "[bb-desktop-body] clay figure up: {}x{} @ margin(L={}, T={}){} — drag the body to move/dock; click the head to chat; drag the feet to stretch",
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
        .insert_source(presence_rx, |event, _meta, app: &mut App| {
            if let ChannelEvent::Msg(text) = event {
                app.on_presence_message(&text);
            }
        })
        .expect("insert presence source");
    app.presence_out = Some(presence::spawn(app.buddy.clone(), presence_tx));

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
    secondary: bool,
    started_at: Instant,
    /// Accumulated physical pointer travel since press — distinguishes click from drag.
    dist: f64,
    /// Whether `grabbed` has been emitted for this press (fires once, when `dist`
    /// first crosses the click slop — never on press, or every click is a phantom grab).
    grabbed_sent: bool,
    /// Whether this press already bloomed the surface dial.
    bloom_started: bool,
}

#[derive(Clone, Copy, PartialEq)]
enum PressTarget {
    Head,
    /// The clay body (torso, arms, legs) outside any control — a move handle like the head,
    /// so a buddy whose head was dragged off-screen can still be grabbed and pulled back. Unlike
    /// the head, a tap here does NOT toggle chat (only a drag moves the buddy).
    Body,
    Input,
    Paste,
    /// The on-body Review / Confirm governance button (visible while chat is open).
    Review,
    /// The on-body Edit / Confirm governance button — emits a typed `repo_edit` intent.
    Edit,
    Perimeter(PerimeterId),
    ReceiptRail(usize),
    SurfaceBloom(usize),
    TorsoAction(TorsoAction),
    /// The legs/feet zone — dragging it vertically stretches the body.
    Feet,
    Bump,
    Outside,
}

fn is_surface_bloom_press(target: PressTarget) -> bool {
    matches!(
        target,
        PressTarget::Perimeter(
            PerimeterId::ArrowN
                | PerimeterId::ArrowE
                | PerimeterId::ArrowS
                | PerimeterId::ArrowW
                | PerimeterId::Quick0
                | PerimeterId::Quick1
                | PerimeterId::Quick2
                | PerimeterId::Quick3
        )
    )
}

fn should_open_surface_bloom(press: &PressState, now: Instant) -> bool {
    !press.secondary
        && !press.bloom_started
        && press.dist <= CLICK_SLOP
        && is_surface_bloom_press(press.target)
        && now.duration_since(press.started_at) >= SURFACE_BLOOM_HOLD
}

/// The native OS window currently available for pinning, kept live by target cues.
struct FrameTarget {
    id: String,
    /// Window title and app id for pinned-mode status text.
    title: String,
    app_id: String,
    bounds: presence::TargetBounds,
}

#[derive(Clone, Copy)]
struct FrameBounds {
    x: f64,
    y: f64,
    w: f64,
}

struct App {
    /// This body's identity — filters inbound cues and stamps outbound events.
    buddy: String,
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
    torso_surface: TorsoSurface,
    /// Decoded image currently shown when `torso_surface` is `Image` — held here (not
    /// in the enum) so a redraw blits it without re-decoding, and so the snapshot stays
    /// owned. Set from an `output` cue's inline bytes via `decode_image_bytes`.
    torso_image: Option<render::TorsoImage>,
    /// Display name for the header (e.g. "Border Wizard"); `HERMES_NAME` env override,
    /// defaulting to the title-cased wire id. Distinct from `buddy` (the wire id).
    name_label: String,
    provider_label: String,
    model_label: String,
    gateway_label: String,
    presence_status: String,
    session_note: String,
    awaiting_reply: bool,
    chat_open: bool,
    configured: bool,
    press: Option<PressState>,
    drag: bool,
    exit: bool,
    /// Outbound to-soul events (clicked/grabbed/dropped). Best-effort: `None` until the
    /// presence thread starts, and sends are dropped when no soul is connected.
    presence_out: Option<std::sync::mpsc::Sender<String>>,
    /// When `Some`, the buddy is tucked against this edge — shown as a minimized bump,
    /// input shrunk to the bump, clicking it summons the buddy back out.
    tucked: Option<presence::Edge>,
    /// When `Some`, a platform driver has identified a native OS window that Hermes
    /// can be pinned to. This is tracking state only; right-click toggles presentation.
    frame_target: Option<FrameTarget>,
    /// Right-click toggles this on once a target has been acquired. The body remains
    /// presentation-only: it follows target cues, but it never reads or moves windows.
    pinned_to_target: bool,
    /// User-chosen surface offset from the tracked target origin while pinned.
    pinned_offset: Option<(f64, f64)>,
    /// Resizable pinned bubble width; env-tunable now, drag handle later.
    pinned_bubble_w: f32,
    /// Keyboard, acquired when the seat advertises the capability. Drives the on-body
    /// text input that replaces the old "Say hello" button.
    keyboard: Option<wl_keyboard::WlKeyboard>,
    /// Latest keyboard modifier state from the compositor; used for shortcuts like paste.
    modifiers: Modifiers,
    /// The text the user is typing to the buddy (only mutated while `input_focused`).
    input_text: String,
    /// Whether the input box has focus — gates keystrokes and shows the caret.
    input_focused: bool,
    /// The effector whose last action_result asked for confirmation, so its on-body governance
    /// button renders (and acts) as Confirm: `receipt_review` flips Review, `repo_edit` flips Edit.
    /// Kept per-effector (not a bare bool) so one act's pending confirm never flips the other's
    /// button. Cleared on any allow/blocked result. `None` = nothing awaiting confirmation.
    pending_effector: Option<String>,
    /// Last 20 action_result cues, newest first. This is display state only; the full
    /// ActionReceipt stays soul-side.
    receipt_rail: VecDeque<ReceiptRailEntry>,
    active_surface: String,
    /// Ordered surface list with per-surface availability, soul-pushed on `hydrate`. The body
    /// cycles and dims from this (Slice 2a) so it never imports the TS surface manifest. Empty
    /// until the first `hydrate` carrying surfaces; until then the body falls back to the
    /// `SURFACE_ORDER` seed below treated as all-available.
    surfaces: Vec<presence::SurfaceDescriptor>,
    /// Local hold-to-bloom dial state; selection still emits `surface_request`.
    surface_bloom_open: bool,
    active_posture: String,
    active_provider: Option<String>,
    /// `local | cloud` from the last `surface_active.route.locality`, drives the passport
    /// locality dot. `None` until a surface with a route is activated.
    active_locality: Option<String>,
    /// Optional soul-derived `ready | degraded | unavailable` from `surface_active.route.health`.
    /// Absent means no ring, preserving back-compat with older cues.
    active_route_health: Option<String>,
    /// Body-observed local→cloud transition flash deadline. This is presentation memory only,
    /// separate from soul-derived health.
    route_flash_until: Option<Instant>,
    /// Which side the bubble/input sit on — recomputed from screen position so the
    /// UI always faces the screen centre, never the docked edge.
    facing: Facing,
    /// Stretchable torso length (the user drags the feet to resize the figure).
    body_len: f32,
    /// Clay colour from `BB_COLOR`.
    color: [u8; 3],
}

/// Map a presence-protocol edge onto the renderer's bump edge.
fn edge_to_bump(edge: presence::Edge) -> BumpEdge {
    match edge {
        presence::Edge::Top => BumpEdge::Top,
        presence::Edge::Right => BumpEdge::Right,
        presence::Edge::Bottom => BumpEdge::Bottom,
        presence::Edge::Left => BumpEdge::Left,
    }
}

fn active_perimeter_controls_for(chat_open: bool, layout: render::Layout) -> Vec<(PerimeterId, render::Rect)> {
    layout
        .perimeter_controls()
        .into_iter()
        .filter(|(id, _)| chat_open || !matches!(id, PerimeterId::Paste | PerimeterId::Review | PerimeterId::Edit))
        .collect()
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
    fn init_hermes_surface(&mut self) {
        // Display name: HERMES_NAME (or <BUDDY>_NAME) override, else the title-cased wire id.
        // The soul sending identity over the wire is the eventual single-source fix; until
        // then the body is env-configured, exactly as provider/model below already are.
        self.name_label = buddy_env(&self.buddy, "NAME").unwrap_or_else(|| render::title_case(&self.buddy));
        self.provider_label = buddy_env(&self.buddy, "PROVIDER").unwrap_or_else(|| "echo".to_string());
        self.model_label = buddy_env(&self.buddy, "MODEL").unwrap_or_else(|| "not configured".to_string());
        self.gateway_label = std::env::var("BB_PRESENCE_URL")
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "ws://127.0.0.1:17387/border-buddies".to_string());
        self.torso_surface = TorsoSurface::Session;
        self.session_note =
            "Speech bubble carries quick updates. Provider text, images, and files land here.".to_string();
    }

    fn tick(&mut self) {
        self.update_surface_bloom_hold();
        self.draw();
        let _ = self.conn.flush();
    }

    /// The current parameterized surface layout (facing + stretch) — single source
    /// for drawing, hit-testing, and input regions so they can never disagree.
    fn layout(&self) -> render::Layout {
        render::Layout { facing: self.facing, body_len: self.body_len }
    }

    fn pinned_layout(&self) -> Option<render::PinnedLayout> {
        if self.tucked.is_some() || !self.pinned_to_target || self.frame_target.is_none() {
            return None;
        }
        Some(render::PinnedLayout::new(self.pinned_bubble_w))
    }

    fn adjusted_frame_bounds(&self) -> Option<FrameBounds> {
        let target = self.frame_target.as_ref()?;
        // COSMIC/Firefox currently reports geometry that visually starts below the top
        // browser chrome. Keep knobs explicit so other apps/compositors can be tuned
        // without another build.
        let expand_top = env_i32("BB_FRAME_EXPAND_TOP", 34) as f64;
        let expand_right = env_i32("BB_FRAME_EXPAND_RIGHT", 0) as f64;
        let expand_left = env_i32("BB_FRAME_EXPAND_LEFT", 0) as f64;
        let offset_x = env_i32("BB_FRAME_OFFSET_X", 0) as f64;
        let offset_y = env_i32("BB_FRAME_OFFSET_Y", 0) as f64;
        Some(FrameBounds {
            x: target.bounds.x + offset_x - expand_left,
            y: target.bounds.y + offset_y - expand_top,
            w: (target.bounds.w + expand_left + expand_right).max(1.0),
        })
    }

    /// Actual compositor-acked surface size. Drawing, hit-testing, and tucked input
    /// regions use this so they hug the buffer that really exists.
    fn surface_size(&self) -> (f64, f64) {
        (self.width as f64, self.height as f64)
    }

    /// Requested full-body surface size. Edge placement uses this deterministic size
    /// when tucking so transient compositor shrinkage cannot park the bump off-buffer.
    fn requested_surface_size(&self) -> (f64, f64) {
        if self.tucked.is_none() {
            if self.pinned_layout().is_some() {
                return (render::PINNED_SURFACE_W as f64, render::PINNED_SURFACE_H as f64);
            }
        }
        (self.requested_surface_w() as f64, self.layout().surface_h() as f64)
    }

    fn receipt_rail_visible(&self) -> bool {
        self.tucked.is_none()
            && self.pinned_layout().is_none()
            && render::receipt_rail_visible_for_body_len(self.body_len)
    }

    fn requested_surface_w(&self) -> u32 {
        if self.receipt_rail_visible() {
            render::SURFACE_W + render::RECEIPT_RAIL_W
        } else {
            render::SURFACE_W
        }
    }

    fn body_hit_x(&self, x: f64) -> f64 {
        if self.receipt_rail_visible() {
            x - render::RECEIPT_RAIL_W as f64
        } else {
            x
        }
    }

    fn offset_rect_for_body(&self, rect: render::Rect) -> render::Rect {
        if self.receipt_rail_visible() {
            render::Rect { x: rect.x + render::RECEIPT_RAIL_W as f32, ..rect }
        } else {
            rect
        }
    }

    fn tucked_bump_rect(&self, edge: presence::Edge) -> render::Rect {
        render::bump_rect(edge_to_bump(edge), self.width, self.height)
    }

    fn point_in_tucked_bump(&self, edge: presence::Edge, x: f64, y: f64) -> bool {
        render::point_in_bump(edge_to_bump(edge), self.width, self.height, x, y)
    }

    fn draw(&mut self) {
        if !self.configured {
            return; // never attach a buffer before the first configure is acked
        }
        let layout = self.layout();
        let torso_output = self.snapshot_torso_output();
        // Direct field borrow, disjoint from self.pool/self.sprite below — lets the
        // decoded image blit zero-copy without the snapshot borrowing all of self.
        let torso_image = self.torso_image.as_ref();
        let speech = self.speech.clone();
        let input_text = self.input_text.clone();
        let input_placeholder = format!("Ask {}...", self.name_label);
        let posture_badge = (self.active_posture == "private").then_some("PRIVATE LOCAL");
        let route_health = self.active_route_health.clone();
        let now = Instant::now();
        let route_flash = self.route_flash_until.is_some_and(|deadline| deadline > now);
        if !route_flash {
            self.route_flash_until = None;
        }
        let receipt_time_labels: Vec<String> = self.receipt_rail.iter().map(ReceiptRailEntry::time_hms).collect();
        let receipt_rail_items: Vec<render::ReceiptRailItem<'_>> = self
            .receipt_rail
            .iter()
            .zip(receipt_time_labels.iter())
            .map(|(entry, time)| render::ReceiptRailItem {
                glyph: entry.glyph(),
                effector: entry.effector.as_str(),
                decision: entry.decision.as_str(),
                route_label: entry.route_label.as_deref(),
                time: time.as_str(),
            })
            .collect();
        // Fade each quick button whose surface the soul reported `unwired` (Slice 2a).
        let mut dim_quick = [false; 4];
        for (slot, id) in SURFACE_QUICK.iter().take(4).enumerate() {
            dim_quick[slot] = self.surface_availability(id) == "unwired";
        }
        let bloom_order = self.surface_bloom_surfaces();
        let bloom_items: Vec<render::SurfaceDialItem<'_>> = if self.surface_bloom_open {
            bloom_order
                .iter()
                .map(|surface| render::SurfaceDialItem {
                    label: surface.label.as_str(),
                    availability: surface.availability.as_str(),
                    active: surface.id == self.active_surface,
                    kind: surface.kind.as_str(),
                })
                .collect()
        } else {
            Vec::new()
        };
        let pinned = self.pinned_layout();
        let (Some(pool), Some(layer)) = (self.pool.as_mut(), self.layer.as_ref()) else {
            return;
        };
        let (w, h) = (self.width, self.height);
        let stride = (w * 4) as i32;

        let view = BodyView {
            t: self.start.elapsed().as_secs_f32(),
            emotion: self.emotion,
            speech: speech.as_deref(),
            torso_output: torso_output.as_render(torso_image),
            chat_open: self.chat_open,
            tucked: self.tucked.map(edge_to_bump),
            input_text: &input_text,
            input_placeholder: &input_placeholder,
            input_focused: self.input_focused,
            review_pending: self.pending_effector.as_deref() == Some("receipt_review"),
            edit_pending: self.pending_effector.as_deref() == Some("repo_edit"),
            posture_badge,
            dim_quick,
            surface_bloom: &bloom_items,
            route_health: route_health.as_deref(),
            route_flash,
            receipt_rail: &receipt_rail_items,
            layout,
            pinned,
            frame: None,
            color: self.color,
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

    fn snapshot_torso_output(&self) -> TorsoSurfaceSnapshot {
        // Owned snapshot — the decoded image is lent separately in draw() (see torso_image).
        match &self.torso_surface {
            // Idle/status now wears the passport ledger (Session snapshot retained for rollback).
            TorsoSurface::Session => TorsoSurfaceSnapshot::Passport {
                persona_label: self.name_label.clone(),
                posture: self.active_posture.clone(),
                provider: self
                    .active_provider
                    .clone()
                    .or_else(|| (!self.provider_label.is_empty()).then(|| self.provider_label.clone())),
                locality: self.active_locality.clone(),
                route_health: self.active_route_health.clone(),
                output_preview: Some(self.session_note.clone()),
            },
            TorsoSurface::Text { title, body } => TorsoSurfaceSnapshot::Text {
                title: title.clone(),
                body: body.clone(),
            },
            TorsoSurface::Image { title, caption, hint } => {
                TorsoSurfaceSnapshot::Image {
                    title: title.clone(),
                    caption: caption.clone(),
                    hint: hint.clone(),
                }
            }
            TorsoSurface::ImageStub { title, caption, hint } => {
                TorsoSurfaceSnapshot::ImageStub {
                    title: title.clone(),
                    caption: caption.clone(),
                    hint: hint.clone(),
                }
            }
            TorsoSurface::FileStub { title, caption, hint } => {
                TorsoSurfaceSnapshot::FileStub {
                    title: title.clone(),
                    caption: caption.clone(),
                    hint: hint.clone(),
                }
            }
        }
    }

    fn mark_presence_connected(&mut self) {
        self.presence_status = format!("Linked to {}", self.provider_label);
        if matches!(self.torso_surface, TorsoSurface::Session) && !self.awaiting_reply {
            self.session_note =
                "Speech bubble carries quick updates. Text, image, and file output can land here.".to_string();
        }
    }

    fn show_reply_in_torso(&mut self, text: &str) {
        self.torso_surface = classify_torso_surface(text);
        self.awaiting_reply = false;
        self.pending_effector = None;
        self.session_note = "Latest provider output loaded in the torso.".to_string();
    }

    /// Apply a typed `output` cue to the torso. `text`/`session` are handled here;
    /// `image`/`file` carry inline base64 bytes — Slice 3 decodes and renders them, so
    /// for now they land as an honest "received" stub rather than a fake preview.
    fn apply_output(
        &mut self,
        surface: &str,
        text: Option<String>,
        caption: Option<String>,
        media_type: Option<String>,
        data_base64: Option<String>,
    ) {
        self.awaiting_reply = false;
        self.pending_effector = None;
        match surface {
            "text" => {
                let body = text.unwrap_or_default();
                self.torso_surface = TorsoSurface::Text {
                    title: "Text output".to_string(),
                    body: body.trim().to_string(),
                };
                self.session_note = "Latest provider output loaded in the torso.".to_string();
            }
            "session" => {
                self.torso_surface = TorsoSurface::Session;
                self.session_note =
                    "Output cleared. Speech bubble carries quick updates.".to_string();
            }
            "image" => {
                let decoded = data_base64
                    .as_deref()
                    .and_then(decode_base64)
                    .and_then(|bytes| render::decode_image_bytes(&bytes));
                match decoded {
                    Some(pixmap) => {
                        self.torso_image = Some(pixmap);
                        self.torso_surface = TorsoSurface::Image {
                            title: "Image output".to_string(),
                            caption: caption.unwrap_or_else(|| "Generated image".to_string()),
                            hint: String::new(),
                        };
                        self.session_note = "Image rendered in the torso.".to_string();
                    }
                    None => {
                        // Honest failure: we received an image we couldn't decode — say so
                        // rather than show a blank or fake frame.
                        self.torso_image = None;
                        self.torso_surface = TorsoSurface::ImageStub {
                            title: "Image output".to_string(),
                            caption: caption.unwrap_or_else(|| "Image received".to_string()),
                            hint: "Could not decode the image bytes (unsupported format?)."
                                .to_string(),
                        };
                    }
                }
            }
            "file" => {
                // Files aren't raster-rendered; present them honestly as a typed stub.
                self.torso_surface = TorsoSurface::FileStub {
                    title: "File output".to_string(),
                    caption: caption.unwrap_or_else(|| "File received".to_string()),
                    hint: media_type
                        .map(|mt| format!("Provider file ({mt})."))
                        .unwrap_or_else(|| "Provider file artifact.".to_string()),
                };
            }
            _ => {}
        }
        self.update_input_region();
    }

    fn bubble_for_surface(&self, text: &str) -> String {
        match classify_torso_surface(text) {
            TorsoSurface::Image { .. } => "Image ready in torso.".to_string(),
            TorsoSurface::ImageStub { .. } => {
                "Here is your picture. Click to open. Tell me what next?".to_string()
            }
            TorsoSurface::FileStub { .. } => "File stub ready in torso.".to_string(),
            TorsoSurface::Text { .. } => {
                if text.len() > 56 || text.contains('\n') {
                    "Reply ready in torso.".to_string()
                } else {
                    text.to_string()
                }
            }
            TorsoSurface::Session => text.to_string(),
        }
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
        self.update_facing();
    }

    fn set_layer_size(&mut self, w: u32, h: u32) {
        if self.width == w && self.height == h {
            return;
        }
        if let Some(layer) = self.layer.as_ref() {
            layer.set_size(w, h);
            layer.commit();
        }
    }

    fn sync_pinned_surface(&mut self) {
        if self.tucked.is_some() {
            return;
        }
        if !self.pinned_to_target {
            self.set_layer_size(self.requested_surface_w(), self.layout().surface_h());
            self.update_input_region();
            return;
        };
        let Some(target) = self.adjusted_frame_bounds() else { return };
        let Some(pinned) = self.pinned_layout() else { return };
        let (offset_x, offset_y) = self.pinned_offset.unwrap_or_else(|| {
            let head = pinned.head_rect();
            let head_cx = head.x as f64 + head.w as f64 / 2.0;
            let head_cy = head.y as f64 + head.h as f64 / 2.0;
            (target.w - 18.0 - head_cx, 18.0 - head_cy)
        });
        self.margin_left = target.x + offset_x;
        self.margin_top = target.y + offset_y;
        self.clamp_pinned_margins();
        self.remember_pinned_offset();
        self.set_layer_size(render::PINNED_SURFACE_W, render::PINNED_SURFACE_H);
        self.reposition();
        self.update_input_region();
    }

    fn clamp_pinned_margins(&mut self) {
        let Some((sw, sh)) = self.screen else { return };
        self.margin_left = self.margin_left.clamp(0.0, (sw - render::PINNED_SURFACE_W as f64).max(0.0));
        self.margin_top = self.margin_top.clamp(0.0, (sh - render::PINNED_SURFACE_H as f64).max(0.0));
    }

    fn remember_pinned_offset(&mut self) {
        let Some(target) = self.adjusted_frame_bounds() else { return };
        self.pinned_offset = Some((self.margin_left - target.x, self.margin_top - target.y));
    }

    /// Keep the bubble + input on the side of the figure that faces the screen
    /// centre, so the UI never clips against the edge the buddy is docked to.
    fn update_facing(&mut self) {
        let Some((sw, _)) = self.screen else { return };
        let head_x = self.margin_left + render::FIG_CX as f64;
        let new_facing = if head_x > sw / 2.0 { Facing::Left } else { Facing::Right };
        if new_facing != self.facing {
            self.facing = new_facing;
            self.update_input_region();
        }
    }

    /// Keep the buddy figure on-screen — not just the head — so it can never be dragged
    /// fully off (the whole clay body is a drag handle now, not only the head). At least
    /// `DRAG_KEEP_VISIBLE` pixels of the figure bounding box must stay visible on each axis;
    /// margins may go negative to reach the left/top edges. On a screen smaller than the
    /// keep-visible sliver, the range collapses so the clamp still degrades sanely.
    fn clamp_margins(&mut self) {
        let (sw, sh) = self.screen.unwrap_or((f64::MAX, f64::MAX));
        let fig = render::figure_bbox(self.body_len);
        let (left, top) = clamp_figure_margins(
            self.margin_left,
            self.margin_top,
            fig,
            (sw, sh),
            render::DRAG_KEEP_VISIBLE as f64,
        );
        self.margin_left = left;
        self.margin_top = top;
    }

    fn active_perimeter_controls(&self, layout: render::Layout) -> Vec<(PerimeterId, render::Rect)> {
        active_perimeter_controls_for(self.chat_open, layout)
    }

    /// Input region = only the parts that should catch the pointer; everywhere else
    /// is transparent AND click-through. Recomputed whenever the menu/bubble toggles.
    fn update_input_region(&mut self) {
        let Some(layer) = self.layer.as_ref() else { return };
        let Ok(region) = Region::new(&self.compositor) else { return };

        // Tucked: only the bump catches the pointer — everything else is click-through,
        // so the screen space the buddy stepped aside from is truly freed.
        let rects = if let Some(edge) = self.tucked {
            vec![self.tucked_bump_rect(edge).as_i32()]
        } else if let Some(pinned) = self.pinned_layout() {
            let mut rects = vec![pinned.head_rect().as_i32()];
            if self.speech.is_some() {
                rects.push(pinned.bubble_rect().as_i32());
            }
            if self.chat_open {
                rects.push(pinned.input_region_rect().as_i32());
            }
            rects
        } else {
            let layout = self.layout();
            // Head = move/dock handle; feet = stretch handle. The torso stays
            // mostly click-through — only the small torso action points catch input.
            let mut rects = Vec::new();
            if self.receipt_rail_visible() {
                rects.push((0, 0, render::RECEIPT_RAIL_W as i32, self.height as i32));
            }
            rects.push(self.offset_rect_for_body(render::head_rect()).as_i32());
            rects.push(self.offset_rect_for_body(layout.feet_rect()).as_i32());
            for (_, rect) in self.active_perimeter_controls(layout) {
                rects.push(self.offset_rect_for_body(rect).as_i32());
            }
            if self.surface_bloom_open {
                for rect in layout.surface_bloom_rects(self.surface_bloom_surfaces().len()) {
                    rects.push(self.offset_rect_for_body(rect).as_i32());
                }
            }
            if self.speech.is_some() {
                rects.push(self.offset_rect_for_body(layout.bubble_rect()).as_i32());
            }
            if self.chat_open {
                rects.push(self.offset_rect_for_body(layout.input_region_rect()).as_i32());
            }
            rects.push(self.offset_rect_for_body(layout.torso_action_rect(TorsoAction::Expand)).as_i32());
            rects.push(self.offset_rect_for_body(layout.torso_action_rect(TorsoAction::Copy)).as_i32());
            rects.push(self.offset_rect_for_body(layout.torso_action_rect(TorsoAction::Scroll)).as_i32());
            rects
        };
        for (x, y, w, h) in rects {
            region.add(x, y, w, h);
        }
        layer.wl_surface().set_input_region(Some(region.wl_region()));
        layer.commit();
    }

    // --- presence: the soul driving the body ---

    /// Apply an inbound presence frame. Cues for other buddies, and anything
    /// malformed or not a body cue, are ignored (the parser already dropped them).
    fn on_presence_message(&mut self, text: &str) {
        let Some(msg) = presence::parse_to_body(text) else { return };
        if msg.buddy != self.buddy {
            return;
        }
        self.mark_presence_connected();
        if debug_log_enabled() {
            eprintln!("[bb-presence] applying cue for {}: {:?}", msg.buddy, msg.cue);
        }
        let cue_ts = msg.ts;
        match msg.cue {
            presence::Cue::Express { emotion } => {
                if let Some(e) = Emotion::from_wire(&emotion) {
                    self.set_emotion(e);
                }
            }
            presence::Cue::Say { text } => self.say(text),
            presence::Cue::MoveTo { position } => self.apply_position(position),
            presence::Cue::Hydrate { position, emotion, speech, surfaces } => {
                if let Some(p) = position {
                    self.apply_position(p);
                }
                if let Some(e) = emotion.as_deref().and_then(Emotion::from_wire) {
                    self.set_emotion(e);
                }
                if let Some(s) = speech {
                    self.say(s);
                }
                // Adopt the soul-pushed surface list; an omitted list leaves the seed in place.
                if !surfaces.is_empty() {
                    self.surfaces = surfaces;
                }
            }
            presence::Cue::Output { surface, text, caption, media_type, data_base64 } => {
                self.apply_output(&surface, text, caption, media_type, data_base64);
            }
            presence::Cue::ActionResult { effector, decision, receipt_id, summary, outcome, .. } => {
                // Present the soul's authorization outcome — the body renders it, it never decides
                // it (law 7). The face is the fastest read: each decision wears a DISTINCT, honest
                // expression (allow→happy, needs_confirmation→curious, blocked→alert) so a glance
                // conveys it before any prose. `needs_confirmation` also flips THIS effector's
                // on-body button (Review or Edit) into Confirm until resolved.
                let executed = outcome.as_ref().map(|o| o.executed);
                let route_label = outcome
                    .as_ref()
                    .and_then(|o| o.route.as_ref())
                    .map(|r| r.provider.clone());
                push_receipt_rail_entry(&mut self.receipt_rail, ReceiptRailEntry {
                    effector: effector.clone(),
                    decision: decision.clone(),
                    ts: cue_ts,
                    summary: summary.clone(),
                    route_label,
                    executed,
                    receipt_id,
                });
                self.pending_effector = (decision == "needs_confirmation").then_some(effector);
                self.set_emotion(Emotion::for_decision(&decision));
                if let Some(text) = summary {
                    self.say(text);
                }
                self.update_input_region();
            }
            presence::Cue::SurfaceActive { surface, posture, label, provider_label, route } => {
                self.active_surface = surface;
                self.active_posture = posture;
                // Prefer the route's provider label + locality when present; the passport row
                // reads these. Fall back to the flat providerLabel for the provider name.
                let previous_locality = self.active_locality.as_deref();
                let next_locality = route.as_ref().map(|r| r.locality.as_str());
                if previous_locality == Some("local") && next_locality == Some("cloud") {
                    self.route_flash_until = Some(Instant::now() + Duration::from_secs(2));
                }
                self.active_locality = route.as_ref().map(|r| r.locality.clone());
                self.active_route_health = route.as_ref().and_then(|r| r.health.clone());
                self.active_provider = route
                    .as_ref()
                    .map(|r| r.label.clone())
                    .or_else(|| provider_label.clone());
                if let Some(provider) = provider_label {
                    self.provider_label = provider;
                }
                self.presence_status = label.clone().unwrap_or_else(|| "Surface active".to_string());
                self.say(label.unwrap_or_else(|| "Surface active".to_string()));
                self.update_input_region();
            }
            presence::Cue::TargetAcquired { target_id, title, app_id, bounds } => {
                if self.tucked.is_some() {
                    return;
                }
                eprintln!("[bb-presence] tracked target {target_id} ({app_id} — {title:?})");
                if !self.pinned_to_target {
                    self.speech = Some(format!("Right-click {} to pin to {app_id}.", self.name_label));
                    self.pinned_offset = None;
                }
                self.frame_target = Some(FrameTarget { id: target_id, title, app_id, bounds });
                self.sync_pinned_surface();
            }
            presence::Cue::TargetMoved { target_id, bounds } => {
                // Only the tracked window updates the pinned overlay; a stray move for
                // another target id is ignored rather than snapping us onto it.
                if let Some(frame) = self.frame_target.as_mut() {
                    if frame.id == target_id {
                        frame.bounds = bounds;
                        self.sync_pinned_surface();
                    }
                }
            }
            presence::Cue::TargetLost { target_id, reason } => {
                // Release only if it's our tracked target.
                if self.frame_target.as_ref().is_some_and(|f| f.id == target_id) {
                    eprintln!("[bb-presence] target lost {target_id}: {reason:?}");
                    self.frame_target = None;
                    self.pinned_to_target = false;
                    self.pinned_offset = None;
                    self.speech = Some("Released the window.".to_string());
                    self.sync_pinned_surface();
                }
            }
        }
    }

    /// Place the body at an abstract presence position. Anchored/Free share the margin
    /// + clamp + reposition path with dragging, so move_to and drag can never drift
    /// apart; Tucked enters the tucked state so a persisted tuck round-trips through
    /// `hydrate` and comes back as a bump.
    fn apply_position(&mut self, position: presence::Position) {
        let (left, top) = match position {
            presence::Position::Tucked { edge, ox, oy } => {
                let along = match edge {
                    presence::Edge::Left | presence::Edge::Right => oy,
                    presence::Edge::Top | presence::Edge::Bottom => ox,
                };
                self.enter_tuck(edge, along);
                return;
            }
            presence::Position::Anchored { edge, ox, oy } => presence::anchored_to_margins(
                edge,
                ox,
                oy,
                self.surface_size(),
                self.screen,
            ),
            presence::Position::Free { x, y } => (x, y),
        };
        // A non-tucked position pops the buddy back to the full figure.
        let was_tucked = self.tucked.take().is_some();
        let was_pinned = self.pinned_to_target;
        self.pinned_to_target = false;
        self.pinned_offset = None;
        self.margin_left = left;
        self.margin_top = top;
        self.clamp_margins();
        if was_pinned {
            self.set_layer_size(self.requested_surface_w(), self.layout().surface_h());
        }
        if was_tucked || was_pinned {
            self.update_input_region();
        }
        self.reposition();
    }

    // --- presentation API (called by presence cues above and local interaction) ---

    fn set_emotion(&mut self, emotion: Emotion) {
        self.emotion = emotion;
    }

    fn say(&mut self, text: impl Into<String>) {
        let text = text.into();
        if self.awaiting_reply {
            self.show_reply_in_torso(&text);
        }
        self.speech = Some(self.bubble_for_surface(&text));
        self.update_input_region();
    }

    /// Open/close the chat input. Opening focuses it immediately (click head →
    /// type). No local mood buttons here: expression belongs to the soul, which
    /// drives it through `express` cues — the user talks, the buddy feels.
    fn toggle_chat(&mut self) {
        self.chat_open = !self.chat_open;
        if self.chat_open {
            self.input_focused = true;
        } else {
            self.speech = None;
            // Closing the chat drops any half-typed input and the caret.
            self.input_focused = false;
            self.input_text.clear();
        }
        self.update_input_region();
    }

    fn toggle_pin(&mut self) {
        if self.pinned_to_target {
            self.pinned_to_target = false;
            self.pinned_offset = None;
            self.chat_open = false;
            self.input_focused = false;
            self.speech = Some("Unpinned.".to_string());
            self.set_layer_size(self.requested_surface_w(), self.layout().surface_h());
            self.clamp_margins();
            self.reposition();
            self.update_input_region();
            return;
        }

        let Some(target) = self.frame_target.as_ref() else {
            self.speech = Some("No active target to pin yet.".to_string());
            self.update_input_region();
            return;
        };
        self.pinned_to_target = true;
        self.pinned_offset = None;
        self.chat_open = false;
        self.input_focused = false;
        let name = if target.title.trim().is_empty() { target.app_id.as_str() } else { target.title.as_str() };
        self.speech = Some(format!("Pinned to {name}."));
        self.sync_pinned_surface();
    }

    /// Handle a keystroke while the input box is focused: submit on Enter, edit on
    /// Backspace, defocus on Escape, otherwise append printable text.
    fn on_key(&mut self, event: KeyEvent) {
        let ks = event.keysym;
        if self.modifiers.ctrl && (ks == Keysym::v || ks == Keysym::V) {
            self.paste_clipboard_into_input();
        } else if ks == Keysym::Return || ks == Keysym::KP_Enter {
            self.submit_input();
        } else if ks == Keysym::BackSpace {
            self.input_text.pop();
        } else if ks == Keysym::Escape {
            self.input_focused = false;
        } else if let Some(text) = event.utf8 {
            self.append_input_text(&text);
        }
    }

    fn append_input_text(&mut self, text: &str) {
        for ch in text.chars() {
            if !ch.is_control() {
                self.input_text.push(ch);
            }
        }
    }

    fn paste_clipboard_into_input(&mut self) {
        match read_clipboard_text().map(|text| sanitize_paste(&text)).and_then(|text| {
            if text.is_empty() {
                Err("clipboard has no text".to_string())
            } else {
                Ok(text)
            }
        }) {
            Ok(text) => {
                if self.input_text.chars().last().is_some_and(|ch| !ch.is_whitespace()) {
                    self.input_text.push(' ');
                }
                self.append_input_text(&text);
                self.speech = Some("Pasted into input.".to_string());
            }
            Err(err) => {
                self.speech = Some(format!("Paste failed: {err}"));
            }
        }
        self.update_input_region();
    }

    /// Send what the user typed to the soul as a `said` event and echo it locally so
    /// they see it land. The reply arrives as a `say` cue from the soul.
    fn submit_input(&mut self) {
        let text = self.input_text.trim().to_string();
        if text.is_empty() {
            return;
        }
        self.send_to_soul(presence::said_json(&self.buddy, &text));
        self.set_emotion(Emotion::Thinking);
        self.awaiting_reply = true;
        self.pending_effector = None;
        self.presence_status = format!("{} is thinking", self.buddy);
        self.torso_surface = TorsoSurface::Text {
            title: "Reply pending".to_string(),
            body: format!("Waiting for {} to answer your latest prompt.", self.buddy),
        };
        self.speech = Some("Message sent.".to_string());
        self.update_input_region();
        self.input_text.clear();
    }

    fn on_press(&mut self, x: f64, y: f64, button: u32) {
        let secondary = button == BTN_RIGHT;
        let primary = button == BTN_LEFT;
        if self.receipt_rail_visible() && x < render::RECEIPT_RAIL_W as f64 {
            let target = if primary {
                render::receipt_rail_card_index(x, y, self.receipt_rail.len())
                    .map(PressTarget::ReceiptRail)
                    .unwrap_or(PressTarget::Outside)
            } else {
                PressTarget::Outside
            };
            self.press = Some(PressState { target, secondary, started_at: Instant::now(), dist: 0.0, grabbed_sent: false, bloom_started: false });
            return;
        }
        let body_x = self.body_hit_x(x);
        let bloom_hit = self.surface_bloom_open.then(|| self.surface_bloom_hit_index(body_x, y)).flatten();
        if let Some(idx) = bloom_hit {
            self.press = Some(PressState { target: PressTarget::SurfaceBloom(idx), secondary, started_at: Instant::now(), dist: 0.0, grabbed_sent: false, bloom_started: false });
            return;
        }
        if self.surface_bloom_open {
            self.surface_bloom_open = false;
            self.update_input_region();
        }
        // While tucked, the only live target is the bump; a click on it summons the
        // buddy back out. The bump is not draggable in v1.
        if let Some(edge) = self.tucked {
            let target = if self.point_in_tucked_bump(edge, x, y) {
                PressTarget::Bump
            } else {
                PressTarget::Outside
            };
            self.press = Some(PressState { target, secondary, started_at: Instant::now(), dist: 0.0, grabbed_sent: false, bloom_started: false });
            return;
        }

        let layout = self.layout();
        let target = if let Some(pinned) = self.pinned_layout() {
            if pinned.contains_head(x, y) {
                PressTarget::Head
            } else if self.chat_open && pinned.input_region_rect().contains(x, y) {
                PressTarget::Input
            } else {
                PressTarget::Outside
            }
        } else if render::point_in_head(body_x, y) {
            PressTarget::Head
        } else if let Some((id, _)) = self
            .active_perimeter_controls(layout)
            .into_iter()
            .find(|(_, rect)| rect.contains(body_x, y))
        {
            match id {
                PerimeterId::Paste => PressTarget::Paste,
                PerimeterId::Review => PressTarget::Review,
                PerimeterId::Edit => PressTarget::Edit,
                other => PressTarget::Perimeter(other),
            }
        } else if self.chat_open && layout.input_region_rect().contains(body_x, y) {
            PressTarget::Input
        } else if let Some(action) = render::torso_action_at(&layout, body_x, y) {
            PressTarget::TorsoAction(action)
        } else if layout.feet_rect().contains(body_x, y) {
            PressTarget::Feet
        } else if render::point_in_draggable_body(&layout, body_x, y) {
            PressTarget::Body
        } else {
            PressTarget::Outside
        };

        self.press = Some(PressState { target, secondary, started_at: Instant::now(), dist: 0.0, grabbed_sent: false, bloom_started: false });
        if primary && matches!(target, PressTarget::Head | PressTarget::Body) && !self.pinned_to_target {
            self.drag = true;
        }
    }

    /// Physical pointer delta from `wp_relative_pointer`. Apply it straight to the
    /// margins while dragging — no surface-frame feedback, no runaway.
    fn on_drag_delta(&mut self, dx: f64, dy: f64) {
        let Some(press) = self.press.as_mut() else { return };
        press.dist += dx.abs() + dy.abs();
        let close_bloom = press.dist > CLICK_SLOP && self.surface_bloom_open && !press.bloom_started;
        if close_bloom {
            self.surface_bloom_open = false;
            press.bloom_started = false;
        }
        if self.pinned_to_target && press.target == PressTarget::Head {
            if press.dist > CLICK_SLOP {
                self.margin_left += dx;
                self.margin_top += dy;
                self.clamp_pinned_margins();
                self.remember_pinned_offset();
                self.reposition();
                self.update_input_region();
            }
            return;
        }
        // A grab is a real drag, not a click: announce it the first time travel
        // crosses the slop. Doing this here (not in on_press) keeps every click from
        // emitting a phantom grabbed+dropped pair.
        let grab_now = self.drag && !press.grabbed_sent && press.dist > CLICK_SLOP;
        if grab_now {
            press.grabbed_sent = true;
        }
        let stretching = press.target == PressTarget::Feet;
        if self.drag {
            self.margin_left += dx;
            self.margin_top += dy;
            self.clamp_margins();
            self.reposition();
        } else if stretching {
            // Dragging the feet stretches/squashes the clay body. Local presentation
            // preference only — nothing is reported to the soul.
            self.set_body_len(self.body_len + dy as f32);
        }
        if grab_now {
            self.emit_grabbed();
        }
        if close_bloom {
            self.update_input_region();
        }
    }

    /// Apply a new torso stretch: clamp, resize the surface to fit, and keep the
    /// input region in step. The compositor acks the new size on the next configure.
    fn set_body_len(&mut self, len: f32) {
        let len = len.clamp(render::BODY_LEN_MIN, render::BODY_LEN_MAX);
        if (len - self.body_len).abs() < 0.5 {
            return;
        }
        self.body_len = len;
        if let Some(layer) = self.layer.as_ref() {
            layer.set_size(self.requested_surface_w(), self.layout().surface_h());
            layer.commit();
        }
        self.update_input_region();
    }

    fn on_release(&mut self, x: f64, y: f64) {
        let Some(press) = self.press.take() else { return };
        self.drag = false;
        let body_x = self.body_hit_x(x);

        if let PressTarget::SurfaceBloom(idx) = press.target {
            self.surface_bloom_open = false;
            if press.dist <= CLICK_SLOP {
                if let Some(desc) = self.surface_bloom_descriptor_at(idx) {
                    self.input_focused = false;
                    self.activate_bloom_descriptor(&desc);
                    return;
                }
            }
            self.update_input_region();
            return;
        }

        if press.bloom_started {
            if let Some(idx) = self.surface_bloom_hit_index(body_x, y) {
                if let Some(desc) = self.surface_bloom_descriptor_at(idx) {
                    self.surface_bloom_open = false;
                    self.input_focused = false;
                    self.activate_bloom_descriptor(&desc);
                    return;
                }
            }
            self.surface_bloom_open = true;
            self.update_input_region();
            return;
        }

        // A click on the tucked bump summons the buddy back out.
        if press.target == PressTarget::Bump {
            if press.dist <= CLICK_SLOP {
                self.summon();
            }
            return;
        }

        if press.secondary {
            if press.dist <= CLICK_SLOP && matches!(press.target, PressTarget::Head | PressTarget::Outside) {
                self.toggle_pin();
            }
            return;
        }

        if self.pinned_to_target && press.target == PressTarget::Head && press.dist > CLICK_SLOP {
            return;
        }

        if press.dist > CLICK_SLOP {
            // A head or body drag ended. If it came to rest near an edge, tuck it there;
            // otherwise report where it landed so the placement can be persisted.
            // (Feet drags are local resizes — nothing to tuck or report.)
            if matches!(press.target, PressTarget::Head | PressTarget::Body) {
                if let Some(edge) = self.nearest_edge_within_threshold() {
                    self.tuck_to(edge);
                } else {
                    self.emit_dropped();
                }
            }
            return;
        }
        match press.target {
            PressTarget::Head => {
                self.emit_clicked();
                self.toggle_chat();
            }
            PressTarget::ReceiptRail(idx) => {
                self.input_focused = false;
                self.show_receipt_rail_entry(idx);
            }
            PressTarget::Input => self.input_focused = true,
            PressTarget::Paste => {
                self.input_focused = true;
                self.paste_clipboard_into_input();
            }
            PressTarget::Review => {
                self.input_focused = false;
                self.request_review();
            }
            PressTarget::Edit => {
                self.input_focused = false;
                self.request_repo_edit();
            }
            PressTarget::Perimeter(id) => {
                self.input_focused = false;
                self.on_perimeter_control(id);
            }
            PressTarget::TorsoAction(action) => {
                self.input_focused = false;
                self.on_torso_action(action);
            }
            PressTarget::Body
            | PressTarget::Feet
            | PressTarget::Bump
            | PressTarget::SurfaceBloom(_)
            | PressTarget::Outside => {
                self.input_focused = false;
            }
        }
    }

    /// The ordered surface list the body navigates. Soul-pushed via `hydrate`; before the
    /// first hydrate it falls back to the `SURFACE_ORDER` seed treated as all-available, so
    /// the perimeter still works on a fresh body that hasn't been hydrated yet.
    fn ordered_surfaces(&self) -> Vec<presence::SurfaceDescriptor> {
        if self.surfaces.is_empty() {
            SURFACE_ORDER
                .iter()
                .map(|id| presence::SurfaceDescriptor {
                    id: (*id).to_string(),
                    label: (*id).to_string(),
                    availability: "available".to_string(),
                    kind: "surface".to_string(),
                    effector: None,
                })
                .collect()
        } else {
            self.surfaces.clone()
        }
    }

    fn surface_bloom_surfaces(&self) -> Vec<presence::SurfaceDescriptor> {
        rotate_surfaces_for_bloom(self.ordered_surfaces(), &self.active_surface)
    }

    fn surface_bloom_hit_index(&self, x: f64, y: f64) -> Option<usize> {
        render::surface_bloom_hit(&self.layout(), self.surface_bloom_surfaces().len(), x, y)
    }

    fn surface_bloom_descriptor_at(&self, idx: usize) -> Option<presence::SurfaceDescriptor> {
        self.surface_bloom_surfaces().get(idx).cloned()
    }

    /// Act on a bloom-dial selection: a launcher opens its external tool through the gate; a
    /// plain surface switches the active surface. Centralises the kind-branch both selection
    /// paths (tap-on-pill and drag-release-on-pill) share.
    fn activate_bloom_descriptor(&mut self, desc: &presence::SurfaceDescriptor) {
        if desc.is_launcher() {
            if let Some(effector) = desc.effector.as_deref() {
                self.request_launch(effector, &desc.label);
            }
        } else {
            self.request_surface(&desc.id);
        }
    }

    fn surface_availability(&self, id: &str) -> &str {
        self.surfaces
            .iter()
            .find(|s| s.id == id)
            .map(|s| s.availability.as_str())
            .unwrap_or("available")
    }

    fn show_receipt_rail_entry(&mut self, idx: usize) {
        if let Some(entry) = self.receipt_rail.get(idx) {
            self.speech = Some(entry.detail_text());
            self.update_input_region();
        }
    }

    fn surface_index(&self) -> usize {
        self.ordered_surfaces()
            .iter()
            .position(|s| s.id == self.active_surface)
            .unwrap_or(0)
    }

    fn request_surface(&mut self, surface: &str) {
        // An `unwired` surface names an effector not yet wired: explain rather than ask the
        // soul to switch (it would no-op anyway). The body only reports availability the soul
        // pushed; it never decides wiring itself (AGENTS.md law 7).
        if self.surface_availability(surface) == "unwired" {
            let label = self
                .surfaces
                .iter()
                .find(|s| s.id == surface)
                .map(|s| s.label.clone())
                .unwrap_or_else(|| surface.to_string());
            self.speech = Some(format!("{label}: not wired yet"));
            self.update_input_region();
            return;
        }
        self.send_to_soul(presence::surface_request_json(&self.buddy, surface));
        self.speech = Some(format!("Requesting surface: {surface}"));
        self.update_input_region();
    }

    fn cycle_surface(&mut self, delta: isize) {
        // Walk the soul-pushed order, skipping `unwired` surfaces so the arrows never dead-end
        // on a surface that can't activate — those stay reachable only via a quick button,
        // which shows the "not wired yet" cue.
        let order = self.ordered_surfaces();
        if let Some(idx) = next_cyclable_index(&order, self.surface_index(), delta) {
            let id = order[idx].id.clone();
            self.request_surface(&id);
        }
    }

    fn on_perimeter_control(&mut self, id: PerimeterId) {
        match id {
            PerimeterId::ArrowN | PerimeterId::ArrowW => self.cycle_surface(-1),
            PerimeterId::ArrowS | PerimeterId::ArrowE => self.cycle_surface(1),
            PerimeterId::Quick0 | PerimeterId::Quick1 | PerimeterId::Quick2 | PerimeterId::Quick3 => {
                let idx = match id {
                    PerimeterId::Quick0 => 0,
                    PerimeterId::Quick1 => 1,
                    PerimeterId::Quick2 => 2,
                    PerimeterId::Quick3 => 3,
                    _ => 0,
                };
                if let Some(surface) = SURFACE_QUICK.get(idx) {
                    self.request_surface(surface);
                }
            }
            PerimeterId::Add => self.request_surface("customize"),
            PerimeterId::Paste | PerimeterId::Review | PerimeterId::Edit => {}
        }
    }

    fn update_surface_bloom_hold(&mut self) {
        let should_open = self
            .press
            .as_ref()
            .is_some_and(|press| should_open_surface_bloom(press, Instant::now()));
        if !should_open || self.surface_bloom_open {
            return;
        }
        if let Some(press) = self.press.as_mut() {
            press.bloom_started = true;
        }
        self.surface_bloom_open = true;
        self.speech = None;
        self.update_input_region();
    }

    /// Ask the soul to run the read-only `receipt_review` effector through the action gate.
    /// First press requests it; once the soul replies `needs_confirmation` the button becomes
    /// Confirm and the next press re-requests with `confirmed`. The body only asks — the soul
    /// authorizes and sends back the ActionReceipt it renders (AGENTS.md law 7).
    fn request_review(&mut self) {
        let confirmed = self.pending_effector.as_deref() == Some("receipt_review");
        self.send_to_soul(presence::action_request_json(
            &self.buddy,
            "receipt_review",
            confirmed,
            None,
        ));
        self.speech = Some(if confirmed {
            "Confirming review…".to_string()
        } else {
            "Requesting receipt review…".to_string()
        });
        self.update_input_region();
    }

    /// Ask the soul to launch an external tool (a reach effector like `open_cursor`). The body
    /// only names the effector — the soul owns the workspace target it opens (AGENTS.md law 7).
    /// First tap requests; the soul replies `needs_confirmation` (which sets `pending_effector`),
    /// and re-tapping the same launcher pill confirms. After the soul remembers the confirmation
    /// for the session, later taps open silently. The body never spawns the process itself.
    fn request_launch(&mut self, effector: &str, label: &str) {
        let confirmed = self.pending_effector.as_deref() == Some(effector);
        self.send_to_soul(presence::action_request_json(&self.buddy, effector, confirmed, None));
        self.speech = Some(if confirmed {
            format!("Opening {label}…")
        } else {
            format!("Open {label}?")
        });
        self.update_input_region();
    }

    /// Ask the soul to run the `repo_edit` act-effector with a TYPED ActionIntent aimed at a
    /// sandbox proof path. The body builds the intent from this fixed affordance — it never parses
    /// free text into one — and emits it; the soul authorizes through the action gate and runs the
    /// live executor only on `allow` (AGENTS.md law 7). First press proposes the effect; once the
    /// soul replies needs_confirmation the Edit button becomes Confirm and the next press re-emits
    /// it `confirmed`. This is the inbound half of the membrane the soul→body outcome mirrors.
    fn request_repo_edit(&mut self) {
        const PROOF_TARGET: &str = ".border-agents/proofs/from-body.md";
        let confirmed = self.pending_effector.as_deref() == Some("repo_edit");
        let intent = presence::ActionIntent {
            operation: "write_patch",
            target_kind: "repo_path",
            target_value: Some(PROOF_TARGET),
            summary: Some("write a proof note from the body"),
            payload_digest: None,
        };
        self.send_to_soul(presence::action_request_intent_json(
            &self.buddy,
            "repo_edit",
            &intent,
            confirmed,
            None,
        ));
        self.speech = Some(if confirmed {
            "Confirming repo edit…".to_string()
        } else {
            format!("Requesting repo_edit on {PROOF_TARGET}…")
        });
        self.update_input_region();
    }

    fn on_torso_action(&mut self, action: TorsoAction) {
        self.speech = Some(match action {
            TorsoAction::Expand => "Fullscreen image open will land here.".to_string(),
            TorsoAction::Copy => match self.current_text_output() {
                Some(text) => match copy_to_clipboard(text) {
                    Ok(()) => "Copied text output.".to_string(),
                    Err(err) => format!("Copy failed: {err}"),
                },
                None => "No text output to copy.".to_string(),
            },
            TorsoAction::Scroll => "Torso scroll controls will land here.".to_string(),
        });
        self.update_input_region();
    }

    fn current_text_output(&self) -> Option<&str> {
        match &self.torso_surface {
            TorsoSurface::Text { body, .. } if !body.trim().is_empty() => Some(body.as_str()),
            _ => None,
        }
    }

    // --- tuck / summon -------------------------------------------------------

    /// If the head currently sits within `TUCK_THRESHOLD` of a screen edge, which edge
    /// (the nearest). Needs known screen bounds; without them, never tucks.
    fn nearest_edge_within_threshold(&self) -> Option<presence::Edge> {
        let (sw, sh) = self.screen?;
        let head = render::head_rect();
        let left = self.margin_left + head.x as f64;
        let top = self.margin_top + head.y as f64;
        let right = sw - (self.margin_left + (head.x + head.w) as f64);
        let bottom = sh - (self.margin_top + (head.y + head.h) as f64);

        let candidates = [
            (presence::Edge::Left, left),
            (presence::Edge::Right, right),
            (presence::Edge::Top, top),
            (presence::Edge::Bottom, bottom),
        ];
        candidates
            .into_iter()
            .filter(|(_, d)| *d < TUCK_THRESHOLD)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(edge, _)| edge)
    }

    /// The along-edge coordinate (screen px) of the head's centre — what we report and
    /// persist so the bump returns to the same spot along the edge.
    fn tuck_along(&self, edge: presence::Edge) -> f64 {
        match edge {
            presence::Edge::Left | presence::Edge::Right => self.margin_top + render::HEAD_CY as f64,
            presence::Edge::Top | presence::Edge::Bottom => self.margin_left + render::FIG_CX as f64,
        }
    }

    /// Tuck against `edge` from a user drag: enter the tucked state, then tell the soul
    /// (dropped = persist tucked placement; dismissed = lifecycle "now away").
    fn tuck_to(&mut self, edge: presence::Edge) {
        let along = self.tuck_along(edge);
        self.enter_tuck(edge, along);
        self.send_to_soul(presence::dropped_tucked_json(&self.buddy, edge, along));
        self.send_to_soul(presence::dismissed_json(&self.buddy));
    }

    /// Enter (or restore, from a hydrate) the tucked state: snap the surface flush to
    /// the edge, place the bump at `along`, shrink the input region to the bump.
    fn enter_tuck(&mut self, edge: presence::Edge, along: f64) {
        self.chat_open = false;
        self.input_focused = false;
        self.speech = None;
        self.tucked = Some(edge);
        let (sw, sh) = self.screen.unwrap_or((f64::MAX, f64::MAX));
        let (surface_w, surface_h) = self.requested_surface_size();
        if let Some(layer) = self.layer.as_ref() {
            // Size for the FULL figure (rail included), not the bump — so a later summon
            // never has to grow the surface back and the receipt rail can't be clipped
            // when the buddy pops out stretched. Tucked rendering only paints the bump.
            layer.set_size(self.requested_surface_w(), self.layout().surface_h());
            layer.commit();
        }

        // Flush axis → snap the surface to the edge; along axis → from `along`.
        match edge {
            presence::Edge::Left => {
                self.margin_left = 0.0;
                self.margin_top = along - render::HEAD_CY as f64;
            }
            presence::Edge::Right => {
                self.margin_left = sw - surface_w;
                self.margin_top = along - render::HEAD_CY as f64;
            }
            presence::Edge::Top => {
                self.margin_top = 0.0;
                self.margin_left = along - render::FIG_CX as f64;
            }
            presence::Edge::Bottom => {
                self.margin_top = sh - surface_h;
                self.margin_left = along - render::FIG_CX as f64;
            }
        }
        self.clamp_tucked(edge);
        self.update_input_region();
        self.reposition();
    }

    /// Summon the buddy back out of a tuck (user clicked the bump): pop the full figure
    /// inward off the edge, restore full input, and tell the soul.
    fn summon(&mut self) {
        let Some(edge) = self.tucked.take() else { return };
        let fig = render::figure_bbox(self.body_len);
        let inset = TUCK_THRESHOLD; // land clear of the tuck zone so it doesn't re-tuck
        let (sw, sh) = self.screen.unwrap_or((f64::MAX, f64::MAX));
        // Place the whole figure bbox just inside the edge it was tucked against, so the
        // body (not only the head) pops fully on-screen — the receipt-rail/expanded case
        // the old head-only placement could leave half-clipped.
        match edge {
            presence::Edge::Left => self.margin_left = -(fig.x as f64) + inset,
            presence::Edge::Right => {
                self.margin_left = sw - (fig.x + fig.w) as f64 - inset;
            }
            presence::Edge::Top => self.margin_top = -(fig.y as f64) + inset,
            presence::Edge::Bottom => {
                self.margin_top = sh - (fig.y + fig.h) as f64 - inset;
            }
        }
        // Restore the full-figure surface (rail included) — tucked sizing kept it at the
        // full extent, but reassert so a stretched body with the receipt rail visible can
        // never pop out into a surface too narrow to hold the rail (the clip bug).
        self.set_layer_size(self.requested_surface_w(), self.layout().surface_h());
        self.clamp_margins();
        self.update_input_region();
        self.reposition();
        self.send_to_soul(presence::summoned_json(&self.buddy));
    }

    /// Keep the *bump* on-screen along its free axis (the flush axis is pinned to the
    /// edge). The untucked `clamp_margins` keeps the head on-screen instead.
    fn clamp_tucked(&mut self, edge: presence::Edge) {
        let (sw, sh) = match self.screen {
            Some(s) => s,
            None => return,
        };
        let bump = self.tucked_bump_rect(edge);
        match edge {
            presence::Edge::Left | presence::Edge::Right => {
                let min_top = -(bump.y as f64);
                let max_top = (sh - (bump.y + bump.h) as f64).max(min_top);
                self.margin_top = self.margin_top.clamp(min_top, max_top);
            }
            presence::Edge::Top | presence::Edge::Bottom => {
                let min_left = -(bump.x as f64);
                let max_left = (sw - (bump.x + bump.w) as f64).max(min_left);
                self.margin_left = self.margin_left.clamp(min_left, max_left);
            }
        }
    }

    // --- presence: report this body's own interaction to the soul ---

    /// Best-effort push of a to-soul event. Dropped silently if the presence thread
    /// isn't up or no soul is connected — the body never depends on being observed.
    fn send_to_soul(&self, json: String) {
        if let Some(tx) = self.presence_out.as_ref() {
            let _ = tx.send(json);
        }
    }

    fn emit_clicked(&self) {
        self.send_to_soul(presence::clicked_json(&self.buddy, self.margin_left, self.margin_top));
    }

    fn emit_grabbed(&self) {
        self.send_to_soul(presence::grabbed_json(&self.buddy, self.margin_left, self.margin_top));
    }

    fn emit_dropped(&self) {
        self.send_to_soul(presence::dropped_json(&self.buddy, self.margin_left, self.margin_top));
    }
}

impl CompositorHandler for App {
    fn scale_factor_changed(&mut self, _c: &Connection, _q: &QueueHandle<Self>, _s: &wl_surface::WlSurface, _n: i32) {}
    fn transform_changed(&mut self, _c: &Connection, _q: &QueueHandle<Self>, _s: &wl_surface::WlSurface, _t: wl_output::Transform) {}
    fn frame(&mut self, _c: &Connection, _q: &QueueHandle<Self>, _s: &wl_surface::WlSurface, _time: u32) {}
    /// The compositor's authoritative word on which output the surface actually
    /// occupies. The startup guess (`outputs().next()`) can name the wrong monitor on
    /// a multi-output desktop — e.g. clamping to the laptop panel while the buddy
    /// renders on an external screen, which leaves a dead invisible border. Trust
    /// `enter` over the guess, and re-derive the clamp bounds from this output. Also
    /// keeps clamping correct when the buddy is later dragged across monitors.
    fn surface_enter(&mut self, _c: &Connection, _q: &QueueHandle<Self>, _s: &wl_surface::WlSurface, o: &wl_output::WlOutput) {
        let Some(info) = self.output_state.info(o) else { return };
        let size = info
            .logical_size
            .or_else(|| info.modes.iter().find(|m| m.current).map(|m| m.dimensions))
            .map(|(w, h)| (w as f64, h as f64));
        let Some(new_screen) = size else { return };
        if self.screen == Some(new_screen) {
            return;
        }
        eprintln!(
            "[bb-desktop-body] surface entered output {:?}; clamp bounds {:?} -> {:?}",
            info.name, self.screen, new_screen,
        );
        self.screen = Some(new_screen);
        // Re-clamp against the real screen and push the corrected margins, so a buddy
        // that started life clamped to the wrong output snaps onto this one.
        if let Some(edge) = self.tucked {
            self.clamp_tucked(edge);
        } else {
            self.clamp_margins();
        }
        self.reposition();
    }
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
        let size_changed = (nw != 0 && nw != self.width) || (nh != 0 && nh != self.height);
        if nw != 0 && nh != 0 && size_changed && debug_log_enabled() {
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
        if let Some(edge) = self.tucked {
            self.clamp_tucked(edge);
            self.update_input_region();
            self.reposition();
        } else if first || size_changed {
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
        if capability == Capability::Keyboard && self.keyboard.is_none() {
            match self.seat_state.get_keyboard(qh, &seat, None) {
                Ok(keyboard) => self.keyboard = Some(keyboard),
                Err(err) => eprintln!("[bb-desktop-body] could not get keyboard: {err}"),
            }
        }
    }
    fn remove_capability(&mut self, _c: &Connection, _q: &QueueHandle<Self>, _s: wl_seat::WlSeat, capability: Capability) {
        if capability == Capability::Pointer {
            if let Some(pointer) = self.pointer.take() {
                pointer.release();
            }
        }
        if capability == Capability::Keyboard {
            if let Some(keyboard) = self.keyboard.take() {
                keyboard.release();
            }
        }
    }
    fn remove_seat(&mut self, _c: &Connection, _q: &QueueHandle<Self>, _s: wl_seat::WlSeat) {}
}

impl KeyboardHandler for App {
    fn enter(
        &mut self,
        _c: &Connection,
        _q: &QueueHandle<Self>,
        _kb: &wl_keyboard::WlKeyboard,
        _surface: &wl_surface::WlSurface,
        _serial: u32,
        _raw: &[u32],
        _keysyms: &[Keysym],
    ) {
    }
    fn leave(
        &mut self,
        _c: &Connection,
        _q: &QueueHandle<Self>,
        _kb: &wl_keyboard::WlKeyboard,
        _surface: &wl_surface::WlSurface,
        _serial: u32,
    ) {
        // Lost keyboard focus (user clicked another window): drop the caret.
        self.input_focused = false;
    }
    fn press_key(
        &mut self,
        _c: &Connection,
        _q: &QueueHandle<Self>,
        _kb: &wl_keyboard::WlKeyboard,
        _serial: u32,
        event: KeyEvent,
    ) {
        if self.input_focused {
            self.on_key(event);
        }
    }
    fn release_key(
        &mut self,
        _c: &Connection,
        _q: &QueueHandle<Self>,
        _kb: &wl_keyboard::WlKeyboard,
        _serial: u32,
        _event: KeyEvent,
    ) {
    }
    fn update_modifiers(
        &mut self,
        _c: &Connection,
        _q: &QueueHandle<Self>,
        _kb: &wl_keyboard::WlKeyboard,
        _serial: u32,
        modifiers: Modifiers,
        _layout: u32,
    ) {
        self.modifiers = modifiers;
    }
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
                PointerEventKind::Press { button, .. } => self.on_press(px, py, button),
                PointerEventKind::Release { .. } => self.on_release(px, py),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn perimeter_controls_filter_chat_buttons_with_chat_state() {
        let layout = render::Layout::initial();
        let closed = active_perimeter_controls_for(false, layout);
        let open = active_perimeter_controls_for(true, layout);

        assert!(!closed.iter().any(|(id, _)| matches!(id, PerimeterId::Paste | PerimeterId::Review | PerimeterId::Edit)));
        assert!(open.iter().any(|(id, _)| *id == PerimeterId::Paste));
        assert!(open.iter().any(|(id, _)| *id == PerimeterId::Review));
        assert!(open.iter().any(|(id, _)| *id == PerimeterId::Edit));
        assert_eq!(open.len(), layout.perimeter_controls().len());
    }

    fn surf(id: &str, availability: &str) -> presence::SurfaceDescriptor {
        presence::SurfaceDescriptor {
            id: id.to_string(),
            label: id.to_string(),
            availability: availability.to_string(),
            kind: "surface".to_string(),
            effector: None,
        }
    }

    fn launcher(id: &str, effector: &str) -> presence::SurfaceDescriptor {
        presence::SurfaceDescriptor {
            id: id.to_string(),
            label: id.to_string(),
            availability: "gated".to_string(),
            kind: "launcher".to_string(),
            effector: Some(effector.to_string()),
        }
    }

    fn receipt(idx: usize) -> ReceiptRailEntry {
        ReceiptRailEntry {
            effector: format!("effector_{idx}"),
            decision: "allow".to_string(),
            ts: 1_800_000 + idx as u64,
            summary: Some(format!("summary {idx}")),
            route_label: Some("claude".to_string()),
            executed: Some(true),
            receipt_id: format!("receipt-{idx}"),
        }
    }

    #[test]
    fn receipt_rail_ring_buffer_keeps_last_twenty() {
        let mut entries = VecDeque::new();
        for idx in 0..25 {
            push_receipt_rail_entry(&mut entries, receipt(idx));
        }
        assert_eq!(entries.len(), RECEIPT_RAIL_CAP);
        assert_eq!(entries.front().unwrap().receipt_id, "receipt-24");
        assert_eq!(entries.back().unwrap().receipt_id, "receipt-5");
    }

    #[test]
    fn receipt_status_glyph_is_closed_and_fails_loud() {
        assert_eq!(receipt_status_glyph("allow", Some(true)), "✅");
        assert_eq!(receipt_status_glyph("allow", None), "✅");
        assert_eq!(receipt_status_glyph("allow", Some(false)), "☑");
        assert_eq!(receipt_status_glyph("needs_confirmation", None), "⏳");
        assert_eq!(receipt_status_glyph("blocked", None), "❌");
        assert_eq!(receipt_status_glyph("maybe", Some(true)), "❌");
        assert_ne!(receipt_status_glyph("allow", None), receipt_status_glyph("blocked", None));
        assert_ne!(receipt_status_glyph("allow", Some(false)), receipt_status_glyph("blocked", None));
    }

    #[test]
    fn receipt_detail_reuses_summary_without_derivation() {
        let entry = ReceiptRailEntry {
            effector: "repo_edit".to_string(),
            decision: "allow".to_string(),
            ts: 3_723,
            summary: Some("Applied the patch.".to_string()),
            route_label: None,
            executed: Some(true),
            receipt_id: "action:1".to_string(),
        };
        let detail = entry.detail_text();
        assert!(detail.contains("Applied the patch."));
        assert!(detail.contains("allow repo_edit at 01:02:03."));
        assert!(detail.contains("receiptId: action:1"));

        let no_summary = ReceiptRailEntry { summary: None, ..entry };
        let detail = no_summary.detail_text();
        assert!(!detail.contains("Applied the patch."));
        assert!(detail.contains("allow repo_edit at 01:02:03."));
    }

    #[test]
    fn cycle_skips_unwired_surfaces() {
        let order = vec![
            surf("session", "available"),
            surf("claude_code", "unwired"),
            surf("agent_zero", "gated"),
        ];
        // Forward from session (0) skips the unwired claude_code and lands on agent_zero (2).
        assert_eq!(next_cyclable_index(&order, 0, 1), Some(2));
        // Backward from session (0) wraps to agent_zero (2), still skipping the unwired one.
        assert_eq!(next_cyclable_index(&order, 0, -1), Some(2));
        // From agent_zero (2) forward wraps past the unwired entry back to session (0).
        assert_eq!(next_cyclable_index(&order, 2, 1), Some(0));
    }

    #[test]
    fn cycle_yields_none_when_all_unwired_or_empty() {
        assert_eq!(next_cyclable_index(&[], 0, 1), None);
        let all_unwired = vec![surf("a", "unwired"), surf("b", "unwired")];
        assert_eq!(next_cyclable_index(&all_unwired, 0, 1), None);
    }

    #[test]
    fn cycle_skips_launchers_so_they_stay_bloom_only() {
        // Launchers ride the bloom dial but the arrow cycle must step over them — arrows switch
        // surfaces, they never open a tool. Forward/backward from session both skip open_cursor.
        let order = vec![
            surf("session", "available"),
            launcher("open_cursor", "open_cursor"),
            surf("agent_zero", "gated"),
        ];
        assert_eq!(next_cyclable_index(&order, 0, 1), Some(2));
        assert_eq!(next_cyclable_index(&order, 0, -1), Some(2));
        // A dial of only launchers has nothing to cycle to.
        let only_launchers = vec![launcher("open_cursor", "open_cursor"), launcher("open_vscode", "open_vscode")];
        assert_eq!(next_cyclable_index(&only_launchers, 0, 1), None);
    }

    #[test]
    fn bloom_order_rotates_active_surface_to_twelve_oclock() {
        let order = vec![
            surf("session", "available"),
            surf("private_local_chat", "gated"),
            surf("claude_code", "unwired"),
            surf("live_hermes", "unwired"),
            surf("agent_zero", "unwired"),
            surf("customize", "available"),
        ];
        let rotated = rotate_surfaces_for_bloom(order, "claude_code");
        let ids: Vec<&str> = rotated.iter().map(|surface| surface.id.as_str()).collect();
        assert_eq!(ids, vec!["claude_code", "live_hermes", "agent_zero", "customize", "session", "private_local_chat"]);
    }

    #[test]
    fn hold_to_bloom_requires_surface_press_time_and_stillness() {
        let now = Instant::now();
        let old_enough = now.checked_sub(SURFACE_BLOOM_HOLD + Duration::from_millis(1)).unwrap();
        let fresh = now.checked_sub(SURFACE_BLOOM_HOLD - Duration::from_millis(1)).unwrap();
        let base = PressState {
            target: PressTarget::Perimeter(PerimeterId::ArrowN),
            secondary: false,
            started_at: old_enough,
            dist: 0.0,
            grabbed_sent: false,
            bloom_started: false,
        };

        assert!(should_open_surface_bloom(&base, now));
        assert!(!should_open_surface_bloom(&PressState { started_at: fresh, ..base }, now));
        assert!(!should_open_surface_bloom(&PressState { dist: CLICK_SLOP + 0.1, ..base }, now));
        assert!(!should_open_surface_bloom(&PressState { secondary: true, ..base }, now));
        assert!(!should_open_surface_bloom(&PressState { target: PressTarget::Head, ..base }, now));
        assert!(!should_open_surface_bloom(&PressState { target: PressTarget::Perimeter(PerimeterId::Add), ..base }, now));
        assert!(!should_open_surface_bloom(&PressState { target: PressTarget::Paste, ..base }, now));
        assert!(!should_open_surface_bloom(&PressState { target: PressTarget::Review, ..base }, now));
        assert!(!should_open_surface_bloom(&PressState { target: PressTarget::Edit, ..base }, now));
        assert!(!should_open_surface_bloom(&PressState { bloom_started: true, ..base }, now));
    }

    #[test]
    fn perimeter_surface_controls_have_clickable_rects() {
        let layout = render::Layout::initial();
        let controls = active_perimeter_controls_for(false, layout);
        for id in [PerimeterId::ArrowN, PerimeterId::ArrowE, PerimeterId::ArrowS, PerimeterId::ArrowW, PerimeterId::Quick0, PerimeterId::Add] {
            let rect = controls.iter().find_map(|(candidate, rect)| (*candidate == id).then_some(*rect)).expect("control exists");
            assert!(rect.w > 0.0 && rect.h > 0.0, "{id:?} should have area");
        }
    }

    #[test]
    fn clamp_keeps_a_sliver_of_the_figure_visible_at_every_edge() {
        // The real figure bbox (default stretch): roughly x∈[141,419], y∈[14,284].
        let fig = render::figure_bbox(render::BODY_LEN_DEFAULT);
        let keep = render::DRAG_KEEP_VISIBLE as f64;
        let sw = 1920.0;
        let sh = 1080.0;

        // Dragged fully off the right: clamp pulls the left edge back so `keep` px stays visible.
        let (left, _) = clamp_figure_margins(5000.0, 0.0, fig, (sw, sh), keep);
        assert_eq!(left, sw - keep - fig.x as f64);
        assert!(left + fig.x as f64 <= sw - keep + 0.5);
        assert!(left + (fig.x + fig.w) as f64 >= sw - keep - 0.5, "a sliver must remain visible");

        // Dragged fully off the left: clamp lets the margin go negative so `keep` px stays visible.
        let (left, _) = clamp_figure_margins(-5000.0, 0.0, fig, (sw, sh), keep);
        assert_eq!(left, keep - (fig.x + fig.w) as f64);
        assert!(left + (fig.x + fig.w) as f64 >= keep - 0.5);
        assert!(left + fig.x as f64 <= keep + 0.5, "a sliver must remain visible");

        // Dragged fully off the bottom and top.
        let (_, top) = clamp_figure_margins(0.0, 5000.0, fig, (sw, sh), keep);
        assert_eq!(top, sh - keep - fig.y as f64);
        let (_, top) = clamp_figure_margins(0.0, -5000.0, fig, (sw, sh), keep);
        assert_eq!(top, keep - (fig.y + fig.h) as f64);
    }

    #[test]
    fn clamp_degrades_safely_when_screen_smaller_than_keep_sliver() {
        // A screen smaller than the keep-visible budget must still clamp to a single in-bounds
        // value rather than panicking or inverting the min/max range.
        let fig = render::figure_bbox(render::BODY_LEN_DEFAULT);
        let tiny = (40.0, 40.0);
        let (left, top) = clamp_figure_margins(10_000.0, 10_000.0, fig, tiny, render::DRAG_KEEP_VISIBLE as f64);
        // min == max on a too-small screen, so the clamp pins the margin to that single value.
        assert!(left.is_finite() && top.is_finite());
    }
}

delegate_compositor!(App);
delegate_output!(App);
delegate_shm!(App);
delegate_seat!(App);
delegate_keyboard!(App);
delegate_pointer!(App);
delegate_layer!(App);
delegate_registry!(App);
