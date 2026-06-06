import type { UiBubblePhase } from "./useUiBubble";
import "./buddy-surface.css";

type BuddyUiBubbleProps = {
  text: string;
  phase: UiBubblePhase;
  mounted: boolean;
  clickable: boolean;
  bubbleSide: "left" | "right";
  bubbleVertical: "above" | "below";
  title?: string;
  onActivate?: () => void;
};

export function BuddyUiBubble({
  text,
  phase,
  mounted,
  clickable,
  bubbleSide,
  bubbleVertical,
  title,
  onActivate,
}: BuddyUiBubbleProps) {
  if (!mounted) {
    return null;
  }

  return (
    <div
      className={[
        "buddy-ui-bubble",
        bubbleSide === "right" ? "buddy-ui-bubble--right" : "buddy-ui-bubble--left",
        bubbleVertical === "above" ? "buddy-ui-bubble--above" : "buddy-ui-bubble--below",
        phase === "entering" ? "buddy-ui-bubble--entering" : "",
        phase === "visible" ? "buddy-ui-bubble--visible" : "",
        phase === "leaving" ? "buddy-ui-bubble--leaving" : "",
        clickable ? "buddy-ui-bubble--clickable" : "",
      ].join(" ")}
      role={clickable ? "button" : "status"}
      aria-live="polite"
      aria-atomic="true"
      tabIndex={clickable ? 0 : -1}
      title={title}
      onClick={(event) => {
        if (!clickable) {
          return;
        }
        event.preventDefault();
        event.stopPropagation();
        onActivate?.();
      }}
      onKeyDown={(event) => {
        if (!clickable) {
          return;
        }
        if (event.key === "Enter" || event.key === " ") {
          event.preventDefault();
          event.stopPropagation();
          onActivate?.();
        }
      }}
    >
      <div className="buddy-ui-bubble__text">{text}</div>
    </div>
  );
}
