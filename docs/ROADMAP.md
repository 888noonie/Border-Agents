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
- Shared TrustWorkbenchPanel used by browser preview and buddy panels
- Nexus buddy panel preview: retrieval grades, blocked counter, prompt summary, source list
- Veritas buddy panel preview: warning counter, warnings, evidence-ready items, receipt row expansion
- Collapsible workbench sections with colored counters for compact buddy panels
- Trust Workbench action state: verify, selected source, receipt export preview, trusted-only context summary
- Receipt detail viewer with receipt id, packet/chunk ids, grade, prompt status, final reason, and derivation steps
- Work / Play / Private posture as a deterministic core primitive (`src/core/userPosture.ts`): tighten-only over the purpose policies, with a separate non-authorization interaction layer and a hard confirmation floor on medium/high-risk actions
- Refined speech bubble behavior: docked buddies show minimal status output; undocked buddies show tabbed controls
- Persisted undocked speech bubble state: active tab, collapsed sections, center/full-height fit, and settings-overflow guard
- Tests for workbench section collapse and receipt expansion
- Tests for receipt detail actions and richer Veritas receipt data
- Tests for user mode normalization and per-mode remembered settings
- Tests for tabbed speech bubble and BuddySurface docked/undocked behavior

Next deliverables:

- Real source opening handlers
- Durable receipt export/download path
- Dedicated receipt viewer layout for longer derivation trails
- Wire file-system review surfaces to the active user mode posture

---

## Presence layer — one soul, many bodies ⏳

Goal: a portable presence surface for buddies, with the body/soul trust boundary
(AGENTS.md law 7) baked in. Born from the overlay rebuild — see
`docs/OVERLAY_POSTMORTEM_AND_REBUILD_PLAN.md`.

Build order:

1. **Presence protocol v0** ✅ — typed, versioned soul⟷body schema
   (`src/presenceProtocol.ts`); JS mirror for the browser body; dev gateway soul.
2. **Layer-shell spike** ✅ — pure-Rust per-buddy surface on `wlr-layer-shell`
   (`desktop-body/`), pixel-exact placement, click-through, no GTK/WebKit/GPU.
3. **Animated body** ✅ (user-verified) — face, blink, emotion, speech bubble, menu;
   drag-stable via relative pointer; on-screen clamp; correct buffer-size rendering.
4. **Wire the soul** ⏳ — presence WebSocket client in the Rust body; first driver is
   the scripted **Wizard** onboarding host (`docs/WIZARD_ONBOARDING_SCRIPT.md`).
   Current `presence-layer` proof (2026-06-13): the body consumes target lifecycle
   cues from a COSMIC window-tracking helper and can pin/unpin Hermes to a tracked
   native window as a small head + speech/input surface. The user can drag the pinned
   head to choose any attachment point on/near the target; that offset follows the
   window as it moves. This is still presentation-only: the body does not read,
   move, or act on the target window.
5. **Governance vertical slice** — a buddy action produces a receipt, joining the
   presence layer to the governance core.

Known TODOs before main: polish pinned placement controls, remove or quarantine the
old full-frame renderer/tests if the pin UX remains the chosen path, and wire any
future target action through Core Patrol receipts rather than the body.

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
