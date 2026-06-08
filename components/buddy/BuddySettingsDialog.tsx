import {
  useEffect,
  useRef,
  useState,
  type ChangeEvent,
  type RefObject,
} from "react";
import {
  BUDDY_MEMORY_LABELS,
  BUDDY_PROVIDER_LABELS,
  type BuddyMemoryMode,
  type BuddyProvider,
  type BuddySettings,
} from "../../src/buddyProfiles";
import type { GatewayConnectionState } from "../../src/gatewayProtocol";
import { normalizeGatewaySettings, type GatewaySettings } from "../../src/gatewaySettings";
import { connectionLabelForState } from "../../src/useBuddyGateway";
import "./buddy-surface.css";

type BuddySettingsDialogProps = {
  open: boolean;
  buddyName: string;
  ownerLabel: string;
  settings: BuddySettings;
  hasGateway: boolean;
  gatewayState: GatewayConnectionState;
  gatewayDetail: string | null;
  gatewayUrl: string;
  gatewayAutoConnect: boolean;
  preventOverflow?: boolean;
  dialogRef?: RefObject<HTMLDivElement | null>;
  onClose: () => void;
  onSave: (settings: BuddySettings, gatewaySettings: GatewaySettings) => void;
  onGatewayConnect: () => void;
  onGatewayDisconnect: () => void;
};

export function BuddySettingsDialog({
  open,
  buddyName,
  ownerLabel,
  settings,
  hasGateway,
  gatewayState,
  gatewayDetail,
  gatewayUrl,
  gatewayAutoConnect,
  preventOverflow = true,
  dialogRef,
  onClose,
  onSave,
  onGatewayConnect,
  onGatewayDisconnect,
}: BuddySettingsDialogProps) {
  const localCardRef = useRef<HTMLDivElement>(null);
  const cardRef = dialogRef ?? localCardRef;
  const [draft, setDraft] = useState(settings);
  const [gatewayDraft, setGatewayDraft] = useState(
    normalizeGatewaySettings({ url: gatewayUrl, autoConnect: gatewayAutoConnect }),
  );

  useEffect(() => {
    if (!open) {
      return;
    }
    setDraft(settings);
    setGatewayDraft(normalizeGatewaySettings({ url: gatewayUrl, autoConnect: gatewayAutoConnect }));
  }, [gatewayAutoConnect, gatewayUrl, open, settings]);

  useEffect(() => {
    if (!open) {
      return;
    }

    function handlePointerDown(event: PointerEvent) {
      const target = event.target as Node | null;
      if (cardRef.current?.contains(target)) {
        return;
      }
      onClose();
    }

    function handleKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") {
        onClose();
      }
    }

    window.addEventListener("pointerdown", handlePointerDown, true);
    window.addEventListener("keydown", handleKeyDown);
    return () => {
      window.removeEventListener("pointerdown", handlePointerDown, true);
      window.removeEventListener("keydown", handleKeyDown);
    };
  }, [cardRef, onClose, open]);

  function updateDraft(patch: Partial<BuddySettings>) {
    setDraft((current) => ({ ...current, ...patch }));
  }

  function handleProviderChange(event: ChangeEvent<HTMLSelectElement>) {
    updateDraft({ provider: event.target.value as BuddyProvider });
  }

  function handleMemoryModeChange(event: ChangeEvent<HTMLSelectElement>) {
    updateDraft({ memoryMode: event.target.value as BuddyMemoryMode });
  }

  if (!open) {
    return null;
  }

  return (
    <div className="buddy-settings-layer" role="presentation">
      <div
        ref={cardRef}
        className={[
          "buddy-dialog__card",
          preventOverflow ? "buddy-dialog__card--prevent-overflow" : "",
        ].join(" ")}
        role="dialog"
        aria-modal="false"
      >
        <form
          className="buddy-dialog__form"
          onSubmit={(event) => {
            event.preventDefault();
            onSave(draft, gatewayDraft);
            onClose();
          }}
        >
          <header className="buddy-dialog__header">
            <p className="buddy-dialog__eyebrow">Settings</p>
            <h2>{buddyName} model, gateway, and authority</h2>
          </header>

          <div className="buddy-dialog__body">
            <label className="buddy-dialog__field">
              <span>Platform</span>
              <select value={draft.provider} onChange={handleProviderChange}>
                {Object.entries(BUDDY_PROVIDER_LABELS).map(([value, label]) => (
                  <option key={value} value={value}>
                    {label}
                  </option>
                ))}
              </select>
            </label>

            <label className="buddy-dialog__field">
              <span>Model</span>
              <input
                type="text"
                value={draft.modelLabel}
                onChange={(event) => updateDraft({ modelLabel: event.target.value })}
              />
            </label>

            <label className="buddy-dialog__field">
              <span>Memory</span>
              <select value={draft.memoryMode} onChange={handleMemoryModeChange}>
                {Object.entries(BUDDY_MEMORY_LABELS).map(([value, label]) => (
                  <option key={value} value={value}>
                    {label}
                  </option>
                ))}
              </select>
            </label>

            <label className="buddy-dialog__check">
              <span>Agent action</span>
              <input
                type="checkbox"
                checked={draft.allowAction}
                onChange={(event) => updateDraft({ allowAction: event.target.checked })}
              />
            </label>

            <label className="buddy-dialog__check">
              <span>External share</span>
              <input
                type="checkbox"
                checked={draft.allowExternalShare}
                onChange={(event) => updateDraft({ allowExternalShare: event.target.checked })}
              />
            </label>

            {hasGateway ? (
              <section className="buddy-dialog__gateway">
                <div className="buddy-dialog__gateway-head">
                  <strong>Desktop gateway</strong>
                  <span className={`buddy-dialog__gateway-state buddy-dialog__gateway-state--${gatewayState}`}>
                    {connectionLabelForState(gatewayState)}
                  </span>
                </div>
                <p className="buddy-dialog__gateway-detail">
                  Desktop gateway routes Hermes chat through the configured provider adapter.
                </p>
                {gatewayDetail ? <p className="buddy-dialog__gateway-detail">{gatewayDetail}</p> : null}
                <label className="buddy-dialog__field">
                  <span>Gateway URL</span>
                  <input
                    type="url"
                    value={gatewayDraft.url}
                    onChange={(event) =>
                      setGatewayDraft(
                        normalizeGatewaySettings({
                          url: event.target.value,
                          autoConnect: gatewayDraft.autoConnect,
                        }),
                      )
                    }
                  />
                </label>
                <label className="buddy-dialog__check">
                  <span>Auto-connect</span>
                  <input
                    type="checkbox"
                    checked={gatewayDraft.autoConnect}
                    onChange={(event) =>
                      setGatewayDraft(
                        normalizeGatewaySettings({
                          url: gatewayDraft.url,
                          autoConnect: event.target.checked,
                        }),
                      )
                    }
                  />
                </label>
                <div className="buddy-dialog__gateway-actions">
                  {gatewayState === "connected" ? (
                    <button type="button" onClick={onGatewayDisconnect}>
                      Disconnect
                    </button>
                  ) : (
                    <button type="button" onClick={onGatewayConnect}>
                      Connect now
                    </button>
                  )}
                </div>
              </section>
            ) : null}

            <p className="buddy-dialog__note">
              {ownerLabel} connection: {draft.connectionLabel}
            </p>
          </div>

          <footer className="buddy-dialog__footer">
            <button type="button" className="buddy-dialog__secondary" onClick={onClose}>
              Cancel
            </button>
            <button type="submit" className="buddy-dialog__primary">
              Save
            </button>
          </footer>
        </form>
      </div>
    </div>
  );
}
