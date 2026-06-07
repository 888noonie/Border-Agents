# Roadmap

## v0.1 — Memory grading proof ✅

Goal: prove the core governance primitive.

Delivered:

- MemoryPacket schema
- PurposePolicy schema
- JSON Schemas for MemoryPacket and PurposePolicy
- MemoryGrader
- SafeContextFrame
- PromptRenderer
- GradeReceipt / derivation trail
- Receipt summary view model
- Mock retrieved results
- Same-vector-different-purpose demo
- Governance trace reporter: `npm run demo:trace`
- Basic tests for grading, rendering, schemas, receipts, traces, and demo behavior

Success criteria:

```text
same retrieved chunks
+ different purposes
= different SafeContextFrames
```

Status:

- Complete on `governance_core`.
- Same retrieved Hermes memory packets now produce different frames for:
  - `summarize_history`
  - `answer_current_policy`
  - `agent_action`
  - `external_share`
- Every retrieved chunk is preserved in a frame bucket.
- Every grade emits a receipt trail.
- Prompt rendering records what entered or stayed out of context.

---

## v0.2 — Trust Workbench ⏳

Goal: make the ledger visible and actionable.

Delivered / scaffolded:

- NexusPanelData view model: grade buckets, prompt summary, trust badge state, source list
- VeritasPanelData view model: receipt groups, warnings, evidence-ready items
- Browser-only Trust Workbench preview with minimize/reopen toggle

Next deliverables:

- Nexus buddy panel: retrieval grades and source list
- Veritas buddy panel: receipt warnings and evidence-ready items
- Action buttons: verify, open source, export receipt, use trusted only
- Receipt viewer
- Tests for panel interactions and receipt expansion

---

## v0.3 — Artifact lifecycle

Goal: turn AI outputs into governed creations.

Deliverables:

- Artifact cards
- Hashing and export bundles
- Version history
- Save / download / share flow
- Nova packaging panel

---

## v0.4 — Action and code borders

Goal: extend governance from memory/claims to actions/code.

Deliverables:

- Forge panel
- Tool action review
- File write review
- Code diff inspection
- Approval receipts

---

## v0.5 — Core Patrol workspace

Goal: integrate the full default patrol.

Deliverables:

- BorderDock
- AgentHead
- AgentPanel
- TrustBadge
- User-configurable visible agents
- Core Patrol settings

---

## v1.0 — Custom Border Agents

Goal: allow users to create scoped agents by contract.

Deliverables:

- Custom agent manifests
- Trigger definitions
- Allowed/forbidden actions
- Permission-scoped tools
- Signed agent packages
- Import/export
- Team policy templates

Rule:

> Custom agents may add inspection, guidance, formatting, or escalation. They may not bypass governance.
