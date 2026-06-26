//! Border-target geometry driver + commandeer mechanism (build-order "step b", extended).
//!
//! A standalone process that speaks the presence protocol. It tracks native COSMIC windows
//! via `cosmic-toplevel-info` and emits:
//!   • `target_acquired` / `target_moved` / `target_lost` — the framed window's geometry, and
//!   • `targets_available` — the full enumerated window list (the pin-picker's data).
//!
//! It also RECEIVES soul→driver commands over the same gateway WebSocket (the gateway is a
//! broadcast relay, so a `commandeer` cue minted by the soul reaches this client). A
//! `commandeer` command carries a `targetId` + `mode`:
//!   • `pin`     — frame/follow that window (sets the tracked target; body wraps it).
//!   • `monitor` — raise it to the front (read posture; vision capture is a later effector).
//!   • `control` — raise it AND type `text` into it via the virtual keyboard.
//!
//! Activation + keystroke injection are the proven primitives from the commandeer probe,
//! folded in here behind the soul's authorization. THE DRIVER IS STILL ONLY THE MECHANISM:
//! it acts only on a command the soul minted through its action gate (AGENTS.md law 7 —
//! bodies/drivers present + act on instruction; souls decide). `zwp_virtual_keyboard` has no
//! OS consent dialog, so that gate is the entire safety boundary.
//!
//! Geometry arrives per-output (relative to the output's origin); we add each output's
//! xdg-output logical position to lift it into one global logical coordinate space — the
//! canonical `TargetBounds` space the body consumes, with `scaleFactor` carried alongside.
//!
//! By design this is COSMIC-only. A Windows/macOS port is a *different* binary emitting the
//! same envelopes and honoring the same commands; the body never learns which one produced a
//! rectangle. Output goes to stdout always, and to the gateway WebSocket (`BB_PRESENCE_URL`).

use std::collections::HashMap;
use std::io::{Seek, SeekFrom, Write};
use std::net::TcpStream;
use std::os::fd::AsFd;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use calloop::channel::Event as ChannelEvent;
use calloop::EventLoop;
use calloop_wayland_source::WaylandSource;

use wayland_client::globals::{registry_queue_init, GlobalListContents};
use wayland_client::protocol::{wl_output, wl_registry, wl_seat};
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle};

use wayland_protocols::ext::foreign_toplevel_list::v1::client::{
    ext_foreign_toplevel_handle_v1 as ext_handle, ext_foreign_toplevel_list_v1 as ext_list,
};
use wayland_protocols::xdg::xdg_output::zv1::client::{
    zxdg_output_manager_v1 as xdg_mgr, zxdg_output_v1 as xdg_output,
};

use cosmic_protocols::toplevel_info::v1::client::{
    zcosmic_toplevel_handle_v1 as cosmic_handle, zcosmic_toplevel_info_v1 as cosmic_info,
};
use cosmic_protocols::toplevel_management::v1::client::zcosmic_toplevel_manager_v1 as cosmic_mgr;

use wayland_protocols_misc::zwp_virtual_keyboard_v1::client::{
    zwp_virtual_keyboard_manager_v1 as vk_mgr, zwp_virtual_keyboard_v1 as vk,
};

use serde_json::Value;
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{connect, Message, WebSocket};

const DEFAULT_URL: &str = "ws://127.0.0.1:17387/border-buddies";

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

// --- gateway link (bidirectional) ----------------------------------------------
//
// Mirrors the body's `presence.rs`: a blocking tungstenite connection on its own thread,
// bridged into the driver's calloop loop. Inbound frames are pushed to the calloop channel
// (soul→driver commands); outbound target cues are drained from an mpsc queue. Best-effort:
// a down gateway never stops the driver — cues still go to stdout, and the thread retries.

const INITIAL_BACKOFF: Duration = Duration::from_millis(500);
const MAX_BACKOFF: Duration = Duration::from_secs(5);
const READ_TIMEOUT: Duration = Duration::from_millis(200);

/// Spawn the link thread. Returns the outbound sender the driver pushes target cues to;
/// `inbound` forwards raw JSON frames into the calloop loop.
fn spawn_link(inbound: calloop::channel::Sender<String>) -> mpsc::Sender<String> {
    let url = std::env::var("BB_PRESENCE_URL").unwrap_or_else(|_| DEFAULT_URL.to_string());
    let (out_tx, out_rx) = mpsc::channel::<String>();
    if let Err(err) = thread::Builder::new()
        .name("bb-driver-link".into())
        .spawn(move || link_run(url, inbound, out_rx))
    {
        eprintln!("[driver] could not start link thread: {err} — stdout only");
    }
    out_tx
}

fn link_run(url: String, inbound: calloop::channel::Sender<String>, out_rx: mpsc::Receiver<String>) {
    let mut backoff = INITIAL_BACKOFF;
    loop {
        match connect(&url) {
            Ok((mut socket, _resp)) => {
                eprintln!("[driver] gateway connected: {url}");
                backoff = INITIAL_BACKOFF;
                while out_rx.try_recv().is_ok() {} // drop stale backlog
                set_read_timeout(&socket, Some(READ_TIMEOUT));
                if pump(&mut socket, &inbound, &out_rx).is_break() {
                    return; // calloop gone — driver exiting
                }
            }
            Err(err) => eprintln!("[driver] gateway unavailable ({url}): {err} — retry {backoff:?}"),
        }
        thread::sleep(backoff);
        backoff = (backoff * 2).min(MAX_BACKOFF);
    }
}

fn pump(
    socket: &mut WebSocket<MaybeTlsStream<TcpStream>>,
    inbound: &calloop::channel::Sender<String>,
    out_rx: &mpsc::Receiver<String>,
) -> std::ops::ControlFlow<()> {
    use std::io::ErrorKind;
    use std::ops::ControlFlow;
    loop {
        match socket.read() {
            Ok(Message::Text(text)) => {
                if inbound.send(text.to_string()).is_err() {
                    return ControlFlow::Break(());
                }
            }
            Ok(Message::Close(_)) => return ControlFlow::Continue(()),
            Ok(_) => {}
            Err(tungstenite::Error::Io(e))
                if e.kind() == ErrorKind::WouldBlock || e.kind() == ErrorKind::TimedOut =>
            {
                if !drain_outbound(socket, out_rx) {
                    return ControlFlow::Continue(());
                }
            }
            Err(err) => {
                eprintln!("[driver] gateway read error: {err} — reconnecting");
                return ControlFlow::Continue(());
            }
        }
    }
}

fn drain_outbound(
    socket: &mut WebSocket<MaybeTlsStream<TcpStream>>,
    out_rx: &mpsc::Receiver<String>,
) -> bool {
    loop {
        match out_rx.try_recv() {
            Ok(text) => {
                if socket.send(Message::Text(text.into())).is_err() {
                    return false;
                }
            }
            Err(_) => return true,
        }
    }
}

fn set_read_timeout(socket: &WebSocket<MaybeTlsStream<TcpStream>>, dur: Option<Duration>) {
    if let MaybeTlsStream::Plain(stream) = socket.get_ref() {
        let _ = stream.set_read_timeout(dur);
    }
}

// --- commandeer primitive (keymap + injection) ---------------------------------
//
// Proven in the commandeer/kbd_inject probes: a layout-independent keymap with one keycode
// per char carrying its Unicode keysym. xkb code `9 + i` ⇒ evdev wire code `i + 1`.

fn build_keymap(s: &str) -> String {
    let mut codes = String::new();
    let mut syms = String::new();
    for (i, c) in s.chars().enumerate() {
        codes.push_str(&format!("    <K{i}> = {};\n", 9 + i));
        syms.push_str(&format!("    key <K{i}> {{ [ U{:04X} ] }};\n", c as u32));
    }
    format!(
        "xkb_keymap {{\n\
         xkb_keycodes \"(driver)\" {{\n    minimum = 8;\n    maximum = 255;\n{codes}}};\n\
         xkb_types \"(driver)\" {{ include \"complete\" }};\n\
         xkb_compatibility \"(driver)\" {{ include \"complete\" }};\n\
         xkb_symbols \"(driver)\" {{\n{syms}}};\n\
         }};\n"
    )
}

/// An anonymous, writable temp file to hand the compositor as the keymap fd — unlinked
/// immediately; the open fd keeps it alive until it's dropped.
fn anon_file() -> std::fs::File {
    let mut path = std::env::temp_dir();
    path.push(format!("bb-driver-{}.xkb", std::process::id()));
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(&path)
        .expect("open temp keymap file");
    let _ = std::fs::remove_file(&path);
    file
}

#[derive(Default, Clone)]
struct OutputInfo {
    logical_x: i32,
    logical_y: i32,
    scale: i32,
}

struct Toplevel {
    title: String,
    app_id: String,
    cosmic: Option<cosmic_handle::ZcosmicToplevelHandleV1>,
    /// Last emitted global-logical bounds + scale, to suppress duplicate geometry events.
    last: Option<(i32, i32, i32, i32, i32)>,
}

struct App {
    conn: Connection,
    qh: QueueHandle<App>,
    info: cosmic_info::ZcosmicToplevelInfoV1,
    xdg_mgr: xdg_mgr::ZxdgOutputManagerV1,
    seat: wl_seat::WlSeat,
    manager: cosmic_mgr::ZcosmicToplevelManagerV1,
    vkm: vk_mgr::ZwpVirtualKeyboardManagerV1,
    keyboard: Option<vk::ZwpVirtualKeyboardV1>,
    out: mpsc::Sender<String>,
    buddy: String,
    target: String,
    toplevels: HashMap<u32, Toplevel>, // keyed by ext handle id
    cosmic_to_ext: HashMap<u32, u32>,
    outputs: HashMap<u32, OutputInfo>, // keyed by wl_output id
    acquired: Option<u32>,             // ext handle id we are currently framing
    last_targets_sig: String,          // dedup the targets_available snapshot
}

impl App {
    fn matches(&self, t: &Toplevel) -> bool {
        let n = self.target.to_lowercase();
        !n.is_empty() && (t.title.to_lowercase().contains(&n) || t.app_id.to_lowercase().contains(&n))
    }

    fn emit(&mut self, json: String) {
        println!("{json}");
        let _ = self.out.send(json);
    }

    /// The detected-window list (pin-picker data). Emitted on identity change / close, deduped.
    fn emit_targets_available(&mut self) {
        let mut entries: Vec<(u32, String, String)> = self
            .toplevels
            .iter()
            .filter(|(_, t)| !t.title.is_empty() || !t.app_id.is_empty())
            .map(|(id, t)| (*id, t.title.clone(), t.app_id.clone()))
            .collect();
        entries.sort_by_key(|(id, _, _)| *id);

        let items: Vec<String> = entries
            .iter()
            .map(|(id, title, app_id)| {
                format!(r#"{{"targetId":"{id}","title":{title:?},"appId":{app_id:?}}}"#)
            })
            .collect();
        let targets = items.join(",");
        if targets == self.last_targets_sig {
            return; // unchanged — don't spam the wire
        }
        self.last_targets_sig = targets.clone();
        let ts = now_ms();
        let buddy = self.buddy.clone();
        self.emit(format!(
            r#"{{"protocol":"presence","v":0,"kind":"targets_available","buddy":"{buddy}","ts":{ts},"targets":[{targets}]}}"#
        ));
    }

    /// A toplevel's identity changed: (re)evaluate the BB_TARGET auto-pick, then refresh the
    /// available list. First match wins for the startup auto-pick; the soul can retarget via
    /// a `commandeer` `pin` command at any time.
    fn reconsider(&mut self, ext_id: u32) {
        self.emit_targets_available();
        if self.acquired.is_some() {
            return;
        }
        let Some(t) = self.toplevels.get(&ext_id) else {
            return;
        };
        if (t.title.is_empty() && t.app_id.is_empty()) || !self.matches(t) {
            return;
        }
        self.set_target(ext_id);
    }

    /// Frame/follow `ext_id` (the `pin` gesture). Replaces any prior target. Mints
    /// `target_acquired` from the last known geometry if we have it, else the next geometry
    /// event acquires (mirrors the original replay logic).
    fn set_target(&mut self, ext_id: u32) {
        if self.acquired == Some(ext_id) || !self.toplevels.contains_key(&ext_id) {
            return;
        }
        if let Some(old) = self.acquired.take() {
            let ts = now_ms();
            let buddy = self.buddy.clone();
            self.emit(format!(
                r#"{{"protocol":"presence","v":0,"kind":"target_lost","buddy":"{buddy}","ts":{ts},"targetId":"{old}","reason":"trackingFailed"}}"#
            ));
            if let Some(t) = self.toplevels.get_mut(&old) {
                t.last = None;
            }
        }
        self.acquired = Some(ext_id);
        if let Some(t) = self.toplevels.get(&ext_id) {
            eprintln!("[driver] pinned '{}' / '{}'", t.title, t.app_id);
        }
        if let Some((gx, gy, w, h, scale)) = self.toplevels.get(&ext_id).and_then(|t| t.last) {
            self.toplevels.get_mut(&ext_id).unwrap().last = None;
            self.emit_geometry(ext_id, gx, gy, w, h, scale);
        }
    }

    /// Raise + focus a window via the cosmic toplevel manager (the `monitor`/`control` lead-in).
    fn activate(&mut self, ext_id: u32) {
        let Some(cosmic) = self.toplevels.get(&ext_id).and_then(|t| t.cosmic.clone()) else {
            return;
        };
        eprintln!("[driver] activating target {ext_id}");
        self.manager.activate(&cosmic, &self.seat);
        let _ = self.conn.flush();
    }

    /// Type `text` into whatever now holds focus (we activate first). The keyboard is created
    /// once and re-keymapped per string (the keymap encodes that specific text).
    fn inject_text(&mut self, text: &str) {
        if self.keyboard.is_none() {
            self.keyboard = Some(self.vkm.create_virtual_keyboard(&self.seat, &self.qh, ()));
        }
        let keyboard = self.keyboard.clone().unwrap();

        let keymap = build_keymap(text);
        let bytes = keymap.as_bytes();
        let mut file = anon_file();
        file.write_all(bytes).unwrap();
        file.write_all(&[0]).unwrap(); // XKB_V1 keymaps are NUL-terminated; size includes it.
        file.flush().unwrap();
        file.seek(SeekFrom::Start(0)).unwrap();
        keyboard.keymap(1, file.as_fd(), (bytes.len() + 1) as u32);
        keyboard.modifiers(0, 0, 0, 0);
        let _ = self.conn.flush();
        thread::sleep(Duration::from_millis(50));

        for (i, _) in text.chars().enumerate() {
            let code = (i as u32) + 1;
            keyboard.key(now_ms() as u32, code, 1);
            let _ = self.conn.flush();
            thread::sleep(Duration::from_millis(6));
            keyboard.key(now_ms() as u32, code, 0);
            let _ = self.conn.flush();
            thread::sleep(Duration::from_millis(12));
        }
        let _ = self.conn.flush();
    }

    /// Act on a soul-minted `commandeer` command. The soul has already authorized this
    /// through its action gate; the driver only carries out the mechanism.
    fn commandeer(&mut self, target_id: &str, mode: &str, text: Option<&str>) {
        let Ok(ext_id) = target_id.parse::<u32>() else {
            eprintln!("[driver] commandeer: bad targetId {target_id:?}");
            return;
        };
        if !self.toplevels.contains_key(&ext_id) {
            eprintln!("[driver] commandeer: no such target {ext_id}");
            return;
        }
        match mode {
            "pin" => self.set_target(ext_id),
            "monitor" => {
                self.activate(ext_id);
                self.set_target(ext_id);
            }
            "control" => {
                self.activate(ext_id);
                self.set_target(ext_id);
                if let Some(t) = text {
                    thread::sleep(Duration::from_millis(450)); // let focus settle before typing
                    self.inject_text(t);
                }
            }
            other => eprintln!("[driver] commandeer: unknown mode {other:?}"),
        }
    }

    /// Parse + dispatch an inbound frame. We only act on `commandeer`; every other relayed
    /// cue (target_*, said, action_request, …) is for the body/soul and ignored here.
    fn on_command(&mut self, text: &str) {
        let Ok(v) = serde_json::from_str::<Value>(text) else {
            return;
        };
        if v.get("protocol").and_then(|p| p.as_str()) != Some("presence")
            || v.get("v").and_then(|n| n.as_u64()) != Some(0)
            || v.get("kind").and_then(|k| k.as_str()) != Some("commandeer")
        {
            return;
        }
        let Some(target_id) = v.get("targetId").and_then(|t| t.as_str()) else {
            return;
        };
        let mode = v.get("mode").and_then(|m| m.as_str()).unwrap_or("pin");
        let cmd_text = v.get("text").and_then(|t| t.as_str());
        self.commandeer(target_id, mode, cmd_text);
    }

    fn on_geometry(&mut self, cosmic_id: u32, output_id: u32, x: i32, y: i32, w: i32, h: i32) {
        let Some(&ext_id) = self.cosmic_to_ext.get(&cosmic_id) else {
            return;
        };
        let out = self.outputs.get(&output_id).cloned().unwrap_or_default();
        let gx = out.logical_x + x;
        let gy = out.logical_y + y;
        let scale = out.scale.max(1);

        if self.acquired != Some(ext_id) {
            if let Some(t) = self.toplevels.get_mut(&ext_id) {
                t.last = Some((gx, gy, w, h, scale));
            }
            return;
        }
        self.emit_geometry(ext_id, gx, gy, w, h, scale);
    }

    fn emit_geometry(&mut self, ext_id: u32, gx: i32, gy: i32, w: i32, h: i32, scale: i32) {
        let (title, app_id, first) = {
            let t = self.toplevels.get_mut(&ext_id).unwrap();
            let first = t.last.is_none();
            if t.last == Some((gx, gy, w, h, scale)) {
                return;
            }
            t.last = Some((gx, gy, w, h, scale));
            (t.title.clone(), t.app_id.clone(), first)
        };
        let bounds = format!(r#"{{"x":{gx},"y":{gy},"w":{w},"h":{h},"scaleFactor":{scale}}}"#);
        let ts = now_ms();
        let buddy = self.buddy.clone();
        let json = if first {
            format!(
                r#"{{"protocol":"presence","v":0,"kind":"target_acquired","buddy":"{buddy}","ts":{ts},"targetId":"{ext_id}","title":{title:?},"appId":{app_id:?},"bounds":{bounds}}}"#
            )
        } else {
            format!(
                r#"{{"protocol":"presence","v":0,"kind":"target_moved","buddy":"{buddy}","ts":{ts},"targetId":"{ext_id}","bounds":{bounds}}}"#
            )
        };
        self.emit(json);
    }

    fn on_closed(&mut self, ext_id: u32) {
        if let Some(t) = self.toplevels.remove(&ext_id) {
            if let Some(c) = &t.cosmic {
                self.cosmic_to_ext.remove(&c.id().protocol_id());
            }
        }
        if self.acquired == Some(ext_id) {
            self.acquired = None;
            let ts = now_ms();
            let buddy = self.buddy.clone();
            self.emit(format!(
                r#"{{"protocol":"presence","v":0,"kind":"target_lost","buddy":"{buddy}","ts":{ts},"targetId":"{ext_id}","reason":"closed"}}"#
            ));
        }
        self.emit_targets_available();
    }
}

fn main() {
    // BB_TARGET still seeds a startup auto-pick (back-compat); empty disables it so the soul's
    // `commandeer pin` is the only thing that frames a window.
    let target = std::env::var("BB_TARGET").unwrap_or_else(|_| "firefox".into());
    // The driver speaks AS the body it serves: the body drops cues addressed to other buddies,
    // so target_*/targets_available must carry the same id. Default "hermes" (the dev buddy).
    let buddy = std::env::var("BB_BUDDY").unwrap_or_else(|_| "hermes".into());
    eprintln!("[driver] buddy={buddy:?}; startup auto-pick matches: {target:?} (soul can retarget via commandeer)");

    let conn = Connection::connect_to_env().expect("connect wayland (is this a COSMIC session?)");
    let (globals, queue) = registry_queue_init::<App>(&conn).expect("registry init");
    let qh = queue.handle();

    let info: cosmic_info::ZcosmicToplevelInfoV1 = globals
        .bind(&qh, 1..=3, ())
        .expect("bind zcosmic_toplevel_info_v1 (COSMIC compositor required)");
    let _list: ext_list::ExtForeignToplevelListV1 = globals
        .bind(&qh, 1..=1, ())
        .expect("bind ext_foreign_toplevel_list_v1");
    let xdg_manager: xdg_mgr::ZxdgOutputManagerV1 =
        globals.bind(&qh, 1..=3, ()).expect("bind zxdg_output_manager_v1");
    let seat: wl_seat::WlSeat = globals.bind(&qh, 1..=7, ()).expect("bind wl_seat");
    let manager: cosmic_mgr::ZcosmicToplevelManagerV1 = globals
        .bind(&qh, 1..=4, ())
        .expect("bind zcosmic_toplevel_manager_v1 (commandeer activate)");
    let vkm: vk_mgr::ZwpVirtualKeyboardManagerV1 = globals
        .bind(&qh, 1..=1, ())
        .expect("bind zwp_virtual_keyboard_manager_v1 (commandeer type)");

    // calloop: the wayland queue and the inbound command channel share one loop.
    let mut event_loop: EventLoop<App> = EventLoop::try_new().expect("event loop");
    let handle = event_loop.handle();
    let (inbound_tx, inbound_rx) = calloop::channel::channel::<String>();
    let out = spawn_link(inbound_tx);

    let mut app = App {
        conn: conn.clone(),
        qh: qh.clone(),
        info,
        xdg_mgr: xdg_manager,
        seat,
        manager,
        vkm,
        keyboard: None,
        out,
        buddy,
        target,
        toplevels: HashMap::new(),
        cosmic_to_ext: HashMap::new(),
        outputs: HashMap::new(),
        acquired: None,
        last_targets_sig: String::new(),
    };

    // Bind every wl_output and request its xdg_output for logical position + scale.
    for g in globals.contents().clone_list() {
        if g.interface == wl_output::WlOutput::interface().name {
            let out: wl_output::WlOutput =
                globals.registry().bind(g.name, g.version.min(4), &qh, ());
            let oid = out.id().protocol_id();
            app.xdg_mgr.get_xdg_output(&out, &qh, oid);
            app.outputs.insert(oid, OutputInfo::default());
        }
    }

    WaylandSource::new(conn, queue)
        .insert(handle.clone())
        .expect("insert wayland source");
    handle
        .insert_source(inbound_rx, |event, _meta, app: &mut App| {
            if let ChannelEvent::Msg(text) = event {
                app.on_command(&text);
            }
        })
        .expect("insert command source");

    eprintln!("[driver] entering event loop");
    if let Err(err) = event_loop.run(None, &mut app, |_app| {}) {
        eprintln!("[driver] event loop ended: {err}");
    }
}

// --- dispatch ------------------------------------------------------------------

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for App {
    fn event(
        _: &mut Self,
        _: &wl_registry::WlRegistry,
        _: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_seat::WlSeat, ()> for App {
    fn event(
        _: &mut Self,
        _: &wl_seat::WlSeat,
        _: wl_seat::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<cosmic_info::ZcosmicToplevelInfoV1, ()> for App {
    fn event(
        _: &mut Self,
        _: &cosmic_info::ZcosmicToplevelInfoV1,
        _: cosmic_info::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<cosmic_mgr::ZcosmicToplevelManagerV1, ()> for App {
    fn event(
        _: &mut Self,
        _: &cosmic_mgr::ZcosmicToplevelManagerV1,
        _: cosmic_mgr::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<vk_mgr::ZwpVirtualKeyboardManagerV1, ()> for App {
    fn event(
        _: &mut Self,
        _: &vk_mgr::ZwpVirtualKeyboardManagerV1,
        _: vk_mgr::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<vk::ZwpVirtualKeyboardV1, ()> for App {
    fn event(
        _: &mut Self,
        _: &vk::ZwpVirtualKeyboardV1,
        _: vk::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ext_list::ExtForeignToplevelListV1, ()> for App {
    fn event(
        app: &mut Self,
        _: &ext_list::ExtForeignToplevelListV1,
        event: ext_list::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let ext_list::Event::Toplevel { toplevel } = event {
            let ext_id = toplevel.id().protocol_id();
            let cosmic = app.info.get_cosmic_toplevel(&toplevel, qh, ext_id);
            app.cosmic_to_ext.insert(cosmic.id().protocol_id(), ext_id);
            app.toplevels.insert(
                ext_id,
                Toplevel {
                    title: String::new(),
                    app_id: String::new(),
                    cosmic: Some(cosmic),
                    last: None,
                },
            );
        }
    }

    wayland_client::event_created_child!(App, ext_list::ExtForeignToplevelListV1, [
        ext_list::EVT_TOPLEVEL_OPCODE => (ext_handle::ExtForeignToplevelHandleV1, ()),
    ]);
}

impl Dispatch<ext_handle::ExtForeignToplevelHandleV1, ()> for App {
    fn event(
        app: &mut Self,
        proxy: &ext_handle::ExtForeignToplevelHandleV1,
        event: ext_handle::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let ext_id = proxy.id().protocol_id();
        match event {
            ext_handle::Event::Title { title } => {
                if let Some(t) = app.toplevels.get_mut(&ext_id) {
                    t.title = title;
                }
                app.reconsider(ext_id);
            }
            ext_handle::Event::AppId { app_id } => {
                if let Some(t) = app.toplevels.get_mut(&ext_id) {
                    t.app_id = app_id;
                }
                app.reconsider(ext_id);
            }
            ext_handle::Event::Closed => app.on_closed(ext_id),
            _ => {}
        }
    }
}

impl Dispatch<cosmic_handle::ZcosmicToplevelHandleV1, u32> for App {
    fn event(
        app: &mut Self,
        proxy: &cosmic_handle::ZcosmicToplevelHandleV1,
        event: cosmic_handle::Event,
        _: &u32,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let cosmic_handle::Event::Geometry {
            output,
            x,
            y,
            width,
            height,
        } = event
        {
            app.on_geometry(
                proxy.id().protocol_id(),
                output.id().protocol_id(),
                x,
                y,
                width,
                height,
            );
        }
    }
}

impl Dispatch<wl_output::WlOutput, ()> for App {
    fn event(
        app: &mut Self,
        proxy: &wl_output::WlOutput,
        event: wl_output::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_output::Event::Scale { factor } = event {
            if let Some(o) = app.outputs.get_mut(&proxy.id().protocol_id()) {
                o.scale = factor;
            }
        }
    }
}

impl Dispatch<xdg_mgr::ZxdgOutputManagerV1, ()> for App {
    fn event(
        _: &mut Self,
        _: &xdg_mgr::ZxdgOutputManagerV1,
        _: xdg_mgr::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<xdg_output::ZxdgOutputV1, u32> for App {
    fn event(
        app: &mut Self,
        _: &xdg_output::ZxdgOutputV1,
        event: xdg_output::Event,
        output_id: &u32,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_output::Event::LogicalPosition { x, y } = event {
            if let Some(o) = app.outputs.get_mut(output_id) {
                o.logical_x = x;
                o.logical_y = y;
            }
        }
    }
}
