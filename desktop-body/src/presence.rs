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

/// Spawn the presence client thread. Returns immediately; the body runs with or
/// without a soul. `inbound` forwards raw inbound JSON frames into the calloop loop.
pub fn spawn(inbound: Sender<String>) {
    let url = std::env::var("BB_PRESENCE_URL").unwrap_or_else(|_| DEFAULT_URL.to_string());
    let buddy = std::env::var("BB_BUDDY").unwrap_or_else(|_| "hermes".to_string());

    if let Err(err) = thread::Builder::new()
        .name("bb-presence".into())
        .spawn(move || run(url, buddy, inbound))
    {
        eprintln!("[bb-presence] could not start presence thread: {err} — running standalone");
    }
}

fn run(url: String, buddy: String, inbound: Sender<String>) {
    let mut backoff = INITIAL_BACKOFF;

    loop {
        match connect(&url) {
            Ok((mut socket, _response)) => {
                eprintln!("[bb-presence] connected to {url} as '{buddy}'");
                backoff = INITIAL_BACKOFF; // reset after a good connection

                if announce(&mut socket, &buddy).is_ok() {
                    set_read_timeout(&socket, Some(READ_TIMEOUT));
                    if pump(&mut socket, &inbound).is_break() {
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
                // Idle read timeout — nothing to read. (Later: drain outbound here.)
            }
            Err(err) => {
                eprintln!("[bb-presence] read error: {err} — reconnecting");
                return ControlFlow::Continue(());
            }
        }
    }
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
