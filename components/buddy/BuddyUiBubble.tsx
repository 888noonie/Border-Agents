import type { UiBubblePhase } from "./useUiBubble";
import type { ReactNode } from "react";
import "./buddy-surface.css";

type BuddyUiBubbleProps = {
  text: string;
  phase: UiBubblePhase;
  mounted: boolean;
  clickable: boolean;
  bubbleSide: "left" | "right";
  bubbleVertical: "above" | "below";
  inline?: boolean;
  tabs?: BuddyUiBubbleTab[];
  activeTab?: string;
  activationLabel?: string;
  title?: string;
  onActivate?: () => void;
  onTabChange?: (tabId: string) => void;
};

export type BuddyUiBubbleTab = {
  id: string;
  label: string;
  icon: string;
  tone: "message" | "settings" | "gateway" | "dock";
  content: ReactNode;
};

export function BuddyUiBubble({
  text,
  phase,
  mounted,
  clickable,
  bubbleSide,
  bubbleVertical,
  inline = false,
  tabs = [],
  activeTab = "message",
  activationLabel,
  title,
  onActivate,
  onTabChange,
}: BuddyUiBubbleProps) {
  if (!mounted) {
    return null;
  }

  const activeTabContent = tabs.find((tab) => tab.id === activeTab)?.content;
  const hasTabs = tabs.length > 0;

  return (
    <div
      className={[
        "buddy-ui-bubble",
        inline ? "buddy-ui-bubble--inline" : "",
        hasTabs ? "buddy-ui-bubble--tabbed" : "",
        bubbleSide === "right" ? "buddy-ui-bubble--right" : "buddy-ui-bubble--left",
        bubbleVertical === "above" ? "buddy-ui-bubble--above" : "buddy-ui-bubble--below",
        phase === "entering" ? "buddy-ui-bubble--entering" : "",
        phase === "visible" ? "buddy-ui-bubble--visible" : "",
        phase === "leaving" ? "buddy-ui-bubble--leaving" : "",
        clickable ? "buddy-ui-bubble--clickable" : "",
      ].join(" ")}
      role={clickable ? "button" : "status"}
      aria-label={clickable && activationLabel ? `${activationLabel}: ${text}` : undefined}
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
      {hasTabs ? (
        <div className="buddy-ui-bubble__tabs" aria-label="Bubble controls">
          {tabs.map((tab) => (
            <button
              className={[
                "buddy-ui-bubble__tab",
                `buddy-ui-bubble__tab--${tab.tone}`,
                tab.id === activeTab ? "buddy-ui-bubble__tab--active" : "",
              ].join(" ")}
              key={tab.id}
              type="button"
              aria-pressed={tab.id === activeTab}
              title={tab.label}
              onClick={(event) => {
                event.preventDefault();
                event.stopPropagation();
                onTabChange?.(tab.id);
              }}
            >
              <span aria-hidden="true">{tab.icon}</span>
              <span className="buddy-ui-bubble__tab-label">{tab.label}</span>
            </button>
          ))}
        </div>
      ) : null}
      <div
        className="buddy-ui-bubble__text"
        onClick={(event) => {
          if (activeTab !== "message") {
            event.stopPropagation();
          }
        }}
      >
        {activeTabContent ?? text}
      </div>
      {clickable && activationLabel ? (
        <small className="buddy-ui-bubble__activation-label">{activationLabel}</small>
      ) : null}
    </div>
  );
}
