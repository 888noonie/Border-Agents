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

## Metaphor & naming

Border Agents is a metaphor-rich product *on purpose* — souls and bodies, buddies,
Core Patrol, the Wizard, Hermes. That vocabulary is a feature: it makes governance
legible and the experience delightful. Use it freely in vision, docs, UX, the
characters, and the presence layer.

Keep one place plain and literal: the **trust-critical core** — the governance
primitives (`MemoryPacket`, `PurposePolicy`, `MemoryGrader`, `SafeContextFrame`,
`PromptRenderer`, `GradeReceipt`, the posture resolver) and their tests. A misread
*there* is a security bug, not a vibe, so those identifiers and assertions stay
precise and concrete. Metaphor may *describe* what they do in comments and docs; it
just shouldn't be the name you reason about when deciding authorization.

Rule of thumb: **metaphor for what the system feels like; literal names for what the
system decides.** In the core, when in doubt, translate. Everywhere else, enjoy it.

The older "four-layer" framing (vision allowed → architecture translated → API
removed → test forbidden) still holds as a *gradient*, just read it as guidance, not
a ban: metaphor is most welcome at the edges and most disciplined at the deciding core.

## Non-negotiable laws

1. Similarity is not authority.
2. Relevant does not mean allowed.
3. Retrieval must be preserved.
4. Authorization must be graded.
5. Prompt context must be purpose-aware.
6. Every grade must produce a receipt.
7. Bodies present; souls act. Screen perception and screen action are governed
   effectors of the soul, routed through Core Patrol with receipts — never
   capabilities of the body. A body is a dumb puppet that *expresses* what the
   soul does (`thinking`, `attention`, pointing); it never reads or acts on the
   screen itself. This boundary is also what keeps bodies portable across
   platforms and the trust core deterministic. It erodes through convenience,
   not decision — hold it.

## Where the build is (June 2026)

Two halves, joined by the presence protocol; the first soul-gated effector now
runs end to end across both.

- **Governance core** (`src/core/`) — deterministic, test-backed: `MemoryPacket`,
  `PurposePolicy`, `MemoryGrader`, `SafeContextFrame`, `PromptRenderer`,
  `GradeReceipt`, and the Work/Play/Private posture (`userPosture.ts`, tighten-only
  over the purpose policies). 70 tests green.
- **Presence layer** (branch `presence-layer`) — soul⟷body split. Typed, versioned
  protocol (`src/presenceProtocol.ts`); a pure-Rust native body on `wlr-layer-shell`
  (`desktop-body/`, animated, drag-stable, user-verified); a browser body
  (`extensions/browser/presence.js`); a dev gateway "soul" (`scripts/gateway-dev.mjs`)
  plus the real `scripts/soul-server.ts` that runs the actual action gate.
  Born from the overlay rebuild — read `docs/OVERLAY_POSTMORTEM_AND_REBUILD_PLAN.md`
  before touching desktop overlay code.
- **Commandeer effector (landed)** — the first computer-use-style screen action,
  governed end to end. The COSMIC `frame_driver.rs` enumerates windows
  (`targets_available`), and on a soul `allow` can pin / monitor / type-into the
  chosen window. The body's right-click opens a two-phase picker (window → P/M/C)
  that emits an `ActionIntent`; Pin/Monitor/Control route through the soul-gated
  `commandeer` act effector (trusted-only, requires `may_use_for_action`), only
  local Unpin stays body-side. The body never reads or acts on the screen itself
  (law 7). Soul gate has governance tests; `desktop-body` cargo test is green.
- **Body settings (landed)** — a body-local settings panel (dial → Customize):
  Colour and Size are editable presentation; Posture and Buddy are read-only and
  only reflect soul/launch state, so the visible posture can never lie about the
  soul's actual authorization posture.

Next: the scripted **Wizard** onboarding host
(`docs/WIZARD_ONBOARDING_SCRIPT.md`) driving the body through Act 0+, then the
**governance vertical slice** that makes a buddy action grade memory and produce a
`GradeReceipt` — finally joining the two halves around the proven grading
primitive.

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
- Effectors (screen read, screen action, file access, external share, tool calls)
  belong to the soul and route through Core Patrol as purpose-typed, receipted
  calls — `agent_action` (strict, trusted-only, requires `may_use_for_action`) is
  the gate for computer-use-style screen action. The presence body has no effector
  capability; it only renders cues. See non-negotiable law 7.

## Naming guidance

The public product is **Border Agents**.

Use plain internal API names:

- `MemoryPacket`
- `PurposePolicy`
- `MemoryGrader`
- `SafeContextFrame`
- `PromptRenderer`
- `GradeReceipt`

Keep these core identifiers literal. Metaphor-rich names are welcome elsewhere —
presence/body code, characters, UI, demos (`soul`, `body`, `Wizard`, `Host`,
`Hermes`, `Core Patrol` all earn their keep). See *Metaphor & naming* above.

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
