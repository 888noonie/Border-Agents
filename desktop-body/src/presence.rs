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
    /// Border-target tracking (the "Morph Frame" seam): a platform driver tells the body
    /// where a native OS window is so it can wrap its hollow torso around it. Split three
    /// ways so the renderer binds distinct behavior to the lifecycle; every cue carries
    /// `target_id`, and `TargetAcquired` carries initial `bounds` so there is no
    /// empty-handed gap before the first move.
    TargetAcquired { target_id: String, title: String, app_id: String, bounds: TargetBounds },
    TargetMoved { target_id: String, bounds: TargetBounds },
    TargetLost { target_id: String, reason: TargetLostReason },
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
        },
        "output" => parse_output(&v)?,
        "action_result" => parse_action_result(&v)?,
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
        // attention (reserved) and all to-soul kinds are not body cues.
        _ => return None,
    };

    Some(ToBody { buddy, cue })
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
            Cue::Hydrate { position, emotion, speech } => {
                assert!(position.is_some());
                assert_eq!(emotion.as_deref(), Some("neutral"));
                assert_eq!(speech.as_deref(), Some("ready"));
            }
            other => panic!("expected hydrate, got {other:?}"),
        }
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
        for kind in ["attached", "clicked", "grabbed", "dropped", "summoned", "dismissed", "said", "attention", "action_request"] {
            assert!(parse_to_body(&fixture(kind)).is_none(), "{kind} should not be a body cue");
        }
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
