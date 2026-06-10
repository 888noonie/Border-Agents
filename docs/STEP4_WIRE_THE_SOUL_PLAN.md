# Step 4 — Wire the Soul: implementation plan

**Status:** approved (Fable review) with 3 amendments folded in; commit 1 (protocol
`attached` addition) **done in TS**. **Branch:** `presence-layer`.

**Amendments applied vs the original draft:**
1. Added an `attached` lifecycle handshake kind to protocol v0 (body → soul on
   (re)connect → soul replies `hydrate`). Distinct from `summoned`, which means the
   *user* opened the surface. Landed first in TS + JS mirror + tests. ✅
2. v1 emits **`grabbed` + `dropped` only** — no per-motion `dragged` stream (no
   consumer yet; add when something reasons over trajectories).
3. A shared **golden fixture** (`fixtures/presence-v0.json`, `npm run gen:fixtures`)
   is read by both the vitest suite and the Rust `cargo test`, making cross-language
   protocol parity a test, not a hope. ✅

**Goal:** the native Rust body (`desktop-body/`) stops puppeting itself and is driven
by a soul over the presence-protocol WebSocket — consuming `express/say/move_to/
hydrate`, emitting `clicked/grabbed/dragged/dropped/summoned/dismissed`. First driver
is the scripted Wizard onboarding host. Same protocol the browser body already speaks
(`src/presenceProtocol.ts`, JS mirror `extensions/browser/presence.js`).

## 1. Integration approach — thread + calloop channel (recommended)

The body is a synchronous `calloop` + Wayland loop (`main.rs:162`). Do **not** pull in
tokio. Instead:

- One **WS thread** owns a blocking `tungstenite` connection. It sets a short
  `set_read_timeout` on the underlying `TcpStream` and loops: try-read a frame
  (timeout → `WouldBlock`, fine), then drain an outbound `std::sync::mpsc` queue and
  write any pending to-soul frames. So one thread does both directions, no split.
- Inbound frames are parsed → `InboundCue` and pushed into a **`calloop::channel::
  Sender`**. The main loop gets them as a new event source (`handle.insert_source`),
  on the same thread as `App`, so applying them touches Wayland safely.
- Outbound: `App` holds the `mpsc::Sender<String>`; pointer logic pushes serialized
  to-soul JSON; the WS thread writes it.

New deps (small, sync): `tungstenite`, `url`, `serde`, `serde_json`. calloop's
`channel` feature.

```
 WS thread (tungstenite, blocking + read timeout)
   ── inbound frame → parse → InboundCue ──▶ calloop::channel ──▶ App (main thread)
   ◀── outbound JSON ◀── mpsc ◀── App pointer logic
```

Rationale: smallest footprint, stays in the existing loop, no async runtime in a
currently-sync binary. (Alternative considered: tokio + tokio-tungstenite — more
idiomatic for heavy networking later, rejected for now as overkill.)

## 2. Message mapping

### Inbound (to-body → App). Mirror the TS strict parser: **drop malformed, never crash.**

| Wire kind  | Payload                              | App effect |
|------------|--------------------------------------|------------|
| `express`  | `emotion`, `intensity?`, `pose?`     | `Emotion::from_wire(emotion)` → `set_emotion`; unknown emotion → drop cue (render.rs:87 already returns `Option`) |
| `say`      | `text`, `ttlMs?`                     | `say(text)`; `ttlMs` schedules auto-clear via a calloop `Timer` (optional v1: ignore TTL) |
| `move_to`  | `position`, `transitionMs?`          | resolve position → target `(margin_left, margin_top)`, `clamp_margins`, `reposition` (see §3); `transitionMs` → snap in v1, lerp later |
| `hydrate`  | `position?`, `emotion?`, `speech?`   | apply each present field once (reconnect/late-join snapshot) |
| `attention`| `focus`                              | v1: no-op (no gaze yet); reserve for eye-glance later |

### Outbound (App → to-soul). Built with the same envelope shape (`protocol:"presence"`, `v`, `kind`, `buddy`, `ts`).

| App moment (existing code)                | Emit | Payload |
|-------------------------------------------|------|---------|
| WS (re)connect (handshake)                | `attached` | `at` = current position; `capabilities` e.g. `["drag","menu","say"]` → soul replies `hydrate` |
| `on_press` head, drag begins (main.rs:391)| `grabbed` | `at` = current position |
| `on_release`, was a drag                  | `dropped` | `at` (final position — the soul learns a drag happened + where it ended; no intermediate stream) |
| `on_release`, click on head → menu opens  | `clicked` + `summoned` | clicked `at`; summoned when menu opens |
| menu closes (toggle off)                  | `dismissed` | — |
| `activate_menu_item`                      | `clicked` | optional, with item index in a label |

**No `dragged` stream in v1** (amendment 2): `grabbed`→`dropped` already proves a drag
occurred and where it ended — all any current consumer (incl. Wizard Act 4) needs.

## 3. Position mapping (the only non-trivial bit)

Body is anchored `TOP|LEFT` with `(margin_left, margin_top)` = surface top-left
(main.rs:141-143). `PresencePosition` is abstract; the body maps it:

- `anchored { edge, offset }` → compute absolute surface top-left against
  `app.screen (sw, sh)` and `SURFACE_W/H`:
  - left edge: `margin_left = offset.x`; right: `sw - SURFACE_W - offset.x`
  - top: `margin_top = offset.y`; bottom: `sh - SURFACE_H - offset.y`
  - corner edges combine both axes; then `clamp_margins()` (already head-aware).
- `free { space: "screen", x, y }` → treat `(x,y)` as desired surface top-left for v1
  (document this; later map to head-center). `space: "buddy"|"surface"` → ignore in v1.
- Keep anchor `TOP|LEFT` fixed; we move by margins only — consistent with how drag
  already works, so `move_to` and drag share one code path.

If `app.screen` is `None`, skip edge-relative math (can't resolve right/bottom) and
apply only left/top offsets.

## 4. Connection lifecycle

- URL from env: `BB_PRESENCE_URL` (default `ws://127.0.0.1:17387/...path...` — match
  `gateway-dev.mjs` PATH). Buddy id from `BB_BUDDY` (default `hermes`).
- **Connect is optional/non-fatal:** if the gateway is down, the body still runs
  standalone (today's behavior). Log and retry.
- **Reconnect with backoff:** WS thread retries on drop — 0.5s → cap 5s, jittered. On
  every (re)connect, the body sends **`attached`** (buddy id, current position,
  capabilities); the soul replies with `hydrate`. Deterministic handshake, distinct
  kind (amendment 1).
- Filter inbound by `buddy` == ours; ignore others (multi-buddy ready).

## 5. Error handling

- Malformed/unknown JSON or kind → **drop the single cue**, log at debug, keep the
  connection. Never panic (mirrors `parsePresenceMessage` returning null in TS).
- Send failure / socket error → mark disconnected, let the WS thread reconnect; App
  keeps running.
- Channel disconnected (WS thread gone) → App continues standalone.
- Shutdown: on `app.exit`, drop the outbound sender so the WS thread's writer loop
  ends; thread is detached/joined best-effort.

## 6. Gateway side — Wizard host replaces the echo soul

In `scripts/gateway-dev.mjs`, behind a flag (`BB_SOUL=wizard`, default keeps echo):
drive **Act 0** of `docs/WIZARD_ONBOARDING_SCRIPT.md` — on body connect, send
`express(curious)` + `say("Hi — I'm your setup host. Ready?")`; on `clicked`/
`summoned`, advance to the next beat. Full Acts 1+ come once the panel exists; Step 4
proves Act 0 end-to-end.

## 7. Testing

- **Rust unit tests** (pure, no Wayland): position mapping (`anchored`/`free` →
  margins for each edge, incl. `screen=None`), inbound parse (valid/malformed/unknown
  kind → `Option<InboundCue>`), outbound envelope serialization round-trips against a
  known-good JSON sample copied from the TS factories.
- **Manual e2e:** `npm run gateway:dev` (wizard flag) + `BB body start`; confirm the
  buddy greets on connect, reacts to `express`/`say`, repositions on `move_to`, and
  the gateway logs `clicked/grabbed/dragged/dropped` as you interact.
- Keep the JS suite green (no TS changes expected; protocol already shipped).

## 8. Commit breakdown (small, reviewable)

0. `feat(presence): attached handshake kind + golden fixtures` — protocol v0 + JS
   mirror + tests + `fixtures/presence-v0.json`. **✅ done (TS), lands before Rust.**
1. `feat(desktop-body): presence WS client scaffold` — thread, calloop channel, deps,
   connect/reconnect, sends `attached` on connect, logs only.
2. `feat(desktop-body): apply inbound presence cues` — express/say/move_to/hydrate +
   position mapping + Rust unit tests (incl. reading the golden fixture).
3. `feat(desktop-body): emit to-soul events` — attached/clicked/grabbed/dropped/
   summoned/dismissed (no `dragged` stream).
4. `feat(gateway): wizard onboarding host (Act 0)` behind `BB_SOUL=wizard`.

## 9. Decisions (resolved in review)

1. **Reconnect handshake:** body sends `attached`; soul replies `hydrate`. Distinct
   kind, never `summoned`. ✅
2. **`move_to` motion:** snap in v1; lerp is a later polish commit.
3. **`attention/focus`:** defer — reserve the kind, no-op the handler, no head-turn stub.
4. **Drag reporting:** `grabbed` + `dropped` only; no `dragged` stream in v1.

## 10. Trajectory guard (hold during implementation)

- The WS client must **never** grow kinds that make the body *do* things (open URL,
  read screen, run task). The body's vocabulary is **presentation in, interaction
  out** — full stop. Anything more is a soul effector through Core Patrol (AGENTS.md
  law 7).
- The dev gateway has **no auth** on its localhost socket — acceptable dev posture;
  must be revisited before anything binds beyond `127.0.0.1`.
