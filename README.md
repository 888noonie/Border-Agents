# Border Agents

**Tiny AI companions that live on the edge of your screen and make AI work feel visible, safe, and delightful.**

Border Agents is a personality-forward interface for AI work. A small patrol of customizable agents peeks from the borders of your workspace. They stay out of the way until something useful happens, then slide in with speech bubbles, quick actions, celebrations, warnings, receipts, and next steps.

The promise is simple:

> **Your AI team lives at the edge of the screen and steps in only when useful.**

The interface is playful. The trust layer underneath is serious.

---

## The experience

Your screen border becomes a living dock.

Agent heads peek in from the top, bottom, left, and right edges. They subtly bob, blink, glance, wave, or glow when idle. When their role becomes relevant, they show a small speech bubble:

```text
Nexus: “I found 7 related memories. Want trusted-only?”
Veritas: “One claim needs evidence. Peek?”
Forge: “This action writes to a protected file. Approval needed.”
Nova: “Artifact packaged and hashed. Ready to share?”
```

Tap or drag an agent into the center to expand it into a focused panel. Dismiss it, pin it, move it to another edge, or customize how it looks and speaks.

Border Agents should feel like a friendly desktop creature system for serious AI work: charming enough to keep open every day, useful enough to earn its place on screen.

---

## Why this matters

AI work is becoming active.

It retrieves memory, writes code, calls tools, drafts artifacts, edits files, and shares outputs across systems. But most AI interfaces still hide the important moments inside prompts, logs, vector stores, and tool calls.

Border Agents turns those hidden moments into visible, friendly interactions.

```text
Something is retrieved → Nexus peeks in
Something is unverified → Veritas peeks in
Something wants to run → Forge peeks in
Something is ready to share → Nova peeks in
Something needs a decision → Conductor peeks in
```

The user does not need to read a governance log first. The right little agent appears and offers the next safe action.

---

## Core Patrol

Border Agents ships with a default team called the **Core Patrol**.

| Agent | Personality direction | Border guarded | What they surface |
|---|---|---|---|
| **Nexus** | curious connector | Memory → Context | Retrieved memories, grades, stale sources, related files |
| **Veritas** | precise truth-checker | Context → Claim | Unsupported claims, evidence, confidence downgrades |
| **Forge** | practical builder | Intent → Action / Code | Tool calls, file writes, code diffs, protected actions |
| **Strategos** | calm planner | Idea → Plan | Scope, sequencing, risk, next step |
| **Nova** | expressive polisher | Draft → Artifact | Formatting, presentation, packaging, share readiness |
| **Aether** | structural synthesizer | Mess → Structure | Architecture, specs, diagrams, coherent systems |
| **Conductor** | final coordinator | Unresolved → Decision | Approval, arbitration, handoff, release decisions |

> **Core Patrol by default. Custom agents by contract.**

Users eventually create their own Border Agents with custom appearance, voice, triggers, allowed actions, and receipt behavior.

---

## The agents are the product

Border Agents are not decorative mascots pasted onto a serious tool.

They are the primary interface.

The governance engine exists to make the agents trustworthy. The agents exist to make governance feel natural, fast, and pleasant.

Each agent should be:

- miniature and expressive
- customizable by the user
- visible only when useful
- easy to drag, pin, dismiss, or expand
- able to explain why it appeared
- backed by real deterministic checks
- capable of producing receipts when a trust decision matters

The UX goal is not “compliance dashboard.”

The UX goal is:

> **Friendly companions that turn invisible AI work into obvious, safe, one-click moments.**

---

## First product loop

The first successful demo should not be a backend library.

It should be a living border experience:

1. User asks a question or creates an artifact.
2. Nexus peeks in with retrieved context.
3. Veritas peeks in if a claim needs evidence.
4. Nova peeks in when the artifact is ready to package.
5. User clicks obvious actions: **Use trusted only**, **Show evidence**, **Verify**, **Hash**, **Save**, **Download**, **Share**.

The user should feel progression:

```text
Idea → Draft → Verified → Packaged → Hashed → Saved → Shared
```

This visible progression is the retention loop.

---

## What powers the agents

Under the playful surface is a deterministic trust layer.

Border Agents follows six governance laws:

1. **Similarity is not authority.**
2. **Relevant does not mean allowed.**
3. **Retrieval must be preserved.**
4. **Authorization must be graded.**
5. **Prompt context must be purpose-aware.**
6. **Every grade must produce a receipt.**

These laws let the agents speak with substance instead of vibes.

---

## Memory grading: the first engine

The first governance engine grades retrieved memory before it becomes prompt context, claim, tool input, or action basis.

Vector databases answer:

> What is semantically close?

Border Agents also asks:

> What is this memory allowed to become?

```text
Query
→ Vector search
→ Top-k chunks
→ Memory grader
→ Safe Context Frame
→ Agent bubble / prompt / action
```

No retrieval is lost.

It is graded.

A **Safe Context Frame** preserves every retrieved result:

```json
{
  "trusted": [],
  "limited": [],
  "reference_only": [],
  "blocked": [],
  "quarantined": []
}
```

This is what lets Nexus say:

```text
“I found 8 memories. 3 trusted, 2 limited, 2 reference-only, 1 quarantined.”
```

And lets Veritas say:

```text
“Only 3 are assertable for this answer.”
```

---

## Purpose is not a label

In Border Agents, purpose is an authorization contract.

The same memory can be graded differently depending on use:

```text
summarize_history       → older context may be shown with caveats
answer_current_policy   → expired policy is downgraded or blocked
agent_action            → only action-authorized memory may influence execution
external_share          → private content is blocked unless approved
```

The user sees this as simple behavior from the agents. The system stores it as receipts.

---

## One-click actions

Border Agents should make the next safe action obvious.

| Situation | Agent action buttons |
|---|---|
| Source is expired | Verify source, Open canonical doc, Use as historical only |
| Claim lacks evidence | Show evidence, Downgrade claim, Remove claim |
| Sensitive content found | Redact, Request approval, Keep internal-only |
| Action lacks permission | Request approval, View reason, Find authorized source |
| Artifact is ready | Package, Hash, Save, Download, Share |
| Memory conflicts | Compare sources, Prefer canonical, Quarantine |

The product should feel less like a warning system and more like a set of helpful companions nudging work forward safely.

---

## Artifact lifecycle

Border Agents treats AI outputs as creations, not disposable chat messages.

A creation can become an artifact with:

- content hash
- version history
- source receipts
- claim receipts
- memory grades
- author identity
- timestamps
- export metadata

Cryptographic hashes provide integrity, provenance, and authorship evidence. They prove that a specific artifact existed in a specific form and has not been altered.

---

## Technical stance

The UX can be charming because the trust layer is strict.

- No LLM decides its own authorization.
- Authority comes from provenance, policy, labels, timestamps, and explicit permission rules.
- Optional classifiers may suggest metadata, but deterministic policy owns enforcement.
- Every decision produces a receipt.
- Every override is logged.
- Every artifact can be hashed and exported.
- Every custom agent operates within a declared permission contract.

---

## v0.1 build target

The first version should prove the UX and the trust primitive together.

### Build first

- A border dock with 2-3 animated miniature agents.
- Nexus and Veritas bubbles.
- Mock retrieved memories.
- Memory grading into a Safe Context Frame.
- Simple artifact card progression.
- One-click actions: Use trusted only, Show evidence, Verify, Hash, Save.

### Do not build yet

- full marketplace
- real vector integrations
- multi-user enterprise controls
- plugin ecosystem
- payments
- complex custom-agent runtime

The first demo should make someone say:

> “I understand what my AI found, what it trusted, what it blocked, and what I can do next — and I actually like using it.”

---

## Repository map

- [`AGENTS.md`](AGENTS.md) — instructions for AI coding assistants
- [`docs/UX.md`](docs/UX.md) — UX-first product specification
- [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) — governance architecture
- [`docs/ROADMAP.md`](docs/ROADMAP.md) — staged build plan
- [`docs/CORE_PATROL.md`](docs/CORE_PATROL.md) — default agents
- [`docs/SPEC_MEMORY_GRADING.md`](docs/SPEC_MEMORY_GRADING.md) — first governance primitive

---

## Core line

> **Border Agents make AI work visible at the edge of your screen.**

## Product line

> **Friendly miniature agents that surface what your AI found, what it trusted, what it blocked, and what you can do next.**

## Governance line

> **A system can possess knowledge without being authorized to act upon it.**
