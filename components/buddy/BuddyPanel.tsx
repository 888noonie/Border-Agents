import { useRef, type FormEvent, type ReactNode } from "react";
import type { GatewayConnectionState } from "../../src/gatewayProtocol";
import { connectionLabelForState } from "../../src/useBuddyGateway";
import type { BuddyDisplayMode } from "./BuddySurface";
import "./buddy-surface.css";

export type BuddyChatLine = {
  id: string;
  role: "user" | "assistant" | "status";
  text: string;
};

type BuddyPanelProps = {
  buddyName: string;
  modelLabel: string;
  displayMode: BuddyDisplayMode;
  draft: string;
  busy: boolean;
  hasGateway: boolean;
  gatewayState: GatewayConnectionState;
  history: BuddyChatLine[];
  panelContent?: ReactNode;
  onDraftChange: (value: string) => void;
  onSubmit: () => void;
  onPrimaryAction: () => void;
  onOpenMenu: (anchor: HTMLElement) => void;
  onOpenSettings: () => void;
  onCollapse: () => void;
  onComposerInteract?: () => void;
};


export function BuddyPanel({
  buddyName,
  modelLabel,
  displayMode,
  draft,
  busy,
  hasGateway,
  gatewayState,
  history,
  panelContent,
  onDraftChange,
  onSubmit,
  onPrimaryAction,
  onOpenMenu,
  onOpenSettings,
  onCollapse,
  onComposerInteract,
}: BuddyPanelProps) {

  const menuButtonRef = useRef<HTMLButtonElement>(null);
  const gatewayOnline = !hasGateway || gatewayState === "connected";
  const isFull = displayMode === "full";
  // Send while offline doubles as "connect": handleSubmit/onSubmit will trigger
  // the gateway connection when we're not online yet. So the button is enabled
  // whenever there is text (or we're offline and could connect on click).
  const primaryLabel = busy
    ? "Stop"
    : hasGateway && !gatewayOnline
      ? "Connect"
      : "Send";
  const primaryDisabled = busy ? false : !draft.trim() && gatewayOnline;


  function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (busy) {
      onPrimaryAction();
      return;
    }
    onSubmit();
  }

  return (
    <div className={["buddy-panel", isFull ? "buddy-panel--full" : "buddy-panel--compact"].join(" ")}>
      {isFull ? (
        <div className="buddy-panel__history" aria-label={`${buddyName} chat history`}>
          {history.length === 0 ? (
            <p className="buddy-panel__history-empty">
              {hasGateway
                ? gatewayOnline
                  ? "Gateway link is live. Ask me anything."
                  : "Open settings and connect to the gateway to start chatting."
                : `Ask ${buddyName} anything.`}
            </p>
          ) : (
            history.map((line) => (
              <div
                key={line.id}
                className={[
                  "buddy-panel__line",
                  `buddy-panel__line--${line.role}`,
                ].join(" ")}
              >
                {line.text}
              </div>
            ))
          )}
        </div>
      ) : null}

      {isFull && panelContent ? (
        <div className="buddy-panel__content" aria-label={`${buddyName} panel content`}>
          {panelContent}
        </div>
      ) : null}

      <form className="buddy-panel__composer" onSubmit={handleSubmit}>
        <textarea
          rows={isFull ? 2 : 1}
          value={draft}
          onChange={(event) => onDraftChange(event.target.value)}
          placeholder={
            hasGateway && !gatewayOnline
              ? `Connect ${buddyName} in settings`
              : `Ask ${buddyName}`
          }
          aria-label={`Ask ${buddyName}`}
          // Always allow clicking/typing — even before the gateway is online.
          // Sending while offline triggers the connection (see handleSubmit /
          // BuddySurface.handleSubmit). Previously this textarea was disabled
          // whenever Hermes' gateway was offline, which made Hermes feel
          // completely unclickable compared to the gateway-less buddies.
          disabled={busy}
          onFocus={onComposerInteract}
          onPointerDown={onComposerInteract}
        />


        <div className="buddy-panel__actions">
          <button
            type="button"
            className="buddy-panel__icon-button"
            aria-label={busy ? "Working" : primaryLabel}
            title={busy ? "Working" : primaryLabel}
            disabled={primaryDisabled}
            onClick={() => {
              if (busy) {
                onPrimaryAction();
                return;
              }
              onSubmit();
            }}
          >
            {busy ? "…" : "↑"}
          </button>
          {!isFull ? (
            <button
              ref={menuButtonRef}
              type="button"
              className="buddy-panel__icon-button"
              aria-label="Open buddy actions"
              title="Open buddy actions"
              onClick={() => {
                if (menuButtonRef.current) {
                  onOpenMenu(menuButtonRef.current);
                }
              }}
            >
              ⋯
            </button>
          ) : null}
        </div>
      </form>

      {isFull ? (
        <footer className="buddy-panel__footer">
          <button
            type="button"
            className="buddy-panel__icon-button"
            aria-label="Collapse chat"
            title="Collapse chat"
            onClick={onCollapse}
          >
            ⊟
          </button>
          <button type="button" className="buddy-panel__model-pill" onClick={onOpenSettings}>
            <span>{modelLabel}</span>
            {hasGateway ? (
              <span className="buddy-panel__gateway-pill">
                {gatewayState === "connected" ? "Hermes" : connectionLabelForState(gatewayState)}
              </span>
            ) : null}
          </button>
          <button
            ref={menuButtonRef}
            type="button"
            className="buddy-panel__icon-button"
            aria-label="Open buddy actions"
            title="Open buddy actions"
            onClick={() => {
              if (menuButtonRef.current) {
                onOpenMenu(menuButtonRef.current);
              }
            }}
          >
            ⋯
          </button>
        </footer>
      ) : null}
    </div>
  );
}
