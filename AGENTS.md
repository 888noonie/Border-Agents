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
7. Bodies present; souls act. Screen perception and screen action are governed
   effectors of the soul, routed through Core Patrol with receipts — never
   capabilities of the body. A body is a dumb puppet that *expresses* what the
   soul does (`thinking`, `attention`, pointing); it never reads or acts on the
   screen itself. This boundary is also what keeps bodies portable across
   platforms and the trust core deterministic. It erodes through convenience,
   not decision — hold it.

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
