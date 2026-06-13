# Step 4 — Wire the Soul: implementation plan

**Status:** approved (Fable review) with amendments folded in. The original
`attached` addition is done in TS; the Rust body now has the presence WebSocket
client, inbound cue application, outbound interaction events, and an experimental
COSMIC target-pin proof. **Branch:** `presence-layer`.

**Amendments applied vs the original draft:**
1. Added an `attached` lifecycle handshake kind to protocol v0 (body → soul on
   (re)connect → soul replies `hydrate`). Distinct from `summoned`, which means the
   *user* opened the surface. Landed first in TS + JS mirror + tests. ✅
2. v1 emits **`grabbed` + `dropped` only** — no per-motion `dragged` stream (no
   consumer yet; add when something reasons over trajectories).
3. A shared **golden fixture** (`fixtures/presence-v0.json`, `npm run gen:fixtures`)
   is read by both the vitest suite and the Rust `cargo test`, making cross-language
   protocol parity a test, not a hope. ✅
4. Experimental target lifecycle cues (`target_acquired` / `target_moved` /
   `target_lost`) are presentation-only. They let the body visually pin Hermes to a
   tracked native window, but do not grant screen read or screen action capability.
   Any future target action remains a soul effector routed through Core Patrol.

## 0. Current live proof (2026-06-13)

- `desktop-body` has a sync tungstenite presence client. The body keeps running if
  the gateway is absent, reconnects when it appears, and filters cues by buddy id.
- `scripts/gateway-dev.mjs` can relay target lifecycle cues from a platform helper to
  bodies, while keeping effectors as separate to-soul/to-driver messages.
- `desktop-body/src/bin/frame_driver.rs` is a COSMIC/Wayland helper that discovers a
  target window (default match: Firefox) and emits `target_acquired`, `target_moved`,
  and `target_lost` envelopes.
- The first full-window "clay frame" experiment proved the geometry path but was too
  heavy. The chosen UX direction is now **pinned mode**: right-click Hermes to
  pin/unpin to the tracked target; left-click the pinned head opens the input; drag
  the pinned head to choose an attachment offset that follows the target window.
- This remains within AGENTS.md law 7: the body tracks and renders presence only. It
  does not read the target window, move the target window, click it, or decide
  authority.

**Goal:** the native Rust body (`desktop-body/`) stops puppeting itself and is driven
by a soul over the presence-protocol WebSocket — consuming `express/say/move_to/
hydrate`, emitting `clicked/grabbed/dragged/dropped/summoned/dismissed`. First driver
is the scripted Wizard onboarding host. Same protocol the browser body already speaks
(`src/presenceProtocol.ts`, JS mirror `extensions/browser/presence.js`).

## 0b. Governance parity over the wire — the real soul (2026-06-13) ✅

The gap flagged in the `What It Is` reviewer note ("native body does not run the
governance action path; receipts are browser-only") is closed. The dev gateway is a JS
relay that cannot import the TS core, so its `action_request` handler only ever emitted a
`gateway-stub` `action_result`. **`scripts/soul-server.ts`** (run via tsx, `npm run
soul:dev`) is the real soul runtime the architecture always implied: it serves the presence
protocol over WebSocket and runs the **actual** gate (`handleActionRequest` →
`authorizeEffectorAction`), the same one the browser body calls in-process.

- Binds `ws://127.0.0.1:17387/border-buddies` (the body's default `BB_PRESENCE_URL`), so
  `BB_BUDDY=owl npm run body:dev` connects with no config.
- `attached` → replies `hydrate` + greeting; `said` → `parseActionCommand` routes `/review`
  / `/confirm` through the gate; a typed `action_request` cue is authorized directly.
- Emits a **real** `action_result` (real `receiptId`, real decision) and persists the
  receipt to a file-backed ledger (`BB_SOUL_LEDGER`), so the native body now sees the exact
  needs_confirmation → confirm → allow round-trip the browser proves — including the
  persona→governance id resolution (`owl` → `veritas`) over the wire.
- Verified by a live WebSocket smoke run and 154 green vitest (incl. `parseActionCommand`).
- AGENTS.md law 7 intact: the body sends text / an action request; the SOUL authorizes; the
  body only renders the result. No new "do something" kinds were added to the body.

On-body Review/Confirm affordance (2026-06-13) ✅ — a Review button appears when chat is
open; click emits `action_request` for `receipt_review`; on `needs_confirmation` the
button flips to Confirm. Verified by compile + layout regression test; live COSMIC click
not yet verified headlessly. Remaining for full Step 4: Wizard Act 0 host (commit 5 below).

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
| `target_acquired` | `targetId`, `title`, `appId`, `bounds` | store as the current pin candidate; do not auto-act on the target |
| `target_moved` | `targetId`, `bounds` | update the tracked bounds and move the pinned surface using the user-chosen offset |
| `target_lost` | `targetId`, `reason?` | clear candidate/pinned state if it matches the current target |

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

Target-control note: a body-originated request like `target_drag_requested` may be
useful as a future UI gesture, but it must remain a request to the soul/effectors, not
a direct body capability. Do not wire screen action into `desktop-body`.

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
4. `feat(presence): COSMIC target lifecycle relay + pinned Hermes proof` — frame
   driver, target cue schema/fixtures, gateway relay, pinned head/bubble/input UX.
5. `feat(gateway): wizard onboarding host (Act 0)` behind `BB_SOUL=wizard`.
6. `feat(soul): real presence soul-server runs the action gate over WebSocket` —
   `scripts/soul-server.ts` + `parseActionCommand` + tests. **✅ done (governance parity).**
7. `feat(presence): native body Review/Confirm affordance` — on-body button, layout test.
   **✅ done (live COSMIC click pending manual verify).**

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
