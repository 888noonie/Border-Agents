# UX Specification

Border Agents wins or loses on UX.

The governance engine is necessary, but the product experience is the friendly, miniature agent presence at the border of the screen.

## Product promise

> Friendly miniature agents that surface what your AI found, what it trusted, what it blocked, and what you can do next.

## Core interaction

1. Agent head peeks from a screen edge.
2. Agent subtly animates when idle.
3. Agent shows a speech bubble when relevant.
4. User taps or drags the agent into the workspace.
5. Agent expands into a focused panel.
6. User chooses one-click actions.
7. Agent returns to the border.

## Border behavior

Agents can live on:

- top edge
- right edge
- bottom edge
- left edge

Agent states:

- idle
- curious
- informative
- warning
- blocked
- celebrating
- waiting_for_approval

## Speech bubbles

Speech bubbles must be short, specific, and actionable.

Good:

```text
Nexus: “7 memories found. 4 trusted.”
Veritas: “One claim needs evidence.”
Nova: “Artifact hashed. Ready to save?”
Forge: “Protected file write. Approval needed.”
```

Bad:

```text
An issue has occurred in the retrieval authorization subsystem.
```

## Bubble actions

Bubbles may include quick actions:

- Use trusted only
- Show evidence
- Verify
- Approve
- Redact
- Hash
- Save
- Download
- Share
- Later
- Customize

## Character direction

Each Core Patrol agent should have a distinct feel.

| Agent | Feel | Motion ideas |
|---|---|---|
| Nexus | curious connector | looks side-to-side, pulls in threads |
| Veritas | precise truth-checker | narrows eyes, stamps receipts |
| Forge | practical builder | taps hammer, checks shield |
| Strategos | calm planner | unfolds tiny map |
| Nova | expressive polisher | sparkles, presents card |
| Aether | structural synthesizer | arranges floating blocks |
| Conductor | coordinator | raises baton, calls decision |

Characters should be charming, not childish. They should feel like useful companions for serious work.

## Trust states as visual language

Use consistent visual indicators:

- trusted: calm green
- limited: amber
- reference-only: neutral grey/blue
- blocked: red
- quarantined: purple
- celebrating: bright accent/confetti

Avoid alarm fatigue. Red should be rare and meaningful.

## First demo scene

The first demo should show:

1. A blank workspace with two or three agent heads peeking from the edges.
2. A user asks: “What is the current deployment policy?”
3. Nexus peeks in: “5 memories found. 1 trusted.”
4. User opens Nexus panel and sees the Safe Context Frame.
5. Veritas peeks in: “Only 1 source is assertable.”
6. User clicks “Use trusted only.”
7. Nova later peeks in: “Answer packaged. Hash and save?”

## v0.1 components

- `BorderDock`
- `AgentHead`
- `SpeechBubble`
- `AgentPanel`
- `TrustBadge`
- `ActionButton`
- `SafeContextFrameView`
- `ArtifactCard`

## UX laws

1. The agents are the hero.
2. Governance should feel like guidance, not bureaucracy.
3. Every warning must offer a next action.
4. Every agent appearance must be explainable.
5. Users must be able to dismiss, pin, mute, or customize agents.
6. The UI must never imply an action is approved unless the governance layer says it is.
