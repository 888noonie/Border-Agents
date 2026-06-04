# Core Patrol

The Core Patrol is the default set of Border Agents.

Each agent guards one class of AI trust boundary and appears when that boundary needs attention.

## Agents

| Agent | Border guarded | Primary responsibility |
|---|---|---|
| Nexus | Memory → Context | Retrieval ledger, memory grades, related sources |
| Veritas | Context → Claim | Claim verification, evidence status, unsupported assertions |
| Forge | Intent → Action / Code | Tool calls, file writes, code diffs, execution requests |
| Strategos | Idea → Plan | Scope, sequencing, risk, next step |
| Nova | Draft → Artifact | Polish, package, export, share readiness |
| Aether | Mess → Structure | Architecture, specs, maps, coherent systems |
| Conductor | Unresolved → Decision | Approval, arbitration, handoff, release decision |

## v0.1 active agents

Only two agents should be active in the first working demo:

### Nexus

Shows:

- retrieved chunks
- grade distribution
- source metadata
- blocked/limited/reference-only reasons
- suggested actions

### Veritas

Shows:

- which claims are supported
- which claims need verification
- which sources are assertable
- whether current answer generation should use trusted-only context

## Agent posture

Border Agents should be:

- present, not intrusive
- protective, not controlling
- visible, not noisy
- actionable, not decorative
- receipt-producing, not vibe-based

## Custom agents

Custom agents come later and must be defined by manifest.

A custom agent must declare:

- border guarded
- trigger events
- allowed actions
- forbidden actions
- policy scope
- tool permissions
- receipt behavior

Custom agents cannot bypass governance.
