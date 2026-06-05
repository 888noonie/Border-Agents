# Global Idle Animation System

Border Buddies should feel like small physical companions, not static icons.
Motion is soft, low-noise, and personality-rich.

## Baseline Motions

### Gentle Floating Bob

- Applies to every buddy while idle.
- Default amplitude: 3-5px.
- Default duration: 3.2-4.2s.
- Use slightly different durations per buddy so the dock never feels mechanical.
- Tucked buddies bob less than free buddies.

### Blink

- Applies to buddies with visible eyes.
- Default interval: 4-7s.
- Duration: 110-160ms.
- Blink should be subtle and never interrupt dragging.

### Head Glance

- Applies occasionally when idle.
- Default interval: 7-11s.
- Eyes or head shift 1-3px, then settle.
- Use this to create the feeling that buddies are aware but not needy.

### Breathing / Glow Pulse

- Applies to glowing, cosmic, magical, robotic, or powered buddies.
- Default duration: 2.8-4.8s.
- Glow opacity should stay soft.
- Hermes uses this as a signature motion with cyan accents.

### Character Flair

- Optional accent motion, one per buddy.
- Examples:
  - Hermes: tiny cyan star glints and baton flick.
  - Claw: one claw wave.
  - Veritas: slow precise blink.
  - Nexus: tail curl or ear twitch.

## CSS Guidance

Use composited properties only:

```css
transform
opacity
filter
```

Avoid layout properties inside keyframes. Do not animate `top`, `left`,
`width`, or `height` for idle motion.

Recommended class structure:

```css
.bb-buddy {
  animation: bb-idle-bob var(--bb-bob-duration, 3600ms) ease-in-out infinite;
}

.bb-buddy[data-state="tucked"] {
  --bb-bob-distance: 2px;
}

.bb-buddy[data-state="free"] {
  --bb-bob-distance: 5px;
}

.bb-buddy[data-buddy="hermes"] {
  animation:
    bb-idle-bob 3600ms ease-in-out infinite,
    bb-glow-pulse 4200ms ease-in-out infinite;
}
```

## React Guidance

- Keep placement state separate from animation state.
- Disable idle animation during active drag.
- Use pointer capture for dragging.
- Use the same snap threshold across desktop and browser where possible.
- Persist placement under the shared key:

```text
border-buddies:placements:v2
```

## Browser Extension Guidance

- Content scripts should inject a single root container.
- The page background must remain interactive.
- Only buddy heads, full bodies, and speech bubbles should receive pointer
  events.
- Mirror desktop behavior: tucked by default, drag to free, release near edge
  to tuck, double-click to tuck manually.

## Accessibility

- Respect `prefers-reduced-motion`.
- If reduced motion is enabled, keep a static tucked pose and allow drag/snap.
- Speech bubbles should be short and dismissible by clicking the buddy.
