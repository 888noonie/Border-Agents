# Border Agents


**Make AI trust boundaries visible, clickable, and governable.**

Border Agents is an edge-native interface and governance layer for AI work.

It gives users a visible team of role-based agents that appear at the border of the workspace when AI output is about to cross a trust boundary: memory becoming context, context becoming a claim, a plan becoming an action, code becoming a repository change, or an artifact becoming something saved, shared, or published.

AI is no longer just answering questions. It retrieves memory, writes code, calls tools, drafts artifacts, edits files, and acts across systems.

But most AI interfaces still hide the important crossings inside prompts, logs, vector stores, and tool calls.

Border Agents makes those crossings visible.

---

## The principle

> **A system can possess knowledge without being authorized to act upon it.**

Border Agents is built around a simple idea:

AI work should not silently move from possibility into use.

A memory can be relevant without being allowed.
A claim can be plausible without being verified.
A tool call can be useful without being authorized.
A code change can compile while still violating policy.
An artifact can be polished without being ready to share.

Border Agents turns those hidden transitions into visible, inspectable workflows.

---

## What it does

Border Agents provides:

* A visible **Core Patrol** of role-based agents.
* A governance surface for memory, claims, actions, code, and artifacts.
* Purpose-aware context grading before prompt assembly.
* One-click actions for verification, approval, redaction, quarantine, export, and sharing.
* Inspectable receipts for every important trust decision.
* A future-safe model for creating custom agents by contract, not by unchecked autonomy.

The goal is not to replace your model, vector database, agent framework, or developer tools.

The goal is to make the trust boundaries between them visible and governable.

---

## Why Border Agents?

Every production AI workflow has borders:

```text
memory → prompt context
context → claim
claim → user belief
plan → tool call
tool call → external system
code diff → repository
draft → artifact
artifact → public share
```

Today, those borders are usually implicit.

Border Agents makes them explicit.

When something needs verification, review, approval, packaging, or escalation, the relevant agent appears at the edge of the screen.

The interface is playful.

The governance is serious.

---

## Core Patrol

Border Agents ships with a default team called the **Core Patrol**.

Each agent guards a specific class of AI trust boundary.

| Agent         | Border guarded         | Role                                                                             |
| ------------- | ---------------------- | -------------------------------------------------------------------------------- |
| **Nexus**     | Memory → Context       | Shows retrieved memory, grades, stale items, blocked items, and related sources. |
| **Veritas**   | Context → Claim        | Verifies assertions, flags unsupported claims, and attaches evidence receipts.   |
| **Forge**     | Intent → Action / Code | Reviews tool calls, file writes, code changes, and execution requests.           |
| **Strategos** | Idea → Plan            | Checks scope, sequencing, risks, and next steps.                                 |
| **Nova**      | Draft → Artifact       | Polishes, formats, packages, and prepares work for presentation or sharing.      |
| **Aether**    | Mess → Structure       | Turns raw ideas into architecture, specs, maps, and coherent systems.            |
| **Conductor** | Unresolved → Decision  | Handles approval, arbitration, handoff, and final release decisions.             |

The Core Patrol provides a useful default experience.

Advanced users can later create custom agents with explicit manifests, triggers, permissions, and receipt requirements.

> **Core Patrol by default. Custom agents by contract.**

---

## The Core Patrol is not decorative

Border Agents are not mascots.

They are role-based governance surfaces.

A Border Agent appears when its domain has something useful to contribute.

Examples:

```text
Nexus:
“I found 8 related memories. 3 are trusted, 2 are limited, 2 are reference-only, and 1 is quarantined.”

Veritas:
“This answer contains 4 factual claims. 1 lacks evidence.”

Forge:
“This tool action writes to a protected path and requires approval.”

Nova:
“This draft is ready to package as a shareable artifact.”

Conductor:
“This workflow has reached a decision point. Approve, revise, or export?”
```

The user can tap or drag an agent into the workspace to inspect the issue and take action.

---

## Founding laws

Border Agents follows six governance laws:

1. **Similarity is not authority.**
2. **Relevant does not mean allowed.**
3. **Retrieval must be preserved.**
4. **Authorization must be graded.**
5. **Prompt context must be purpose-aware.**
6. **Every grade must produce a receipt.**

These laws apply across memory, claims, actions, code, and artifacts.

They are the foundation for making AI work inspectable instead of invisible.

---

## Purpose-aware memory grading

The first production module focuses on retrieval authority.

Vector databases are good at answering:

> What is semantically close?

They are not designed to answer:

> Is this current?
> Is this verified?
> Is this private?
> Can it be quoted?
> Can it be used for an action?
> Can it be shared externally?
> Why was it allowed into the prompt?

Border Agents introduces a deterministic grading layer for retrieved memory.

Vector search retrieves freely.

The grader preserves every result and assigns each one a permitted use.

```text
Query
→ Vector search
→ Top-k chunks
→ Memory grader
→ Safe Context Frame
→ Prompt / Agent action
```

No retrieval is lost.

It is graded.

---

## Safe Context Frame

A **Safe Context Frame** is the structured ledger produced after retrieval grading.

It preserves the full retrieval result while separating what each chunk is allowed to become.

```json
{
  "trusted": [],
  "limited": [],
  "reference_only": [],
  "blocked": [],
  "quarantined": []
}
```

This avoids one of the most frustrating debugging failures in AI systems:

> Did retrieval fail, or did governance silently remove the context?

With Border Agents, the answer is inspectable.

The vector result is preserved.
The authorization grade is visible.
The reason is logged.
The next action is obvious.

---

## Purpose is not a label

In Border Agents, purpose is an authorization contract.

The same memory can receive different grades depending on what the AI is trying to do.

Examples:

```text
purpose = "summarize_history"
→ stale or limited material may be included with caveats

purpose = "answer_current_policy"
→ expired material is downgraded or blocked

purpose = "agent_action"
→ only action-authorized context may influence execution

purpose = "external_share"
→ private or sensitive material is blocked unless approved
```

A purpose is resolved through deterministic policy.

It is not an arbitrary string.
It is not a prompt hint.
It is not a vibe.

It is a contract that controls what retrieved memory may become.

---

## One-click actions

Border Agents turns governance into useful action, not passive warning.

Examples:

| Condition                 | Suggested actions                                                |
| ------------------------- | ---------------------------------------------------------------- |
| Expired source            | Verify source, open canonical document, use as historical only   |
| Low authority             | Find canonical source, request review, keep as limited context   |
| Sensitive content         | Redact, request disclosure approval, use internal-only summary   |
| Missing action permission | Request approval, find action-authorized source, view derivation |
| Contradicted memory       | Compare sources, resolve conflict, quarantine                    |
| Unsupported claim         | Add evidence, downgrade claim, remove claim                      |
| Ready artifact            | Hash, save, download, share, promote                             |

The interface is designed to make the next safe step obvious.

---

## Artifact lifecycle

Border Agents treats AI outputs as creations, not disposable chat messages.

A creation can move through a visible lifecycle:

```text
Idea
→ Draft
→ Verified
→ Graded
→ Packaged
→ Hashed
→ Saved
→ Shared
→ Promoted
```

Artifacts can include:

* content hash
* version history
* source receipts
* claim receipts
* memory grades
* author identity
* timestamps
* export metadata

Cryptographic hashes provide integrity, provenance, and authorship evidence.

They prove that a specific artifact existed in a specific form and has not been altered.

---

## Custom Border Agents

Border Agents will support custom agents after the Core Patrol is stable.

A custom agent must define:

* the border it guards
* the events that summon it
* the actions it may take
* the tools it may access
* the policy scope it operates within
* the receipts it must produce

Example:

```json
{
  "id": "legal_review",
  "name": "Legal Review",
  "border": "artifact_to_external_share",
  "edge": "right",
  "triggers": [
    "external_share_requested",
    "contract_language_detected"
  ],
  "allowed_actions": [
    "flag_risk",
    "request_review",
    "redact",
    "attach_receipt"
  ],
  "forbidden_actions": [
    "send_external",
    "approve_final"
  ],
  "requires_policy": true,
  "receipt_required": true
}
```

Custom agents may add inspection, guidance, formatting, or escalation.

They may not bypass governance.

---

## Architecture

```text
Border Agents
│
├── Core Patrol
│   ├── Nexus
│   ├── Veritas
│   ├── Forge
│   ├── Strategos
│   ├── Nova
│   ├── Aether
│   └── Conductor
│
├── Governance primitives
│   ├── MemoryPacket
│   ├── PurposePolicy
│   ├── MemoryGrader
│   ├── SafeContextFrame
│   ├── PromptRenderer
│   └── GradeReceipt
│
├── Artifact system
│   ├── ArtifactCard
│   ├── ArtifactHash
│   ├── VersionHistory
│   ├── ExportBundle
│   └── ShareReceipt
│
└── Interface layer
    ├── BorderDock
    ├── AgentHead
    ├── AgentPanel
    ├── TrustBadge
    ├── ActionButton
    └── ReceiptViewer
```

---

## Technical stance

Border Agents is deterministic where trust matters.

* No LLM decides its own authorization.
* Authority comes from provenance, policy, labels, timestamps, and explicit permission rules.
* Optional classifiers may suggest metadata, but deterministic policy owns enforcement.
* Every decision produces a receipt.
* Every override is logged.
* Every artifact can be hashed and exported.
* Every custom agent operates within a declared permission contract.

---

## Example flow

A user asks:

> What is the current deployment policy?

The system retrieves five relevant chunks from a vector store.

Nexus appears:

```text
5 memories retrieved.
1 trusted.
1 limited.
2 reference-only.
1 blocked.
```

Veritas appears:

```text
Only 1 source is authorized for current-policy assertion.
The limited source is expired.
The blocked source contains internal-only credentials.
```

The user can choose:

```text
Use trusted context only
Verify expired source
Open source
Export receipt
Ask policy owner
```

The answer is generated only from context authorized for that purpose.

The full retrieval ledger is preserved.

---

## Roadmap

### v0.1 — Core memory grading demo

* MemoryPacket schema
* PurposePolicy schema
* MemoryGrader
* SafeContextFrame
* PromptRenderer
* Nexus + Veritas prototype panels
* Same-vector-different-purpose demo

### v0.2 — Artifact lifecycle

* Artifact cards
* Hashing and export bundles
* Save / download / share flow
* Receipt viewer
* Nova packaging panel

### v0.3 — Action and code borders

* Forge panel
* Tool action review
* File write review
* Code diff inspection
* Approval receipts

### v0.4 — Core Patrol workspace

* Full Core Patrol UI
* BorderDock
* Agent customization
* Agent visibility controls
* Local project workspace

### v1.0 — Custom Border Agents

* Custom agent manifests
* Signed agent packages
* Import/export
* Permission-scoped tools
* Team policy templates

---

## Who this is for

### Developers

Use Border Agents to understand what your AI retrieved, what it was allowed to use, what was blocked, and why.

### AI builders

Add purpose-aware governance to RAG, agent workflows, artifact generation, and tool use without replacing your existing stack.

### Teams

Make AI work inspectable, auditable, and easier to approve.

### Creators

Turn AI conversations into verified, packaged, cryptohashed artifacts you can save, download, and share.

### Security and compliance leaders

See what crossed a trust boundary, what was blocked, who approved an override, and what evidence supported each decision.

---

## What Border Agents is not

Border Agents is not:

* a model provider
* a vector database
* a replacement for your agent framework
* a generic chatbot skin
* a mascot layer
* an output-only guardrail
* a marketplace of unchecked agents

Border Agents is a visible governance surface for AI work.

It helps decide what may cross from retrieval into context, from context into claim, from plan into action, and from creation into artifact.

---

## Status

Early design and prototype planning.

The first build target is the memory grading and Safe Context Frame workflow, surfaced through Nexus and Veritas.

The first demo will show:

```text
same vector results
+ different purposes
= different authorized context frames
```

That is the core product proof.

---

## Core line

> **Border Agents make AI trust boundaries visible, clickable, and governable.**

## Shorter line

> **AI work crosses borders. Border Agents decide what may pass.**

## Deepest line

> **A system can possess knowledge without being authorized to act upon it.**
