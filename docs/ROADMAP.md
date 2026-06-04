# Roadmap

## v0.1 — Memory grading proof

Goal: prove the core governance primitive.

Deliverables:

- MemoryPacket schema
- PurposePolicy schema
- MemoryGrader
- SafeContextFrame
- PromptRenderer
- GradeReceipt / derivation trail
- Mock retrieved results
- Same-vector-different-purpose demo
- Basic tests for grading and rendering

Success criteria:

```text
same retrieved chunks
+ different purposes
= different SafeContextFrames
```

---

## v0.2 — Trust Workbench

Goal: make the ledger visible and actionable.

Deliverables:

- Nexus panel: retrieval grades and source list
- Veritas panel: claim/evidence warnings
- Action buttons: verify, open source, export receipt, use trusted only
- Receipt viewer

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
