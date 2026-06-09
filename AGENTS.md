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

### Services

| Service | Command | Port |
|---------|---------|------|
| Vite dev server | `npm run dev` | `http://127.0.0.1:1420` |
| Hermes gateway | `npm run gateway:dev` | `ws://127.0.0.1:17387/border-buddies` |
| Full desktop stack | `bash scripts/bb-start.sh` | starts gateway + `tauri dev` (Vite on 1420) |

Do not run `npm run dev` and `npm run desktop:dev` at the same time — Tauri's `beforeDevCommand` also starts Vite on port 1420.

### One-time Linux desktop prerequisites

Tauri on Linux needs system packages (already installed in the Cloud VM image):

```bash
sudo apt-get install -y libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev patchelf
```

Rust must be **stable ≥ 1.85** (dependency `serde_spanned` requires edition 2024). In this VM, Rust lives under `/usr/local/cargo` (not `~/.cargo`):

```bash
source /usr/local/cargo/env
rustup update stable
```

`scripts/bb-start.sh` sources `$HOME/.cargo/env`; use `/usr/local/cargo/env` here if desktop start fails with "cargo not found".

### Verify / lint

There is no ESLint config and no `npm test` script yet (vitest is listed but unwired). Use:

```bash
npm run build          # tsc + vite production build
source /usr/local/cargo/env && cd src-tauri && cargo check   # Rust/Tauri compile check
```

### Quick smoke test (no GUI)

With the gateway running, echo chat over WebSocket:

```bash
node --input-type=module -e "
import WebSocket from 'ws';
const ws = new WebSocket('ws://127.0.0.1:17387/border-buddies');
ws.on('open', () => ws.send(JSON.stringify({ type: 'chat', buddy: 'hermes', text: 'ping', requestId: '1' })));
ws.on('message', d => { console.log(d.toString()); ws.close(); });
"
```

### Browser vs desktop

- **Browser preview** (`npm run dev` + open `http://127.0.0.1:1420`): fastest for UI and Hermes chat; needs the gateway for chat.
- **Desktop overlay** (`npm run desktop:dev` or `bb-start.sh`): requires `DISPLAY`, GTK/WebKit deps, and Rust; exercises Linux hitbox shaping in `src-tauri/`.
