//! Presence WebSocket client (Step 4 — commit 1: scaffold).
//!
//! A blocking `tungstenite` connection on its own thread, bridged into the body's
//! `calloop` loop via a `calloop::channel`. No async runtime, no GTK/WebKit/GPU.
//!
//! On every (re)connect the body announces itself with an `attached` handshake — the
//! soul is expected to reply with `hydrate`. `attached` is deliberately distinct from
//! `summoned` (which means the *user* opened the surface): see `src/presenceProtocol.ts`.
//!
//! This commit only *forwards* inbound frames to the main thread, which logs them.
//! Parsing cues into body state (express/say/move_to/hydrate) and emitting to-soul
//! events are the next commits. The connection is non-fatal: if the gateway is down
//! the body runs standalone and the thread keeps retrying with backoff.

use std::net::TcpStream;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use calloop::channel::Sender;
use serde_json::json;
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{connect, Message, WebSocket};

const DEFAULT_URL: &str = "ws://127.0.0.1:17387/border-buddies";
const PROTOCOL: &str = "presence";
const VERSION: u32 = 0;

const INITIAL_BACKOFF: Duration = Duration::from_millis(500);
const MAX_BACKOFF: Duration = Duration::from_secs(5);
/// Keep `read()` from blocking forever so the loop stays responsive (and, in a later
/// commit, can drain an outbound queue between reads).
const READ_TIMEOUT: Duration = Duration::from_millis(200);

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// Spawn the presence client thread. Returns an outbound sender the body uses to push
/// to-soul events (clicked/grabbed/dropped/...); the body runs with or without a soul,
/// so sends are best-effort and silently dropped if no soul is connected. `inbound`
/// forwards raw inbound JSON frames into the calloop loop. `buddy` is the body's
/// identity — used in the `attached` handshake and to filter inbound cues.
pub fn spawn(buddy: String, inbound: Sender<String>) -> mpsc::Sender<String> {
    let url = std::env::var("BB_PRESENCE_URL").unwrap_or_else(|_| DEFAULT_URL.to_string());
    let (out_tx, out_rx) = mpsc::channel::<String>();

    if let Err(err) = thread::Builder::new()
        .name("bb-presence".into())
        .spawn(move || run(url, buddy, inbound, out_rx))
    {
        eprintln!("[bb-presence] could not start presence thread: {err} — running standalone");
    }

    out_tx
}

fn run(url: String, buddy: String, inbound: Sender<String>, out_rx: mpsc::Receiver<String>) {
    let mut backoff = INITIAL_BACKOFF;

    loop {
        match connect(&url) {
            Ok((mut socket, _response)) => {
                eprintln!("[bb-presence] connected to {url} as '{buddy}'");
                backoff = INITIAL_BACKOFF; // reset after a good connection

                // Presence events are ephemeral. Anything queued while we were
                // disconnected is now stale — replaying a grabbed/clicked after
                // reconnect would be a lie. Discard the backlog; `attached`+`hydrate`
                // re-establishes truth.
                while out_rx.try_recv().is_ok() {}

                if announce(&mut socket, &buddy).is_ok() {
                    set_read_timeout(&socket, Some(READ_TIMEOUT));
                    if pump(&mut socket, &inbound, &out_rx).is_break() {
                        return; // main loop gone — body is exiting
                    }
                }
                // any error falls through to reconnect
            }
            Err(err) => {
                eprintln!("[bb-presence] connect failed ({url}): {err} — retry in {backoff:?}");
            }
        }

        thread::sleep(backoff);
        backoff = (backoff * 2).min(MAX_BACKOFF);
    }
}

/// Announce ourselves so the soul knows to hydrate us.
fn announce(socket: &mut WebSocket<MaybeTlsStream<TcpStream>>, buddy: &str) -> Result<(), ()> {
    let attached = json!({
        "protocol": PROTOCOL,
        "v": VERSION,
        "kind": "attached",
        "buddy": buddy,
        "ts": now_ms(),
        "capabilities": ["drag", "menu", "say"],
    })
    .to_string();

    match socket.send(Message::Text(attached.into())) {
        Ok(()) => Ok(()),
        Err(err) => {
            eprintln!("[bb-presence] failed to send attached: {err}");
            Err(())
        }
    }
}

/// Read frames until the connection drops. Returns `Break` only when the calloop
/// channel is closed (the body is shutting down), so the caller stops reconnecting.
fn pump(
    socket: &mut WebSocket<MaybeTlsStream<TcpStream>>,
    inbound: &Sender<String>,
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
            Ok(Message::Close(_)) => {
                eprintln!("[bb-presence] server closed the connection");
                return ControlFlow::Continue(());
            }
            // Pings are answered automatically by tungstenite on the next write; ignore.
            Ok(_) => {}
            Err(tungstenite::Error::Io(e))
                if e.kind() == ErrorKind::WouldBlock || e.kind() == ErrorKind::TimedOut =>
            {
                // Idle read timeout — the cadence (~200ms) at which we flush queued
                // to-soul events. A send failure means the link is gone; reconnect.
                if !drain_outbound(socket, out_rx) {
                    return ControlFlow::Continue(());
                }
            }
            Err(err) => {
                eprintln!("[bb-presence] read error: {err} — reconnecting");
                return ControlFlow::Continue(());
            }
        }
    }
}

/// Flush all queued to-soul events to the socket. Returns `false` if a send failed
/// (the caller should reconnect). An empty or disconnected queue is not an error.
fn drain_outbound(
    socket: &mut WebSocket<MaybeTlsStream<TcpStream>>,
    out_rx: &mpsc::Receiver<String>,
) -> bool {
    loop {
        match out_rx.try_recv() {
            Ok(text) => {
                if let Err(err) = socket.send(Message::Text(text.into())) {
                    eprintln!("[bb-presence] failed to send to-soul event: {err} — reconnecting");
                    return false;
                }
            }
            // Empty: nothing pending. Disconnected: the body is shutting down — the
            // inbound channel will surface that; just stop draining for now.
            Err(_) => return true,
        }
    }
}

// --- outbound to-soul events ---------------------------------------------------
//
// The body reports its own interaction (clicked/grabbed/dropped) so the soul can
// react and, in time, mint receipts. These are presentation-OUT only: the body never
// reports screen content or does anything beyond its own surface (AGENTS.md law 7).
// Position is the body's current placement as a `free`/`screen` point — what the soul
// persists so a later `hydrate` can restore it.

fn to_soul(kind: &str, buddy: &str, mut fields: Value) -> String {
    let envelope = fields.as_object_mut().expect("fields is a json object");
    envelope.insert("protocol".into(), json!(PROTOCOL));
    envelope.insert("v".into(), json!(VERSION));
    envelope.insert("kind".into(), json!(kind));
    envelope.insert("buddy".into(), json!(buddy));
    envelope.insert("ts".into(), json!(now_ms()));
    Value::Object(envelope.clone()).to_string()
}

fn free_at(x: f64, y: f64) -> Value {
    json!({ "mode": "free", "space": "screen", "x": x, "y": y })
}

/// A non-drag press on the buddy.
pub fn clicked_json(buddy: &str, x: f64, y: f64) -> String {
    to_soul("clicked", buddy, json!({ "button": "primary", "at": free_at(x, y) }))
}

/// The user began dragging the buddy (emitted once, when travel crosses the click slop).
pub fn grabbed_json(buddy: &str, x: f64, y: f64) -> String {
    to_soul("grabbed", buddy, json!({ "at": free_at(x, y) }))
}

/// The user released a drag — `at` is the final resting position the soul persists.
pub fn dropped_json(buddy: &str, x: f64, y: f64) -> String {
    to_soul("dropped", buddy, json!({ "at": free_at(x, y) }))
}

fn edge_wire(edge: Edge) -> &'static str {
    match edge {
        Edge::Top => "top",
        Edge::Right => "right",
        Edge::Bottom => "bottom",
        Edge::Left => "left",
    }
}

fn tucked_at(edge: Edge, along: f64) -> Value {
    // Offset's along-edge axis carries the position; the flush axis is ignored.
    let offset = match edge {
        Edge::Left | Edge::Right => json!({ "x": 0.0, "y": along }),
        Edge::Top | Edge::Bottom => json!({ "x": along, "y": 0.0 }),
    };
    json!({ "mode": "tucked", "edge": edge_wire(edge), "offset": offset })
}

/// The user dropped the buddy against an edge — it tucked. `at` is a tucked position
/// the soul persists so the next `hydrate` brings the buddy back tucked.
pub fn dropped_tucked_json(buddy: &str, edge: Edge, along: f64) -> String {
    to_soul("dropped", buddy, json!({ "at": tucked_at(edge, along), "onTarget": "edge" }))
}

/// The buddy was tucked away (user pushed it to an edge). Distinct from `dropped`:
/// dropped persists placement; dismissed is the lifecycle signal that it's now away.
pub fn dismissed_json(buddy: &str) -> String {
    to_soul("dismissed", buddy, json!({}))
}

/// The user clicked a tucked bump to bring the buddy back out.
pub fn summoned_json(buddy: &str) -> String {
    to_soul("summoned", buddy, json!({}))
}

/// The user typed a message to the buddy through the on-body input box. This is the
/// one to-soul event that carries free text — still presentation-in only: the body
/// reports what was typed, it never acts on it.
pub fn said_json(buddy: &str, text: &str) -> String {
    to_soul("said", buddy, json!({ "text": text }))
}

/// The user asked the buddy to run one of its granted effectors (e.g. a `/review`
/// affordance → `receipt_review`). This is only a *request*: authorization happens in the
/// soul's action gate, never in the body (law 7). `confirmed` is the follow-up press after
/// a `needs_confirmation` result; `request_id` correlates the resulting `action_result`.
pub fn action_request_json(buddy: &str, effector: &str, confirmed: bool, request_id: Option<&str>) -> String {
    let mut fields = json!({ "effector": effector });
    if confirmed {
        fields["confirmed"] = json!(true);
    }
    if let Some(id) = request_id {
        fields["requestId"] = json!(id);
    }
    to_soul("action_request", buddy, fields)
}

/// A typed action intent the body fills on an `action_request` — the wire membrane: only a typed
/// intent may authorize an `act` effector (e.g. `repo_edit`). The body builds one from a fixed
/// affordance; it never parses free text into it. The soul validates it against the manifest + gate
/// and lifts it into the core `ActionIntent` (AGENTS.md law 7 — the body fills fields, the soul
/// interprets). Borrowed `&str` throughout so call sites pass literals/state fields without
/// allocating. A `none`/value-less target is grant-only — the soul will refuse to authorize an
/// effect from it.
#[derive(Debug, Clone, Copy)]
pub struct ActionIntent<'a> {
    /// Effector-specific verb, e.g. "write_patch".
    pub operation: &'a str,
    /// One of `repo_path` | `file_path` | `url` | `command` | `none` (re-validated soul-side).
    pub target_kind: &'a str,
    /// The concrete target — a repo-relative path, URL, or command string. `None` pairs with a
    /// `none`/grant-only intent.
    pub target_value: Option<&'a str>,
    /// One-line, user-facing description of the intended effect. The soul synthesizes one if absent.
    pub summary: Option<&'a str>,
    /// Hash of the payload (diff/command/body) so the receipt pins WHAT was authorized.
    pub payload_digest: Option<&'a str>,
}

/// Render an `ActionIntent` to its wire object — mirrors the TS `PresenceActionIntent` shape so the
/// soul's strict parser (`isActionIntent`) accepts it. Optional fields are omitted, not nulled.
fn intent_value(intent: &ActionIntent) -> Value {
    let mut target = json!({ "kind": intent.target_kind });
    if let Some(v) = intent.target_value {
        target["value"] = json!(v);
    }
    let mut obj = json!({ "operation": intent.operation, "target": target });
    if let Some(s) = intent.summary {
        obj["summary"] = json!(s);
    }
    if let Some(d) = intent.payload_digest {
        obj["payloadDigest"] = json!(d);
    }
    obj
}

/// Like `action_request_json`, but carrying a typed `intent` — the body asking the soul to
/// authorize a specific EFFECT (operation + target), not just the effector grant. Authorization
/// still happens soul-side; this only fills the wire fields (law 7). `confirmed` is the follow-up
/// press after a `needs_confirmation`; `request_id` correlates the resulting `action_result`.
pub fn action_request_intent_json(
    buddy: &str,
    effector: &str,
    intent: &ActionIntent,
    confirmed: bool,
    request_id: Option<&str>,
) -> String {
    let mut fields = json!({ "effector": effector, "intent": intent_value(intent) });
    if confirmed {
        fields["confirmed"] = json!(true);
    }
    if let Some(id) = request_id {
        fields["requestId"] = json!(id);
    }
    to_soul("action_request", buddy, fields)
}

/// The body's right-click P/M/C menu asking the soul to run the act-floored `commandeer`
/// effector on a tracked native window. AGENTS.md law 7: the body only NAMES the target and
/// the mode — it never raises or types into the window itself. The soul authorizes through the
/// action gate and, on `allow`, dispatches the world-effect (activate / type) to the frame
/// driver. `mode` is `monitor` | `control`; `pin` is a local presentation toggle and is never
/// routed here. The window rides as a `command` target so the soul lifts it to
/// `intent.target.path`, which `dispatchCommandeer` reads as the `targetId`. `confirmed` is the
/// follow-up after the act-floor's `needs_confirmation`.
pub fn commandeer_request_json(
    buddy: &str,
    mode: &str,
    target_id: &str,
    name: &str,
    confirmed: bool,
) -> String {
    let summary = format!("{mode} {name}");
    let intent = ActionIntent {
        operation: mode,
        target_kind: "command",
        target_value: Some(target_id),
        summary: Some(&summary),
        payload_digest: None,
    };
    action_request_intent_json(buddy, "commandeer", &intent, confirmed, None)
}

/// The user asked to enter/switch to a governed surface. The body only names the desired
/// surface; the soul decides whether it is known, granted, wired, and confirmed.
pub fn surface_request_json(buddy: &str, surface: &str) -> String {
    to_soul("surface_request", buddy, json!({ "surface": surface }))
}

/// The user dragged the visible frame head while the buddy was framing a native window.
/// This is only a request: moving the OS window is an effector owned by the soul/driver,
/// never by the body.
pub fn target_drag_requested_json(buddy: &str, target_id: &str, dx: f64, dy: f64) -> String {
    to_soul(
        "target_drag_requested",
        buddy,
        json!({ "targetId": target_id, "delta": { "x": dx, "y": dy } }),
    )
}

/// Set a read timeout on the underlying TCP stream (plain ws:// only).
fn set_read_timeout(socket: &WebSocket<MaybeTlsStream<TcpStream>>, dur: Option<Duration>) {
    match socket.get_ref() {
        MaybeTlsStream::Plain(stream) => {
            let _ = stream.set_read_timeout(dur);
        }
        _ => {}
    }
}

// --- inbound to-body cues ------------------------------------------------------
//
// We parse only the to-body kinds the body acts on: express / say / move_to /
// hydrate. `attention` is reserved (deferred — see the Step 4 plan), and to-soul
// kinds are not cues, so both yield `None` and are ignored. Mirrors the strict
// TS parser: anything malformed returns `None` and is dropped, never panics.

use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    Top,
    Right,
    Bottom,
    Left,
}

/// A body-agnostic position resolved enough for the body to place itself.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Position {
    /// Float the whole figure near `edge` with a pixel offset (anchor+margin).
    Anchored { edge: Edge, ox: f64, oy: f64 },
    /// Park flush against `edge` in minimized (bump) form; the offset's along-edge
    /// axis positions it down/across that edge, the flush axis is ignored. The
    /// minimized rendering itself lands in a later commit; placement is shared with
    /// `Anchored` for now (a flush-axis offset of 0 puts it against the edge).
    Tucked { edge: Edge, ox: f64, oy: f64 },
    /// Float at a screen point (v1: treated as the surface top-left).
    Free { x: f64, y: f64 },
}

/// Bounds of a tracked native OS window, in logical pixels in one global space, with
/// the `scale_factor` that produced them. The platform driver (cosmic-toplevel-info
/// here, a Win32 hook elsewhere) converts into this canonical space before emitting, so
/// the body does the final device-pixel math the same way regardless of host OS.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TargetBounds {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub scale_factor: f64,
}

/// Why the body lost its grip on a target. A closed set, mirroring the TS union, so the
/// renderer can branch on it (release gracefully on `Closed` vs. look around on
/// `TrackingFailed`); an unknown reason string drops the whole cue rather than guessing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetLostReason {
    Closed,
    WorkspaceSwitched,
    Minimized,
    TrackingFailed,
}

/// An inbound presentation cue the body applies to itself.
#[derive(Debug, Clone, PartialEq)]
pub enum Cue {
    Express { emotion: String },
    Say { text: String },
    MoveTo { position: Position },
    Hydrate {
        position: Option<Position>,
        emotion: Option<String>,
        speech: Option<String>,
        /// Ordered, soul-pushed surface list (canonical SURFACE_ORDER) with per-surface
        /// availability. The body cycles and dims from this instead of any local manifest
        /// (AGENTS.md: bodies stay manifest-free). Empty when the soul omitted it.
        surfaces: Vec<SurfaceDescriptor>,
    },
    /// Rich result content for the output surface (torso). `surface` is one of
    /// text/image/file/session; image/file carry inline base64 bytes the body decodes
    /// (the body has no HTTP/TLS — the soul/gateway always sends bytes, never a URL).
    Output {
        surface: String,
        text: Option<String>,
        caption: Option<String>,
        media_type: Option<String>,
        data_base64: Option<String>,
    },
    /// Outcome of an `action_request` the soul ran through the governance action gate.
    /// Distinct from `Output`: a governance result the body shows as a badge/affordance,
    /// keyed on `decision` (allow/needs_confirmation/blocked). The full receipt stays
    /// soul-side; the body only holds this thin cue (AGENTS.md law 7).
    ActionResult {
        effector: String,
        decision: String,
        receipt_id: String,
        request_id: Option<String>,
        summary: Option<String>,
        /// World-facing execution outcome (present only on `allow` paths). `executed` is the
        /// load-bearing bit; `route` is the provider provenance ("providers rotate").
        outcome: Option<ActionOutcome>,
    },
    SurfaceActive {
        surface: String,
        posture: String,
        label: Option<String>,
        provider_label: Option<String>,
        /// The route the active surface rides — provider label, `local|cloud` locality, and
        /// optional `ready|degraded|unavailable` health. Mirrors the TS `SurfaceRoute`; one
        /// nested object so the passport row and a future route ring read the same shape.
        route: Option<SurfaceRoute>,
    },
    /// An onboarding form section the wizard Host wants rendered in the torso panel (Build C).
    /// The native twin of the React `OnboardingWizardPanel`: the Host owns the act→section mapping
    /// and the words (law 7); the body draws the title/prompt/options/fields and reports a
    /// `clicked{panel: primary_panel}` on confirm — it never invents the token. `section == "none"`
    /// carries no form and tells the body to close the panel.
    Panel {
        section: String,
        title: String,
        prompt: Option<String>,
        options: Vec<PanelOption>,
        fields: Vec<PanelField>,
        rows: Vec<PanelRow>,
        primary_label: Option<String>,
        primary_panel: Option<String>,
    },
    /// Border-target tracking (the "Morph Frame" seam): a platform driver tells the body
    /// where a native OS window is so it can wrap its hollow torso around it. Split three
    /// ways so the renderer binds distinct behavior to the lifecycle; every cue carries
    /// `target_id`, and `TargetAcquired` carries initial `bounds` so there is no
    /// empty-handed gap before the first move.
    TargetAcquired { target_id: String, title: String, app_id: String, bounds: TargetBounds },
    TargetMoved { target_id: String, bounds: TargetBounds },
    TargetLost { target_id: String, reason: TargetLostReason },
    /// The full enumerated window list the platform driver can act on — the data behind the
    /// right-click commandeer picker's first phase. Unlike the `Target*` lifecycle (which tracks
    /// the ONE pinned window), this is the menu of everything available to pin/monitor/control.
    /// The body only renders it as choices; selecting one emits a soul-gated `commandeer` request
    /// (AGENTS.md law 7 — the body never raises or types into any of these itself).
    TargetsAvailable { targets: Vec<TargetEntry> },
}

/// One selectable window in the commandeer picker, mirroring the driver's `targets_available`
/// entries. Carries only identity + labels — never geometry or content; the body shows it as a
/// choice and reports the user's pick, nothing more.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetEntry {
    pub target_id: String,
    pub title: String,
    pub app_id: String,
}

/// One selectable row in a `Panel` section (a posture/placement choice, a provider preset).
/// Mirrors the TS `PresencePanelOption`. `selected` is the Host's default highlight, never
/// authoritative — the body reports the user's actual pick (law 7).
#[derive(Debug, Clone, PartialEq)]
pub struct PanelOption {
    pub id: String,
    pub label: String,
    pub detail: Option<String>,
    pub selected: bool,
}

/// One input field in a `Panel` section. Mirrors the TS `PresencePanelField`: `control` is the
/// closed set `paste_key | text | select` (`paste_key` is the masked clipboard credential
/// affordance); `masked` echoes the value as dots.
#[derive(Debug, Clone, PartialEq)]
pub struct PanelField {
    pub key: String,
    pub label: String,
    pub control: String,
    pub masked: bool,
    pub value: Option<String>,
}

/// One receipt row in the summary section. Mirrors the TS `PresencePanelRow`: `status` is the
/// closed set `recorded | pending`.
#[derive(Debug, Clone, PartialEq)]
pub struct PanelRow {
    pub label: String,
    pub status: String,
}

/// One entry in the `hydrate` surface list. Mirrors the TS `PresenceSurfaceDescriptor`:
/// `availability` is the closed set `available | unwired | gated` — `unwired` renders dimmed.
/// `kind` is `surface` (default — switches the active surface) or `launcher` (opens an external
/// tool via a reach `action_request`); a launcher carries the `effector` id to request. Both new
/// fields are additive: an absent `kind` parses as `surface` so older soul snapshots still load.
#[derive(Debug, Clone, PartialEq)]
pub struct SurfaceDescriptor {
    pub id: String,
    pub label: String,
    pub availability: String,
    pub kind: String,
    pub effector: Option<String>,
}

impl SurfaceDescriptor {
    /// A launcher opens a tool (action_request) instead of switching the active surface.
    pub fn is_launcher(&self) -> bool {
        self.kind == "launcher"
    }
}

/// The route an active surface is riding. Mirrors the TS `SurfaceRoute`: `locality` is the
/// closed set `local | cloud`; `health` is optional (`ready | degraded | unavailable`) and
/// stays `None` until the soul derives it (Slice 3).
#[derive(Debug, Clone, PartialEq)]
pub struct SurfaceRoute {
    pub label: String,
    pub locality: String,
    pub health: Option<String>,
}

/// The provider route that carried an executed effect. Mirrors the TS `PresenceActionOutcome`
/// route shape; `locality` is the closed set `local | cloud`.
#[derive(Debug, Clone, PartialEq)]
pub struct ActionRoute {
    pub provider: String,
    pub locality: String,
    pub downgraded: bool,
    pub fallback_of: Option<String>,
}

/// World-facing execution outcome on an `action_result`. The full ExecutionReceipt stays
/// soul-side; the body holds only this thin cue so it can render "executed" vs "not run".
#[derive(Debug, Clone, PartialEq)]
pub struct ActionOutcome {
    pub executed: bool,
    pub execution_receipt_id: Option<String>,
    pub route: Option<ActionRoute>,
}

/// A parsed to-body message: which buddy it concerns, and the cue to apply.
#[derive(Debug, Clone, PartialEq)]
pub struct ToBody {
    pub buddy: String,
    pub ts: u64,
    pub cue: Cue,
}

fn parse_edge(value: &str) -> Option<Edge> {
    match value {
        "top" => Some(Edge::Top),
        "right" => Some(Edge::Right),
        "bottom" => Some(Edge::Bottom),
        "left" => Some(Edge::Left),
        _ => None,
    }
}

/// Parse an `output` cue. Mirrors the TS validator: `surface` must be a known kind,
/// and image/file must carry both inline bytes (`dataBase64`) and a `mediaType` —
/// anything else is dropped (returns None) rather than rendered as a broken card.
fn parse_output(v: &Value) -> Option<Cue> {
    let surface = v.get("surface")?.as_str()?.to_string();
    if !matches!(surface.as_str(), "text" | "image" | "file" | "session") {
        return None;
    }
    let text = v.get("text").and_then(|s| s.as_str()).map(String::from);
    let caption = v.get("caption").and_then(|s| s.as_str()).map(String::from);
    let media_type = v.get("mediaType").and_then(|s| s.as_str()).map(String::from);
    let data_base64 = v.get("dataBase64").and_then(|s| s.as_str()).map(String::from);

    if surface == "image" || surface == "file" {
        let has_bytes = data_base64.as_deref().map(|s| !s.is_empty()).unwrap_or(false);
        let has_type = media_type.as_deref().map(|s| !s.is_empty()).unwrap_or(false);
        if !has_bytes || !has_type {
            return None;
        }
    }

    Some(Cue::Output { surface, text, caption, media_type, data_base64 })
}

/// Parse an `action_result` cue. Mirrors the TS validator: `effector` and `receiptId`
/// must be non-empty strings and `decision` must be one of the three literals — an
/// unknown decision drops the whole cue rather than rendering an unknown state.
fn parse_action_result(v: &Value) -> Option<Cue> {
    let effector = nonempty(v.get("effector"))?;
    let decision = v.get("decision")?.as_str()?.to_string();
    if !matches!(decision.as_str(), "allow" | "needs_confirmation" | "blocked") {
        return None;
    }
    let receipt_id = nonempty(v.get("receiptId"))?;
    let request_id = v.get("requestId").and_then(|s| s.as_str()).map(String::from);
    let summary = v.get("summary").and_then(|s| s.as_str()).map(String::from);
    // An `outcome` is optional, but if present it must be well-formed — a malformed outcome
    // drops the whole cue (mirrors the TS `isActionOutcome` guard) rather than rendering
    // a half-known execution state.
    let outcome = match v.get("outcome") {
        None => None,
        Some(raw) => Some(parse_action_outcome(raw)?),
    };
    Some(Cue::ActionResult { effector, decision, receipt_id, request_id, summary, outcome })
}

/// Parse the optional `outcome` on an `action_result`. `executed` must be a boolean; if a
/// `route` is present, its `provider` must be non-empty and `locality` one of `local|cloud`.
fn parse_action_outcome(v: &Value) -> Option<ActionOutcome> {
    let executed = v.get("executed")?.as_bool()?;
    let execution_receipt_id = v.get("executionReceiptId").and_then(|s| s.as_str()).map(String::from);
    let route = match v.get("route") {
        None => None,
        Some(r) => {
            let provider = nonempty(r.get("provider"))?;
            let locality = r.get("locality")?.as_str()?.to_string();
            if !matches!(locality.as_str(), "local" | "cloud") {
                return None;
            }
            let downgraded = r.get("downgraded")?.as_bool()?;
            let fallback_of = r.get("fallbackOf").and_then(|s| s.as_str()).map(String::from);
            Some(ActionRoute { provider, locality, downgraded, fallback_of })
        }
    };
    Some(ActionOutcome { executed, execution_receipt_id, route })
}

/// Parse a `route` object (the value, not the parent). `label` must be non-empty and `locality`
/// one of `local|cloud`; if `health` is present it must be one of `ready|degraded|unavailable`.
/// Returns `None` on a malformed route so the caller's `?` drops the whole cue (closed sets, not
/// free strings), matching the TS `isSurfaceRoute` guard. An *absent* route is handled by the
/// caller (it never calls this), so `None` here unambiguously means "present but invalid".
fn parse_surface_route(r: &Value) -> Option<SurfaceRoute> {
    let label = nonempty(r.get("label"))?;
    let locality = r.get("locality")?.as_str()?.to_string();
    if !matches!(locality.as_str(), "local" | "cloud") {
        return None;
    }
    let health = match r.get("health") {
        None => None,
        Some(h) => {
            let h = h.as_str()?.to_string();
            if !matches!(h.as_str(), "ready" | "degraded" | "unavailable") {
                return None;
            }
            Some(h)
        }
    };
    Some(SurfaceRoute { label, locality, health })
}

/// Parse the `hydrate` surface list. Mirrors the TS `isSurfaceDescriptorArray` guard: each
/// entry needs a non-empty `id`, a string `label`, and an `availability` in the closed set
/// `available | unwired | gated`. A present-but-malformed list drops the whole cue (`?`).
fn parse_surfaces(value: &Value) -> Option<Vec<SurfaceDescriptor>> {
    let arr = value.as_array()?;
    let mut out = Vec::with_capacity(arr.len());
    for item in arr {
        let id = nonempty(item.get("id"))?;
        let label = item.get("label")?.as_str()?.to_string();
        let availability = item.get("availability")?.as_str()?.to_string();
        if !matches!(availability.as_str(), "available" | "unwired" | "gated") {
            return None;
        }
        // Additive (Slice 0 launchers): `kind` defaults to "surface"; `effector` names the
        // reach effector a launcher requests. A present-but-unknown `kind` drops the cue.
        let kind = item.get("kind").and_then(|v| v.as_str()).unwrap_or("surface").to_string();
        if !matches!(kind.as_str(), "surface" | "launcher") {
            return None;
        }
        let effector = item.get("effector").and_then(|v| v.as_str()).map(|s| s.to_string());
        out.push(SurfaceDescriptor { id, label, availability, kind, effector });
    }
    Some(out)
}

/// Parse the optional `options` list on a `Panel`. Each entry needs a non-empty `id` and a
/// string `label`; `detail`/`selected` are optional. A present-but-malformed list drops the
/// whole cue (`?`), mirroring the TS `isPanelOptionArray` guard.
fn parse_panel_options(value: &Value) -> Option<Vec<PanelOption>> {
    let arr = value.as_array()?;
    let mut out = Vec::with_capacity(arr.len());
    for item in arr {
        out.push(PanelOption {
            id: nonempty(item.get("id"))?,
            label: item.get("label")?.as_str()?.to_string(),
            detail: item.get("detail").and_then(|s| s.as_str()).map(String::from),
            selected: item.get("selected").and_then(|s| s.as_bool()).unwrap_or(false),
        });
    }
    Some(out)
}

/// Parse the optional `fields` list on a `Panel`. Each entry needs a non-empty `key`, a string
/// `label`, and a `control` in the closed set `paste_key | text | select` — anything else drops
/// the whole cue (mirrors the TS `isPanelFieldArray` guard).
fn parse_panel_fields(value: &Value) -> Option<Vec<PanelField>> {
    let arr = value.as_array()?;
    let mut out = Vec::with_capacity(arr.len());
    for item in arr {
        let control = item.get("control")?.as_str()?.to_string();
        if !matches!(control.as_str(), "paste_key" | "text" | "select") {
            return None;
        }
        out.push(PanelField {
            key: nonempty(item.get("key"))?,
            label: item.get("label")?.as_str()?.to_string(),
            control,
            masked: item.get("masked").and_then(|s| s.as_bool()).unwrap_or(false),
            value: item.get("value").and_then(|s| s.as_str()).map(String::from),
        });
    }
    Some(out)
}

/// Parse the optional `rows` list on a `Panel` (summary receipts). Each entry needs a string
/// `label` and a `status` in the closed set `recorded | pending`.
fn parse_panel_rows(value: &Value) -> Option<Vec<PanelRow>> {
    let arr = value.as_array()?;
    let mut out = Vec::with_capacity(arr.len());
    for item in arr {
        let status = item.get("status")?.as_str()?.to_string();
        if !matches!(status.as_str(), "recorded" | "pending") {
            return None;
        }
        out.push(PanelRow {
            label: item.get("label")?.as_str()?.to_string(),
            status,
        });
    }
    Some(out)
}

/// A required, non-empty string field (mirrors the TS `isNonEmptyString` guard).
fn nonempty(value: Option<&Value>) -> Option<String> {
    let s = value?.as_str()?;
    (!s.is_empty()).then(|| s.to_string())
}

/// Parse a `TargetBounds`. Every field is required and must be finite — `as_f64`
/// rejects non-numbers, mirroring the TS `isTargetBounds` guard.
fn parse_bounds(value: &Value) -> Option<TargetBounds> {
    Some(TargetBounds {
        x: value.get("x")?.as_f64()?,
        y: value.get("y")?.as_f64()?,
        w: value.get("w")?.as_f64()?,
        h: value.get("h")?.as_f64()?,
        scale_factor: value.get("scaleFactor")?.as_f64()?,
    })
}

/// Parse the `targets_available` window list. Each entry needs a non-empty `targetId` and string
/// `title`/`appId` (which may be empty — an untitled window still picks). A present-but-malformed
/// list drops the whole cue (`?`), mirroring the strict-guard discipline of the other parsers.
fn parse_target_entries(value: &Value) -> Option<Vec<TargetEntry>> {
    let arr = value.as_array()?;
    let mut out = Vec::with_capacity(arr.len());
    for item in arr {
        out.push(TargetEntry {
            target_id: nonempty(item.get("targetId"))?,
            title: item.get("title")?.as_str()?.to_string(),
            app_id: item.get("appId")?.as_str()?.to_string(),
        });
    }
    Some(out)
}

fn parse_lost_reason(value: &str) -> Option<TargetLostReason> {
    match value {
        "closed" => Some(TargetLostReason::Closed),
        "workspaceSwitched" => Some(TargetLostReason::WorkspaceSwitched),
        "minimized" => Some(TargetLostReason::Minimized),
        "trackingFailed" => Some(TargetLostReason::TrackingFailed),
        _ => None,
    }
}

fn parse_position(value: &Value) -> Option<Position> {
    match value.get("mode")?.as_str()? {
        "anchored" => {
            let edge = parse_edge(value.get("edge")?.as_str()?)?;
            let offset = value.get("offset")?;
            let ox = offset.get("x")?.as_f64()?;
            let oy = offset.get("y")?.as_f64()?;
            Some(Position::Anchored { edge, ox, oy })
        }
        "tucked" => {
            let edge = parse_edge(value.get("edge")?.as_str()?)?;
            let offset = value.get("offset")?;
            let ox = offset.get("x")?.as_f64()?;
            let oy = offset.get("y")?.as_f64()?;
            Some(Position::Tucked { edge, ox, oy })
        }
        "free" => {
            let x = value.get("x")?.as_f64()?;
            let y = value.get("y")?.as_f64()?;
            Some(Position::Free { x, y })
        }
        _ => None,
    }
}

/// Parse a raw inbound frame into a to-body cue, or `None` if it is not a
/// well-formed v0 presence message of a kind the body acts on.
pub fn parse_to_body(text: &str) -> Option<ToBody> {
    let v: Value = serde_json::from_str(text).ok()?;

    if v.get("protocol")?.as_str()? != PROTOCOL {
        return None;
    }
    if v.get("v")?.as_u64()? != VERSION as u64 {
        return None;
    }
    let buddy = v.get("buddy")?.as_str()?.to_string();
    let ts = v.get("ts")?.as_u64()?;

    let cue = match v.get("kind")?.as_str()? {
        "express" => Cue::Express {
            emotion: v.get("emotion")?.as_str()?.to_string(),
        },
        "say" => Cue::Say {
            text: v.get("text")?.as_str()?.to_string(),
        },
        "move_to" => Cue::MoveTo {
            position: parse_position(v.get("position")?)?,
        },
        "hydrate" => Cue::Hydrate {
            position: v.get("position").and_then(parse_position),
            emotion: v.get("emotion").and_then(|e| e.as_str()).map(String::from),
            speech: v.get("speech").and_then(|s| s.as_str()).map(String::from),
            // Absent → empty (valid); present-but-malformed → `?` drops the cue.
            surfaces: match v.get("surfaces") {
                None => Vec::new(),
                Some(s) => parse_surfaces(s)?,
            },
        },
        "output" => parse_output(&v)?,
        "action_result" => parse_action_result(&v)?,
        "surface_active" => {
            let surface = nonempty(v.get("surface"))?;
            let posture = v.get("posture")?.as_str()?.to_string();
            if !matches!(posture.as_str(), "work" | "play" | "private") {
                return None;
            }
            // Absent route → None (valid); present-but-malformed → `?` drops the cue.
            let route = match v.get("route") {
                None => None,
                Some(r) => Some(parse_surface_route(r)?),
            };
            Cue::SurfaceActive {
                surface,
                posture,
                label: v.get("label").and_then(|s| s.as_str()).map(String::from),
                provider_label: v.get("providerLabel").and_then(|s| s.as_str()).map(String::from),
                route,
            }
        }
        "panel" => {
            let section = v.get("section")?.as_str()?.to_string();
            if !matches!(
                section.as_str(),
                "connect" | "posture" | "placement" | "summary" | "none"
            ) {
                return None;
            }
            Cue::Panel {
                section,
                title: v.get("title")?.as_str()?.to_string(),
                prompt: v.get("prompt").and_then(|s| s.as_str()).map(String::from),
                // Absent → empty (valid); present-but-malformed → `?` drops the cue.
                options: match v.get("options") {
                    None => Vec::new(),
                    Some(o) => parse_panel_options(o)?,
                },
                fields: match v.get("fields") {
                    None => Vec::new(),
                    Some(f) => parse_panel_fields(f)?,
                },
                rows: match v.get("rows") {
                    None => Vec::new(),
                    Some(r) => parse_panel_rows(r)?,
                },
                primary_label: v.get("primaryLabel").and_then(|s| s.as_str()).map(String::from),
                // Absent → None (valid); present must be a non-empty string, else `?` drops the
                // cue (mirrors the TS `isNonEmptyString(raw.primaryPanel)` guard).
                primary_panel: match v.get("primaryPanel") {
                    None => None,
                    Some(p) => Some(nonempty(Some(p))?),
                },
            }
        }
        "target_acquired" => Cue::TargetAcquired {
            target_id: nonempty(v.get("targetId"))?,
            title: v.get("title")?.as_str()?.to_string(),
            app_id: v.get("appId")?.as_str()?.to_string(),
            bounds: parse_bounds(v.get("bounds")?)?,
        },
        "target_moved" => Cue::TargetMoved {
            target_id: nonempty(v.get("targetId"))?,
            bounds: parse_bounds(v.get("bounds")?)?,
        },
        "target_lost" => Cue::TargetLost {
            target_id: nonempty(v.get("targetId"))?,
            reason: parse_lost_reason(v.get("reason")?.as_str()?)?,
        },
        "targets_available" => Cue::TargetsAvailable {
            targets: parse_target_entries(v.get("targets")?)?,
        },
        // attention (reserved) and all to-soul kinds are not body cues.
        _ => return None,
    };

    Some(ToBody { buddy, ts, cue })
}

/// Resolve an `anchored` position to surface (left, top) margins. Edge-relative math
/// needs the screen size; without it (`screen == None`) we can only honor offsets
/// from the top-left, so right/bottom fall back to the raw offset.
pub fn anchored_to_margins(
    edge: Edge,
    ox: f64,
    oy: f64,
    surface: (f64, f64),
    screen: Option<(f64, f64)>,
) -> (f64, f64) {
    let (surf_w, surf_h) = surface;
    let mut left = ox;
    let mut top = oy;
    if let Some((sw, sh)) = screen {
        match edge {
            Edge::Right => left = sw - surf_w - ox,
            Edge::Bottom => top = sh - surf_h - oy,
            Edge::Left | Edge::Top => {}
        }
    }
    (left, top)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The cross-language contract: this is the same fixture the TS suite checks.
    const FIXTURES: &str = include_str!("../../fixtures/presence-v0.json");

    fn fixture(kind: &str) -> String {
        let all: Value = serde_json::from_str(FIXTURES).unwrap();
        all.get(kind).unwrap().to_string()
    }

    #[test]
    fn parses_each_to_body_fixture() {
        let express = parse_to_body(&fixture("express")).unwrap();
        assert_eq!(express.buddy, "hermes");
        assert_eq!(express.cue, Cue::Express { emotion: "happy".into() });

        let say = parse_to_body(&fixture("say")).unwrap();
        assert!(matches!(say.cue, Cue::Say { .. }));

        let move_to = parse_to_body(&fixture("move_to")).unwrap();
        assert_eq!(
            move_to.cue,
            Cue::MoveTo { position: Position::Anchored { edge: Edge::Right, ox: 24.0, oy: 48.0 } }
        );

        let hydrate = parse_to_body(&fixture("hydrate")).unwrap();
        match hydrate.cue {
            Cue::Hydrate { position, emotion, speech, surfaces } => {
                assert!(position.is_some());
                assert_eq!(emotion.as_deref(), Some("neutral"));
                assert_eq!(speech.as_deref(), Some("ready"));
                // The fixture carries one descriptor per availability state.
                let states: Vec<&str> = surfaces.iter().map(|s| s.availability.as_str()).collect();
                assert_eq!(states, vec!["available", "gated", "unwired"]);
            }
            other => panic!("expected hydrate, got {other:?}"),
        }
    }

    #[test]
    fn hydrate_without_surfaces_parses_empty() {
        // The soul may omit `surfaces`; an absent list is valid and yields an empty Vec.
        let raw = r#"{"protocol":"presence","v":0,"kind":"hydrate","buddy":"h","ts":1,"emotion":"neutral"}"#;
        match parse_to_body(raw).unwrap().cue {
            Cue::Hydrate { surfaces, .. } => assert!(surfaces.is_empty()),
            other => panic!("expected hydrate, got {other:?}"),
        }
    }

    #[test]
    fn hydrate_surfaces_validate_closed_availability_set() {
        let base = r#"{"protocol":"presence","v":0,"kind":"hydrate","buddy":"h","ts":1,"surfaces":"#;
        // A bad availability drops the whole cue, mirroring the TS strict guard.
        let bad = format!("{base}[{{\"id\":\"x\",\"label\":\"X\",\"availability\":\"bogus\"}}]}}");
        assert!(parse_to_body(&bad).is_none());
        // An empty id drops it too (nonempty guard).
        let empty_id = format!("{base}[{{\"id\":\"\",\"label\":\"X\",\"availability\":\"gated\"}}]}}");
        assert!(parse_to_body(&empty_id).is_none());
        // A well-formed entry parses through.
        let ok = format!("{base}[{{\"id\":\"session\",\"label\":\"Session\",\"availability\":\"available\"}}]}}");
        match parse_to_body(&ok).unwrap().cue {
            Cue::Hydrate { surfaces, .. } => {
                assert_eq!(surfaces.len(), 1);
                assert_eq!(surfaces[0].id, "session");
                assert_eq!(surfaces[0].availability, "available");
            }
            other => panic!("expected hydrate, got {other:?}"),
        }
    }

    #[test]
    fn hydrate_parses_launcher_descriptor_and_defaults_kind_to_surface() {
        let base = r#"{"protocol":"presence","v":0,"kind":"hydrate","buddy":"h","ts":1,"surfaces":"#;
        // A surface with no `kind` defaults to "surface" and is not a launcher (back-compat).
        let surface = format!("{base}[{{\"id\":\"session\",\"label\":\"Session\",\"availability\":\"available\"}}]}}");
        match parse_to_body(&surface).unwrap().cue {
            Cue::Hydrate { surfaces, .. } => {
                assert_eq!(surfaces[0].kind, "surface");
                assert!(!surfaces[0].is_launcher());
                assert_eq!(surfaces[0].effector, None);
            }
            other => panic!("expected hydrate, got {other:?}"),
        }
        // A launcher carries kind + effector and reads back as a launcher.
        let launcher = format!(
            "{base}[{{\"id\":\"open_cursor\",\"label\":\"Open in Cursor\",\"availability\":\"gated\",\"kind\":\"launcher\",\"effector\":\"open_cursor\"}}]}}"
        );
        match parse_to_body(&launcher).unwrap().cue {
            Cue::Hydrate { surfaces, .. } => {
                assert!(surfaces[0].is_launcher());
                assert_eq!(surfaces[0].effector.as_deref(), Some("open_cursor"));
            }
            other => panic!("expected hydrate, got {other:?}"),
        }
        // An unknown `kind` drops the whole cue, mirroring the TS guard.
        let bad_kind = format!("{base}[{{\"id\":\"x\",\"label\":\"X\",\"availability\":\"gated\",\"kind\":\"bogus\"}}]}}");
        assert!(parse_to_body(&bad_kind).is_none());
    }

    #[test]
    fn parses_output_image_fixture() {
        let output = parse_to_body(&fixture("output")).unwrap();
        assert_eq!(output.buddy, "hermes");
        match output.cue {
            Cue::Output { surface, caption, media_type, data_base64, .. } => {
                assert_eq!(surface, "image");
                assert_eq!(caption.as_deref(), Some("a red bicycle"));
                assert_eq!(media_type.as_deref(), Some("image/png"));
                assert!(data_base64.is_some_and(|b| !b.is_empty()));
            }
            other => panic!("expected output, got {other:?}"),
        }
    }

    #[test]
    fn parses_target_lifecycle_fixtures() {
        let acquired = parse_to_body(&fixture("target_acquired")).unwrap();
        match acquired.cue {
            Cue::TargetAcquired { target_id, title, app_id, bounds } => {
                assert_eq!(target_id, "win-42");
                assert_eq!(title, "Firefox");
                assert_eq!(app_id, "org.mozilla.firefox");
                assert_eq!(bounds, TargetBounds { x: 320.0, y: 180.0, w: 1280.0, h: 720.0, scale_factor: 2.0 });
            }
            other => panic!("expected target_acquired, got {other:?}"),
        }

        let moved = parse_to_body(&fixture("target_moved")).unwrap();
        assert!(matches!(moved.cue, Cue::TargetMoved { ref target_id, .. } if target_id == "win-42"));

        let lost = parse_to_body(&fixture("target_lost")).unwrap();
        assert_eq!(
            lost.cue,
            Cue::TargetLost { target_id: "win-42".into(), reason: TargetLostReason::Closed }
        );
    }

    #[test]
    fn parses_targets_available_window_list() {
        // The driver's enumerated window list (the picker's first-phase data). Matches the wire
        // shape emitted by frame_driver's emit_targets_available.
        let raw = r#"{"protocol":"presence","v":0,"kind":"targets_available","buddy":"forge","ts":7,"targets":[{"targetId":"win-1","title":"Firefox","appId":"org.mozilla.firefox"},{"targetId":"win-2","title":"","appId":"com.system76.CosmicTerm"}]}"#;
        match parse_to_body(raw).unwrap().cue {
            Cue::TargetsAvailable { targets } => {
                assert_eq!(targets.len(), 2);
                assert_eq!(targets[0], TargetEntry {
                    target_id: "win-1".into(),
                    title: "Firefox".into(),
                    app_id: "org.mozilla.firefox".into(),
                });
                // An untitled window is still a valid pick (empty title allowed).
                assert_eq!(targets[1].target_id, "win-2");
                assert_eq!(targets[1].title, "");
            }
            other => panic!("expected targets_available, got {other:?}"),
        }
        // An empty list is valid (no windows to act on yet).
        let empty = r#"{"protocol":"presence","v":0,"kind":"targets_available","buddy":"forge","ts":7,"targets":[]}"#;
        assert!(matches!(parse_to_body(empty).unwrap().cue, Cue::TargetsAvailable { targets } if targets.is_empty()));
        // A missing targetId drops the whole cue (strict guard).
        let bad = r#"{"protocol":"presence","v":0,"kind":"targets_available","buddy":"forge","ts":7,"targets":[{"targetId":"","title":"x","appId":"y"}]}"#;
        assert!(parse_to_body(bad).is_none());
    }

    #[test]
    fn parses_panel_connect_fixture() {
        // The shared golden fixture's `panel` is a connect section (options + a masked paste_key
        // field + a Host-owned primary token) — the cross-language contract for Build C.
        let panel = parse_to_body(&fixture("panel")).unwrap();
        assert_eq!(panel.buddy, "hermes");
        match panel.cue {
            Cue::Panel { section, title, options, fields, primary_panel, .. } => {
                assert_eq!(section, "connect");
                assert_eq!(title, "Connect a provider");
                assert_eq!(options.len(), 2);
                assert!(options[0].selected);
                assert_eq!(options[0].id, "xai");
                // The credential field arrives as a masked paste_key control.
                let key = fields.iter().find(|f| f.key == "apiKey").unwrap();
                assert_eq!(key.control, "paste_key");
                assert!(key.masked);
                assert_eq!(primary_panel.as_deref(), Some("connection_ok"));
            }
            other => panic!("expected panel, got {other:?}"),
        }
    }

    #[test]
    fn panel_validation_mirrors_ts() {
        // `none` carries no form — the minimal close signal still parses.
        assert!(matches!(
            parse_to_body(r#"{"protocol":"presence","v":0,"kind":"panel","buddy":"host","ts":1,"section":"none","title":""}"#).unwrap().cue,
            Cue::Panel { .. }
        ));
        // Unknown section is dropped (closed set, not a free string).
        assert!(parse_to_body(r#"{"protocol":"presence","v":0,"kind":"panel","buddy":"host","ts":1,"section":"billing","title":"x"}"#).is_none());
        // Unknown field control is dropped.
        assert!(parse_to_body(r#"{"protocol":"presence","v":0,"kind":"panel","buddy":"host","ts":1,"section":"connect","title":"x","fields":[{"key":"apiKey","label":"API key","control":"biometric"}]}"#).is_none());
        // Unknown summary row status is dropped.
        assert!(parse_to_body(r#"{"protocol":"presence","v":0,"kind":"panel","buddy":"host","ts":1,"section":"summary","title":"x","rows":[{"label":"Posture","status":"halfway"}]}"#).is_none());
        // An option missing its id, and a present-but-empty primaryPanel, both drop the cue.
        assert!(parse_to_body(r#"{"protocol":"presence","v":0,"kind":"panel","buddy":"host","ts":1,"section":"posture","title":"x","options":[{"label":"Work"}]}"#).is_none());
        assert!(parse_to_body(r#"{"protocol":"presence","v":0,"kind":"panel","buddy":"host","ts":1,"section":"connect","title":"x","primaryPanel":""}"#).is_none());
    }

    #[test]
    fn target_validation_mirrors_ts() {
        // Empty targetId is dropped.
        assert!(parse_to_body(r#"{"protocol":"presence","v":0,"kind":"target_moved","buddy":"h","ts":1,"targetId":"","bounds":{"x":0,"y":0,"w":1,"h":1,"scaleFactor":1}}"#).is_none());
        // Bounds missing a field is dropped.
        assert!(parse_to_body(r#"{"protocol":"presence","v":0,"kind":"target_moved","buddy":"h","ts":1,"targetId":"w","bounds":{"x":0,"y":0,"w":1,"h":1}}"#).is_none());
        // Unknown lost reason is dropped (closed union, not a free string).
        assert!(parse_to_body(r#"{"protocol":"presence","v":0,"kind":"target_lost","buddy":"h","ts":1,"targetId":"w","reason":"vanished"}"#).is_none());
    }

    #[test]
    fn output_validation_mirrors_ts() {
        // text surface: a text body is enough.
        assert!(matches!(
            parse_to_body(r#"{"protocol":"presence","v":0,"kind":"output","buddy":"h","ts":1,"surface":"text","text":"hi"}"#).unwrap().cue,
            Cue::Output { .. }
        ));
        // session surface: no payload required (a clear signal).
        assert!(parse_to_body(r#"{"protocol":"presence","v":0,"kind":"output","buddy":"h","ts":1,"surface":"session"}"#).is_some());
        // image without inline bytes is dropped.
        assert!(parse_to_body(r#"{"protocol":"presence","v":0,"kind":"output","buddy":"h","ts":1,"surface":"image","mediaType":"image/png"}"#).is_none());
        // unknown surface is dropped.
        assert!(parse_to_body(r#"{"protocol":"presence","v":0,"kind":"output","buddy":"h","ts":1,"surface":"hologram"}"#).is_none());
    }

    #[test]
    fn parses_tucked_position() {
        // The `tucked` mode arrives via move_to/hydrate just like anchored; the
        // dropped fixture carries one too, but dropped is to-soul so it can't reach
        // this path. Drive it through move_to to prove the body parses tucked.
        let msg = parse_to_body(
            r#"{"protocol":"presence","v":0,"kind":"move_to","buddy":"hermes","ts":1,"position":{"mode":"tucked","edge":"left","offset":{"x":0,"y":400}}}"#,
        )
        .unwrap();
        assert_eq!(
            msg.cue,
            Cue::MoveTo { position: Position::Tucked { edge: Edge::Left, ox: 0.0, oy: 400.0 } }
        );
    }

    #[test]
    fn to_soul_and_attention_fixtures_are_not_body_cues() {
        for kind in ["attached", "clicked", "grabbed", "dropped", "summoned", "dismissed", "said", "attention", "action_request", "surface_request"] {
            assert!(parse_to_body(&fixture(kind)).is_none(), "{kind} should not be a body cue");
        }
    }

    #[test]
    fn parses_surface_active_fixture() {
        let active = parse_to_body(&fixture("surface_active")).unwrap();
        assert_eq!(active.buddy, "hermes");
        assert_eq!(
            active.cue,
            Cue::SurfaceActive {
                surface: "private_local_chat".into(),
                posture: "private".into(),
                label: Some("Private local chat".into()),
                provider_label: Some("LM Studio".into()),
                // The nested route crosses the wire and parses identically; `health` is
                // soul-derived and optional for older surface_active cues.
                route: Some(SurfaceRoute {
                    label: "LM Studio".into(),
                    locality: "local".into(),
                    health: Some("ready".into()),
                }),
            }
        );
    }

    #[test]
    fn surface_active_route_validates_closed_sets() {
        let base = r#"{"protocol":"presence","v":0,"kind":"surface_active","buddy":"h","ts":1,"surface":"claude_code","posture":"work""#;
        // A present-but-malformed route (bad locality) drops the whole cue.
        assert!(parse_to_body(&format!(r#"{base},"route":{{"label":"Codex","locality":"orbit"}}}}"#)).is_none());
        // A bad health value also drops it.
        assert!(parse_to_body(&format!(r#"{base},"route":{{"label":"Codex","locality":"cloud","health":"flaky"}}}}"#)).is_none());
        // A well-formed route (with optional health) parses.
        let ok = parse_to_body(&format!(r#"{base},"route":{{"label":"Codex","locality":"cloud","health":"degraded"}}}}"#)).unwrap();
        match ok.cue {
            Cue::SurfaceActive { route, .. } => {
                let route = route.expect("well-formed route parses");
                assert_eq!(route.locality, "cloud");
                assert_eq!(route.health.as_deref(), Some("degraded"));
            }
            other => panic!("expected SurfaceActive, got {other:?}"),
        }
        // An absent route is valid (additive field): the cue still parses.
        assert!(parse_to_body(&format!(r#"{base}}}"#)).is_some());
    }

    #[test]
    fn parses_action_result_fixture() {
        let result = parse_to_body(&fixture("action_result")).unwrap();
        assert_eq!(result.buddy, "hermes");
        match result.cue {
            Cue::ActionResult { effector, decision, receipt_id, request_id, summary, outcome } => {
                assert_eq!(effector, "receipt_review");
                assert_eq!(decision, "allow");
                assert!(!receipt_id.is_empty());
                assert_eq!(request_id.as_deref(), Some("req-1"));
                assert!(summary.is_some());
                // Execution outcome + route provenance cross the wire and parse identically.
                let outcome = outcome.expect("fixture carries an execution outcome");
                assert!(outcome.executed);
                assert_eq!(outcome.execution_receipt_id.as_deref(), Some("exec:hermes:receipt_review:t0"));
                let route = outcome.route.expect("fixture carries a route");
                assert_eq!(route.provider, "claude");
                assert_eq!(route.locality, "cloud");
                assert!(!route.downgraded);
            }
            other => panic!("expected action_result, got {other:?}"),
        }
    }

    #[test]
    fn action_outcome_validation_mirrors_ts() {
        // No outcome at all is fine (blocked/needs_confirmation paths).
        assert!(parse_to_body(r#"{"protocol":"presence","v":0,"kind":"action_result","buddy":"h","ts":1,"effector":"repo_edit","decision":"blocked","receiptId":"r1"}"#).is_some());
        // A non-boolean `executed` drops the whole cue.
        assert!(parse_to_body(r#"{"protocol":"presence","v":0,"kind":"action_result","buddy":"h","ts":1,"effector":"repo_edit","decision":"allow","receiptId":"r1","outcome":{"executed":"yes"}}"#).is_none());
        // An unknown route locality drops the cue (closed set, not a free string).
        assert!(parse_to_body(r#"{"protocol":"presence","v":0,"kind":"action_result","buddy":"h","ts":1,"effector":"repo_edit","decision":"allow","receiptId":"r1","outcome":{"executed":true,"route":{"provider":"gpt","locality":"orbit","downgraded":false}}}"#).is_none());
        // A well-formed outcome with a downgraded route parses.
        assert!(parse_to_body(r#"{"protocol":"presence","v":0,"kind":"action_result","buddy":"h","ts":1,"effector":"repo_edit","decision":"allow","receiptId":"r1","outcome":{"executed":true,"route":{"provider":"gpt","locality":"cloud","downgraded":true,"fallbackOf":"claude"}}}"#).is_some());
    }

    #[test]
    fn action_result_validation_mirrors_ts() {
        // Unknown decision literal is dropped (closed union, not a free string).
        assert!(parse_to_body(r#"{"protocol":"presence","v":0,"kind":"action_result","buddy":"h","ts":1,"effector":"receipt_review","decision":"maybe","receiptId":"r1"}"#).is_none());
        // Empty effector is dropped.
        assert!(parse_to_body(r#"{"protocol":"presence","v":0,"kind":"action_result","buddy":"h","ts":1,"effector":"","decision":"allow","receiptId":"r1"}"#).is_none());
        // Empty receiptId is dropped.
        assert!(parse_to_body(r#"{"protocol":"presence","v":0,"kind":"action_result","buddy":"h","ts":1,"effector":"receipt_review","decision":"allow","receiptId":""}"#).is_none());
        // Minimal valid (no optional requestId/summary) parses.
        assert!(parse_to_body(r#"{"protocol":"presence","v":0,"kind":"action_result","buddy":"h","ts":1,"effector":"receipt_review","decision":"blocked","receiptId":"r1"}"#).is_some());
    }

    #[test]
    fn action_request_builder_emits_valid_envelope() {
        // Bare request: effector only, no confirmed/requestId fields.
        let a: Value = serde_json::from_str(&action_request_json("hermes", "receipt_review", false, None)).unwrap();
        assert_eq!(a["protocol"], "presence");
        assert_eq!(a["kind"], "action_request");
        assert_eq!(a["buddy"], "hermes");
        assert_eq!(a["effector"], "receipt_review");
        assert!(a.get("confirmed").is_none(), "confirmed omitted when false");
        assert!(a.get("requestId").is_none(), "requestId omitted when absent");

        // Confirmation follow-up carries confirmed:true and the correlating requestId.
        let c: Value = serde_json::from_str(&action_request_json("hermes", "receipt_review", true, Some("req-9"))).unwrap();
        assert_eq!(c["confirmed"], true);
        assert_eq!(c["requestId"], "req-9");
    }

    #[test]
    fn action_request_intent_builder_emits_typed_intent() {
        // A repo_edit effect aimed at a sandbox path — the typed intent the soul lifts + gates.
        let intent = ActionIntent {
            operation: "write_patch",
            target_kind: "repo_path",
            target_value: Some(".border-agents/proofs/notes.md"),
            summary: Some("write notes.md"),
            payload_digest: None,
        };
        let a: Value =
            serde_json::from_str(&action_request_intent_json("hermes", "repo_edit", &intent, false, Some("req-edit-1")))
                .unwrap();
        assert_eq!(a["protocol"], "presence");
        assert_eq!(a["kind"], "action_request");
        assert_eq!(a["effector"], "repo_edit");
        assert_eq!(a["requestId"], "req-edit-1");
        assert!(a.get("confirmed").is_none(), "confirmed omitted when false");
        // The typed intent rides in the `intent` object the soul validates with isActionIntent.
        assert_eq!(a["intent"]["operation"], "write_patch");
        assert_eq!(a["intent"]["target"]["kind"], "repo_path");
        assert_eq!(a["intent"]["target"]["value"], ".border-agents/proofs/notes.md");
        assert_eq!(a["intent"]["summary"], "write notes.md");
        assert!(a["intent"].get("payloadDigest").is_none(), "absent optional omitted");

        // A grant-only / value-less target emits a bare `none` kind with no value key, and the
        // confirm follow-up carries confirmed:true.
        let grant_only = ActionIntent {
            operation: "noop",
            target_kind: "none",
            target_value: None,
            summary: None,
            payload_digest: Some("sha256:abc"),
        };
        let g: Value =
            serde_json::from_str(&action_request_intent_json("hermes", "repo_edit", &grant_only, true, None)).unwrap();
        assert_eq!(g["confirmed"], true);
        assert_eq!(g["intent"]["target"]["kind"], "none");
        assert!(g["intent"]["target"].get("value").is_none(), "value omitted when None");
        assert_eq!(g["intent"]["payloadDigest"], "sha256:abc");
        assert!(g["intent"].get("summary").is_none());
    }

    #[test]
    fn commandeer_request_builder_emits_gated_command_intent() {
        // Monitor: a `commandeer` action_request carrying the window as a `command` target and the
        // mode as the operation — exactly what the soul lifts to intent.target.path + operation
        // for dispatchCommandeer. No `text`: control-text is the soul's to add, never the body's.
        let m: Value =
            serde_json::from_str(&commandeer_request_json("forge", "monitor", "win-42", "Firefox", false))
                .unwrap();
        assert_eq!(m["protocol"], "presence");
        assert_eq!(m["kind"], "action_request");
        assert_eq!(m["buddy"], "forge");
        assert_eq!(m["effector"], "commandeer");
        assert_eq!(m["intent"]["operation"], "monitor");
        assert_eq!(m["intent"]["target"]["kind"], "command");
        assert_eq!(m["intent"]["target"]["value"], "win-42");
        assert_eq!(m["intent"]["summary"], "monitor Firefox");
        assert!(m.get("confirmed").is_none(), "confirmed omitted when false");
        assert!(m["intent"].get("payloadDigest").is_none());

        // Control confirm follow-up carries confirmed:true and the same command target.
        let c: Value =
            serde_json::from_str(&commandeer_request_json("forge", "control", "win-7", "Editor", true))
                .unwrap();
        assert_eq!(c["confirmed"], true);
        assert_eq!(c["intent"]["operation"], "control");
        assert_eq!(c["intent"]["target"]["value"], "win-7");
    }

    #[test]
    fn surface_request_builder_emits_valid_envelope() {
        let a: Value = serde_json::from_str(&surface_request_json("aether", "private_local_chat")).unwrap();
        assert_eq!(a["protocol"], "presence");
        assert_eq!(a["kind"], "surface_request");
        assert_eq!(a["buddy"], "aether");
        assert_eq!(a["surface"], "private_local_chat");
    }

    #[test]
    fn rejects_malformed_and_wrong_version() {
        assert!(parse_to_body("not json").is_none());
        assert!(parse_to_body(r#"{"protocol":"presence","v":1,"kind":"say","buddy":"h","ts":1,"text":"x"}"#).is_none());
        assert!(parse_to_body(r#"{"protocol":"gateway","v":0,"kind":"say","buddy":"h","ts":1,"text":"x"}"#).is_none());
        // move_to with an incomplete offset
        assert!(parse_to_body(r#"{"protocol":"presence","v":0,"kind":"move_to","buddy":"h","ts":1,"position":{"mode":"anchored","edge":"right","offset":{"x":1}}}"#).is_none());
    }

    #[test]
    fn anchored_mapping_honors_each_edge() {
        let surface = (320.0, 360.0);
        let screen = Some((1000.0, 800.0));
        assert_eq!(anchored_to_margins(Edge::Left, 24.0, 48.0, surface, screen), (24.0, 48.0));
        assert_eq!(anchored_to_margins(Edge::Top, 24.0, 48.0, surface, screen), (24.0, 48.0));
        assert_eq!(anchored_to_margins(Edge::Right, 24.0, 48.0, surface, screen), (1000.0 - 320.0 - 24.0, 48.0));
        assert_eq!(anchored_to_margins(Edge::Bottom, 24.0, 48.0, surface, screen), (24.0, 800.0 - 360.0 - 48.0));
    }

    #[test]
    fn anchored_mapping_falls_back_without_screen() {
        let surface = (320.0, 360.0);
        // Right/bottom can't be resolved without a screen — honor the raw offset.
        assert_eq!(anchored_to_margins(Edge::Right, 24.0, 48.0, surface, None), (24.0, 48.0));
    }

    #[test]
    fn to_soul_builders_emit_valid_envelopes() {
        let g: Value = serde_json::from_str(&grabbed_json("hermes", 100.0, 200.0)).unwrap();
        assert_eq!(g["protocol"], "presence");
        assert_eq!(g["v"], 0);
        assert_eq!(g["kind"], "grabbed");
        assert_eq!(g["buddy"], "hermes");
        assert_eq!(g["at"]["mode"], "free");
        assert_eq!(g["at"]["space"], "screen");
        assert_eq!(g["at"]["x"], 100.0);
        assert_eq!(g["at"]["y"], 200.0);

        let c: Value = serde_json::from_str(&clicked_json("hermes", 1.0, 2.0)).unwrap();
        assert_eq!(c["kind"], "clicked");
        assert_eq!(c["button"], "primary");
        assert_eq!(c["at"]["mode"], "free");

        let d: Value = serde_json::from_str(&dropped_json("hermes", 3.0, 4.0)).unwrap();
        assert_eq!(d["kind"], "dropped");
        assert_eq!(d["at"]["x"], 3.0);
        assert_eq!(d["at"]["y"], 4.0);

        let s: Value = serde_json::from_str(&said_json("hermes", "hello buddy")).unwrap();
        assert_eq!(s["kind"], "said");
        assert_eq!(s["buddy"], "hermes");
        assert_eq!(s["text"], "hello buddy");

        let drag: Value =
            serde_json::from_str(&target_drag_requested_json("hermes", "win-42", 12.0, -4.0))
                .unwrap();
        assert_eq!(drag["kind"], "target_drag_requested");
        assert_eq!(drag["targetId"], "win-42");
        assert_eq!(drag["delta"]["x"], 12.0);
        assert_eq!(drag["delta"]["y"], -4.0);
    }

    #[test]
    fn tuck_summon_builders_emit_valid_envelopes() {
        // Tucked to the left edge: along-axis is y, flush axis (x) is 0.
        let dl: Value = serde_json::from_str(&dropped_tucked_json("hermes", Edge::Left, 400.0)).unwrap();
        assert_eq!(dl["kind"], "dropped");
        assert_eq!(dl["at"]["mode"], "tucked");
        assert_eq!(dl["at"]["edge"], "left");
        assert_eq!(dl["at"]["offset"]["x"], 0.0);
        assert_eq!(dl["at"]["offset"]["y"], 400.0);

        // Tucked to the top edge: along-axis is x.
        let dt: Value = serde_json::from_str(&dropped_tucked_json("hermes", Edge::Top, 250.0)).unwrap();
        assert_eq!(dt["at"]["edge"], "top");
        assert_eq!(dt["at"]["offset"]["x"], 250.0);
        assert_eq!(dt["at"]["offset"]["y"], 0.0);

        // A tucked drop round-trips back through the inbound parser as Tucked.
        let restored = parse_position(&dl["at"]).unwrap();
        assert_eq!(restored, Position::Tucked { edge: Edge::Left, ox: 0.0, oy: 400.0 });

        let s: Value = serde_json::from_str(&summoned_json("hermes")).unwrap();
        assert_eq!(s["kind"], "summoned");
        assert_eq!(s["buddy"], "hermes");

        let dm: Value = serde_json::from_str(&dismissed_json("hermes")).unwrap();
        assert_eq!(dm["kind"], "dismissed");
    }
}
