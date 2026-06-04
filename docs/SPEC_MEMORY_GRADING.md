# Specification: Memory Grading

Memory grading is the first production wedge for Border Agents.

It separates retrieval from authorization.

```text
Vector search retrieves freely.
Memory grading decides what each result is allowed to become.
```

## Invariant

> No memory retrieval is lost. It is graded.

## Grades

| Grade | Meaning | Prompt behavior |
|---|---|---|
| `trusted` | Authorized for the active purpose | May enter trusted context |
| `limited` | Relevant but constrained | May enter constrained context in annotated mode |
| `reference_only` | Useful for UI/reference, not prompt assertion | UI/sidebar only by default |
| `blocked` | Relevant but not permitted for current purpose | Receipt only |
| `quarantined` | Conflict, sensitivity, or policy issue requiring review | Security/audit/review only |

## MemoryPacket

Minimum v0.1 shape:

```json
{
  "packet_id": "mem_pkt_01",
  "content_hash": "sha256:...",
  "source": {
    "type": "repo_file",
    "id": "docs/deploy_policy.md",
    "created_at": "2026-06-02T12:00:00Z"
  },
  "claim_type": "observed_fact",
  "authority": "high",
  "sensitivity": "internal",
  "valid_until": null,
  "permissions": {
    "may_retrieve": true,
    "may_quote": true,
    "may_assert": true,
    "may_use_for_action": false,
    "requires_verification_before_assertion": false
  },
  "labels": ["canonical"],
  "policy": {
    "id": "border-agents-default",
    "version": "0.1.0"
  },
  "derivation": [],
  "review": {
    "mode": "strict",
    "requires_review": false,
    "reviewed_by": null,
    "reviewed_at": null
  }
}
```

## PurposePolicy

A purpose is an authorization contract.

Example:

```json
{
  "id": "agent_action",
  "risk": "high",
  "allow_grades_in_prompt": ["trusted"],
  "require_permissions": ["may_retrieve", "may_use_for_action"],
  "assertion_requires": [],
  "action_requires": ["may_use_for_action"],
  "allow_sensitive": ["public", "internal"],
  "render_mode": "strict"
}
```

## Derivation receipt

Every grade should include a machine-readable derivation trail.

```json
{
  "field": "may_use_for_action",
  "value": false,
  "source": "default",
  "reason": "action use requires explicit action-authorizing label",
  "policy_rule": "defaults.may_use_for_action"
}
```

## SafeContextFrame

```json
{
  "purpose": "answer_current_policy",
  "trusted": [],
  "limited": [],
  "reference_only": [],
  "blocked": [],
  "quarantined": [],
  "receipts": []
}
```

## Required demo

Using the same retrieved chunks, show different results for:

- `summarize_history`
- `answer_current_policy`
- `agent_action`
- `external_share`

## Required tests

- expired chunks are not trusted for current policy answers
- chunks without `may_use_for_action` cannot influence `agent_action`
- blocked chunks are preserved in the frame ledger
- limited chunks render with constraints in annotated mode
- strict mode excludes limited/reference-only/blocked/quarantined content
- custom purposes cannot widen permissions without override receipt
