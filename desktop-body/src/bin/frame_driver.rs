//! Border-target geometry driver (build-order "step b").
//!
//! A standalone process that speaks the presence protocol. It tracks one native COSMIC
//! window — matched by title/app-id substring (`BB_TARGET`, default "firefox") — via the
//! `cosmic-toplevel-info` protocol, and emits `target_acquired` / `target_moved` /
//! `target_lost` envelopes as the window is created, moved, resized, or closed.
//!
//! Geometry arrives per-output (relative to the output's origin); we add each output's
//! xdg-output logical position to lift it into one global logical coordinate space — the
//! canonical `TargetBounds` space the body consumes, with `scaleFactor` carried alongside
//! so HiDPI math stays on the body where it belongs.
//!
//! By design this is COSMIC-only. A Windows/macOS port is a *different* binary emitting the
//! same envelopes; the body never learns which one produced a rectangle. Output goes to
//! stdout always, and to the gateway WebSocket (`BB_PRESENCE_URL`) when reachable, so the
//! running body receives target cues like any other to-body message.

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use wayland_client::globals::{registry_queue_init, GlobalListContents};
use wayland_client::protocol::{wl_output, wl_registry};
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

const DEFAULT_URL: &str = "ws://127.0.0.1:17387/border-buddies";

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Optional gateway sink. Best-effort: a down gateway never stops the driver — target cues
/// still go to stdout, mirroring the body's own non-fatal presence connection.
struct GatewaySink {
    url: String,
    socket:
        Option<tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<std::net::TcpStream>>>,
}

impl GatewaySink {
    fn connect() -> Self {
        let url = std::env::var("BB_PRESENCE_URL").unwrap_or_else(|_| DEFAULT_URL.to_string());
        let socket = match tungstenite::connect(&url) {
            Ok((socket, _)) => {
                eprintln!("[driver] gateway connected: {url}");
                Some(socket)
            }
            Err(e) => {
                eprintln!("[driver] gateway unavailable ({e}); stdout only");
                None
            }
        };
        Self { url, socket }
    }

    fn reconnect(&mut self) {
        if self.socket.is_some() {
            return;
        }
        match tungstenite::connect(&self.url) {
            Ok((socket, _)) => {
                eprintln!("[driver] gateway connected: {}", self.url);
                self.socket = Some(socket);
            }
            Err(_) => {}
        };
    }

    fn send(&mut self, json: &str) {
        self.reconnect();
        if let Some(socket) = self.socket.as_mut() {
            if socket
                .send(tungstenite::Message::text(json.to_string()))
                .is_err()
            {
                eprintln!("[driver] gateway send failed; dropping sink");
                self.socket = None;
            }
        }
    }
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
    info: cosmic_info::ZcosmicToplevelInfoV1,
    xdg_mgr: xdg_mgr::ZxdgOutputManagerV1,
    sink: GatewaySink,
    target: String,
    toplevels: HashMap<u32, Toplevel>, // keyed by ext handle id
    cosmic_to_ext: HashMap<u32, u32>,
    outputs: HashMap<u32, OutputInfo>, // keyed by wl_output id
    acquired: Option<u32>,             // ext handle id we are currently framing
}

impl App {
    fn matches(&self, t: &Toplevel) -> bool {
        let n = self.target.to_lowercase();
        t.title.to_lowercase().contains(&n) || t.app_id.to_lowercase().contains(&n)
    }

    fn emit(&mut self, json: String) {
        println!("{json}");
        self.sink.send(&json);
    }

    /// A toplevel's identity changed: (re)evaluate whether it should be our target. First
    /// match wins — a spike tracks one window; multi-target selection is a later concern.
    fn reconsider(&mut self, ext_id: u32) {
        if self.acquired.is_some() {
            return;
        }
        let Some(t) = self.toplevels.get(&ext_id) else {
            return;
        };
        if (t.title.is_empty() && t.app_id.is_empty()) || !self.matches(t) {
            return;
        }
        self.acquired = Some(ext_id);
        eprintln!("[driver] acquired '{}' / '{}'", t.title, t.app_id);
        // Bounds follow from the first geometry event (which emits target_acquired); if
        // geometry already arrived before identity matched, replay it now.
        if let Some((gx, gy, w, h, scale)) = self.toplevels.get(&ext_id).and_then(|t| t.last) {
            self.toplevels.get_mut(&ext_id).unwrap().last = None;
            self.emit_geometry(ext_id, gx, gy, w, h, scale);
        }
    }

    fn on_geometry(&mut self, cosmic_id: u32, output_id: u32, x: i32, y: i32, w: i32, h: i32) {
        let Some(&ext_id) = self.cosmic_to_ext.get(&cosmic_id) else {
            return;
        };
        let out = self.outputs.get(&output_id).cloned().unwrap_or_default();
        // Per-output coords → global logical space (the canonical TargetBounds space).
        let gx = out.logical_x + x;
        let gy = out.logical_y + y;
        let scale = out.scale.max(1);

        if self.acquired != Some(ext_id) {
            // Not our target (yet). Stash the latest bounds so a later identity match can
            // emit target_acquired without waiting for the next move.
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
                return; // unchanged
            }
            t.last = Some((gx, gy, w, h, scale));
            (t.title.clone(), t.app_id.clone(), first)
        };
        let bounds = format!(r#"{{"x":{gx},"y":{gy},"w":{w},"h":{h},"scaleFactor":{scale}}}"#);
        let ts = now_ms();
        let json = if first {
            format!(
                r#"{{"protocol":"presence","v":0,"kind":"target_acquired","buddy":"hermes","ts":{ts},"targetId":"{ext_id}","title":{title:?},"appId":{app_id:?},"bounds":{bounds}}}"#
            )
        } else {
            format!(
                r#"{{"protocol":"presence","v":0,"kind":"target_moved","buddy":"hermes","ts":{ts},"targetId":"{ext_id}","bounds":{bounds}}}"#
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
            self.emit(format!(
                r#"{{"protocol":"presence","v":0,"kind":"target_lost","buddy":"hermes","ts":{ts},"targetId":"{ext_id}","reason":"closed"}}"#
            ));
        }
    }
}

fn main() {
    let target = std::env::var("BB_TARGET").unwrap_or_else(|_| "firefox".into());
    eprintln!("[driver] tracking windows matching: {target:?}");

    let conn = Connection::connect_to_env().expect("connect wayland (is this a COSMIC session?)");
    let (globals, mut queue) = registry_queue_init::<App>(&conn).expect("registry init");
    let qh = queue.handle();

    let info: cosmic_info::ZcosmicToplevelInfoV1 = globals
        .bind(&qh, 1..=3, ())
        .expect("bind zcosmic_toplevel_info_v1 (COSMIC compositor required)");
    let _list: ext_list::ExtForeignToplevelListV1 = globals
        .bind(&qh, 1..=1, ())
        .expect("bind ext_foreign_toplevel_list_v1");
    let xdg_manager: xdg_mgr::ZxdgOutputManagerV1 = globals
        .bind(&qh, 1..=3, ())
        .expect("bind zxdg_output_manager_v1");

    let mut app = App {
        info,
        xdg_mgr: xdg_manager,
        sink: GatewaySink::connect(),
        target,
        toplevels: HashMap::new(),
        cosmic_to_ext: HashMap::new(),
        outputs: HashMap::new(),
        acquired: None,
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

    eprintln!("[driver] entering event loop");
    loop {
        queue.blocking_dispatch(&mut app).expect("dispatch");
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
            // Pair with a cosmic handle so we receive geometry for this toplevel.
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
