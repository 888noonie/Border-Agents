# AGENTS.md

Guidance for AI coding assistants working in this repository.

## Project stance

Border Agents is a visible interface and governance layer for AI work. It makes trust boundaries inspectable before AI outputs become memory, claims, actions, code changes, or shared artifacts.

The first build target is intentionally narrow:

```text
same vector results
+ different purposes
= different authorized Safe Context Frames
```

Do not expand into a general agent framework until the memory grading primitive is proven.

## Translation rule

Use the project owner's four-layer rule:

```text
Vision layer: metaphor allowed
Architecture layer: metaphor translated
API layer: metaphor removed
Test layer: metaphor forbidden
```

That means:

- Vision docs may use terms like border, patrol, crossing, safe passage.
- Architecture docs must translate those into boundaries, states, policies, frames, receipts, and UI events.
- API code must use plain technical names.
- Tests must use concrete inputs, outputs, and assertions only.

## Non-negotiable laws

1. Similarity is not authority.
2. Relevant does not mean allowed.
3. Retrieval must be preserved.
4. Authorization must be graded.
5. Prompt context must be purpose-aware.
6. Every grade must produce a receipt.

## v0.1 scope

Build only:

- MemoryPacket schema
- PurposePolicy schema
- MemoryGrader
- SafeContextFrame
- PromptRenderer
- GradeReceipt / derivation trail
- Mock vector result demo
- Nexus + Veritas UI mock or CLI output

Do not build yet:

- full multi-agent runtime
- marketplace
- cloud auth
- payments
- plugin ecosystem
- complex vector integrations
- octonion/topological/geometric memory
- LLM-based authority decisions

## Technical stance

Trust decisions must be deterministic.

- An LLM may suggest metadata.
- An LLM must not decide authorization.
- Authorization comes from policy, provenance, labels, timestamps, permissions, and explicit overrides.
- Every override must produce a receipt.

## Naming guidance

The public product is **Border Agents**.

Use plain internal API names:

- `MemoryPacket`
- `PurposePolicy`
- `MemoryGrader`
- `SafeContextFrame`
- `PromptRenderer`
- `GradeReceipt`

Avoid mystical or metaphor-heavy class names in implementation.

## First demo requirement

The first demo must show the same mocked vector results graded differently for these purposes:

- `summarize_history`
- `answer_current_policy`
- `agent_action`
- `external_share`

The output must preserve all retrieved chunks and place them into grades:

- `trusted`
- `limited`
- `reference_only`
- `blocked`
- `quarantined`

## Testing rules

Every governance rule needs tests.

Minimum tests:

- expired chunks are not trusted for current policy answers
- chunks without `may_use_for_action` cannot influence `agent_action`
- blocked chunks are preserved in the frame ledger
- limited chunks render with constraints in annotated mode
- strict mode excludes limited/reference-only/blocked/quarantined content from prompt context
- custom purposes cannot widen permissions without an override receipt

## Collaboration workflow

Prefer small, reviewable commits.

Every PR should answer:

1. What border does this change make visible?
2. What trust decision does it make inspectable?
3. What receipt does it produce?
4. What tests prove it?

## Cursor Cloud specific instructions

### Stack and services

| Service | Port / URL | Start command |
|--------|------------|---------------|
| Vite dev server (React UI) | `http://127.0.0.1:1420` | `npm run dev` |
| Hermes dev gateway (WebSocket) | `ws://127.0.0.1:17387/border-buddies` | `HERMES_PROVIDER=echo npm run gateway:dev` |
| Full desktop overlay (Tauri) | loads UI from `:1420` | `bash scripts/bb-start.sh` or `npm run desktop:dev` |

For browser-only development, run Vite and the gateway in separate terminals (or tmux sessions). For the full desktop overlay, use `bash scripts/bb-start.sh`, which starts the gateway then `tauri dev`.

### Lint, build, and tests

- **Typecheck + production build:** `npm run build` (`tsc && vite build`)
- **Rust/Tauri compile check:** `cd src-tauri && cargo check` (requires Rust ≥ 1.96 and Linux GTK deps below)
- **Lint:** no ESLint script is wired in `package.json` yet
- **Automated tests:** vitest is a devDependency but no `npm test` script exists yet

### Linux desktop prerequisites (Tauri)

Tauri v2 on Linux needs WebKit2GTK 4.1 and GTK 3 dev packages (one-time VM/image setup, not in the update script):

```bash
sudo apt-get install -y libwebkit2gtk-4.1-dev libgtk-3-dev build-essential \
  libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev pkg-config
```

Ensure `rustc` ≥ 1.96 is on `PATH` (`rustup default stable`). The repo's Tauri crate graph requires Cargo edition 2024 support.

### Gateway echo mode (no API keys)

Copy `.env.example` to `.env` only when using a real LLM provider. For local dev without credentials, set `HERMES_PROVIDER=echo` so the gateway echoes chat messages back.

### Hello-world verification

1. `npm run dev` → open `http://127.0.0.1:1420` and confirm buddy characters render.
2. `HERMES_PROVIDER=echo npm run gateway:dev` → send a WebSocket `hello` + `chat` to `ws://127.0.0.1:17387/border-buddies`; expect a `chat_reply` echo.
3. In the UI, open Hermes chat and send a message; status should show gateway ready and replies should echo.

### Gotchas

- `BorderDock` imports Tauri APIs; in a plain browser preview, Tauri calls fail gracefully but some desktop-only features (hitboxes, always-on-top) are unavailable.
- Do not run `npm run dev` and `npm run desktop:dev` on the same port without stopping one first — both target port `1420`.
- `scripts/bb-stop.sh` / `npm run desktop:stop` stops desktop-related processes; gateway may need a manual kill if started separately.
