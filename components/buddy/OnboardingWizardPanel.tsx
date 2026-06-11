import { BUDDY_PROVIDER_LABELS } from "../../src/buddyProfiles";
import type {
  ConnectSectionModel,
  OnboardingPanelModel,
  OnboardingPanelSection,
  PlacementSectionModel,
  PostureSectionModel,
  SummarySectionModel,
} from "../../src/onboardingPanelModel";
import type { OnboardingSurfaceDraft } from "../../src/onboardingSurfaceState";
import "./buddy-surface.css";

type OnboardingWizardPanelProps = {
  model: OnboardingPanelModel;
  draft: OnboardingSurfaceDraft;
  onAdvance: (event: string) => void;
  onDraftChange: (patch: Partial<OnboardingSurfaceDraft>) => void;
  onSectionSelect: (section: OnboardingPanelSection) => void;
};

export function OnboardingWizardPanel({
  model,
  draft,
  onAdvance,
  onDraftChange,
  onSectionSelect,
}: OnboardingWizardPanelProps) {
  return (
    <section className="onboarding-panel" aria-label="Onboarding wizard">
      <header className="onboarding-panel__header">
        <div>
          <p className="onboarding-panel__eyebrow">
            {model.mode === "hub" ? "Setup hub" : "Setup flow"}
          </p>
          <h3>{model.act.title}</h3>
        </div>
        <span className="onboarding-panel__mode">
          {model.mode === "hub" ? "Hub" : "Guided"}
        </span>
      </header>

      <nav className="onboarding-panel__nav" aria-label="Setup sections">
        {model.nav.map((item) => (
          <button
            key={item.section}
            type="button"
            className={[
              "onboarding-panel__nav-item",
              item.active ? "onboarding-panel__nav-item--active" : "",
            ].join(" ")}
            disabled={!item.enabled}
            onClick={() => onSectionSelect(item.section)}
          >
            {item.label}
          </button>
        ))}
      </nav>

      {model.section ? (
        <div className="onboarding-panel__body">
          {model.section.kind === "connect" ? (
            <ConnectSection model={model.section} draft={draft} onDraftChange={onDraftChange} onAdvance={onAdvance} />
          ) : null}
          {model.section.kind === "posture" ? (
            <PostureSection model={model.section} draft={draft} onDraftChange={onDraftChange} onAdvance={onAdvance} />
          ) : null}
          {model.section.kind === "placement" ? (
            <PlacementSection model={model.section} draft={draft} onDraftChange={onDraftChange} onAdvance={onAdvance} />
          ) : null}
          {model.section.kind === "summary" ? (
            <SummarySection model={model.section} onAdvance={onAdvance} />
          ) : null}
        </div>
      ) : (
        <div className="onboarding-panel__body">
          <p className="onboarding-panel__intro">
            The host is ready. Open the setup flow when you want the full panel to step through connection, posture, placement, and receipts.
          </p>
          <button
            type="button"
            className="onboarding-panel__primary"
            onClick={() => onAdvance("clicked")}
          >
            Let&apos;s set up
          </button>
        </div>
      )}
    </section>
  );
}

function ConnectSection({
  model,
  draft,
  onDraftChange,
  onAdvance,
}: {
  model: ConnectSectionModel;
  draft: OnboardingSurfaceDraft;
  onDraftChange: (patch: Partial<OnboardingSurfaceDraft>) => void;
  onAdvance: (event: string) => void;
}) {
  const activePreset = model.providers.find((provider) => provider.id === draft.provider);
  const requiresApiKey = activePreset?.requiresApiKey ?? true;
  const canConnect =
    draft.apiBase.trim().length > 0 &&
    draft.model.trim().length > 0 &&
    (!requiresApiKey || draft.apiKey.trim().length > 0);
  return (
    <>
      <div className="onboarding-panel__providers">
        {model.providers.map((provider) => (
          <button
            key={provider.id}
            type="button"
            className={[
              "onboarding-panel__provider",
              draft.provider === provider.id ? "onboarding-panel__provider--active" : "",
            ].join(" ")}
            onClick={() =>
              onDraftChange({
                provider: provider.id,
                apiBase: provider.apiBase,
                model: provider.modelPlaceholder,
              })}
          >
            <strong>{provider.label}</strong>
            <span>{provider.helper}</span>
          </button>
        ))}
      </div>

      <div className="onboarding-panel__fields">
        {model.fields.map((field) => (
          <label key={field.key} className="onboarding-panel__field">
            <span>{field.label}</span>
            {field.key === "provider" ? (
              <select
                value={draft.provider}
                onChange={(event) => {
                  const next = model.providers.find((provider) => provider.id === event.target.value);
                  if (!next) {
                    return;
                  }
                  onDraftChange({
                    provider: next.id,
                    apiBase: next.apiBase,
                    model: next.modelPlaceholder,
                  });
                }}
              >
                {model.providers.map((provider) => (
                  <option key={provider.id} value={provider.id}>
                    {provider.label}
                  </option>
                ))}
              </select>
            ) : field.control === "textarea" ? (
              <textarea
                rows={3}
                value={fieldValue(draft, field.key)}
                placeholder={field.placeholder}
                onChange={(event) => onDraftChange({ [field.key]: event.target.value } as Partial<OnboardingSurfaceDraft>)}
              />
            ) : (
              <input
                type={field.control === "password" ? "password" : "text"}
                value={fieldValue(draft, field.key)}
                placeholder={field.placeholder}
                onChange={(event) => onDraftChange({ [field.key]: event.target.value } as Partial<OnboardingSurfaceDraft>)}
              />
            )}
          </label>
        ))}
      </div>

      <p className="onboarding-panel__note">
        Credentials still live in `.env` today; this panel is the real flow shell we&apos;ll keep wiring into receipt-backed settings.
        {requiresApiKey
          ? " Hosted providers need an API key before we record the connection."
          : " Local providers connect without an API key."}
      </p>
      <button
        type="button"
        className="onboarding-panel__primary"
        disabled={!canConnect}
        onClick={() => {
          if (!canConnect) {
            return;
          }
          onAdvance("panel:connection_ok");
        }}
      >
        {model.primaryActionLabel}
      </button>
    </>
  );
}

function PostureSection({
  model,
  draft,
  onDraftChange,
  onAdvance,
}: {
  model: PostureSectionModel;
  draft: OnboardingSurfaceDraft;
  onDraftChange: (patch: Partial<OnboardingSurfaceDraft>) => void;
  onAdvance: (event: string) => void;
}) {
  return (
    <>
      <div className="onboarding-panel__cards">
        {model.options.map((option) => (
          <button
            key={option.posture}
            type="button"
            className={[
              "onboarding-panel__card",
              draft.posture === option.posture ? "onboarding-panel__card--active" : "",
            ].join(" ")}
            onClick={() => onDraftChange({ posture: option.posture })}
          >
            <strong>{option.label}</strong>
            <span>{option.consequence}</span>
            <small>{option.authorizationSummary}</small>
            <small>{option.interactionSummary}</small>
          </button>
        ))}
      </div>
      <button type="button" className="onboarding-panel__primary" onClick={() => onAdvance("panel:posture_set")}>
        Save posture
      </button>
    </>
  );
}

function PlacementSection({
  model,
  draft,
  onDraftChange,
  onAdvance,
}: {
  model: PlacementSectionModel;
  draft: OnboardingSurfaceDraft;
  onDraftChange: (patch: Partial<OnboardingSurfaceDraft>) => void;
  onAdvance: (event: string) => void;
}) {
  return (
    <>
      <div className="onboarding-panel__placement-list">
        {model.buddyChoices.map((choice) => {
          const enabled = draft.enabledBuddyIds.includes(choice.buddyId);
          return (
            <div key={choice.buddyId} className="onboarding-panel__placement-row">
              <label className="onboarding-panel__check">
                <input
                  type="checkbox"
                  checked={enabled}
                  onChange={(event) => {
                    const enabledBuddyIds = event.target.checked
                      ? [...new Set([...draft.enabledBuddyIds, choice.buddyId])]
                      : draft.enabledBuddyIds.filter((buddyId) => buddyId !== choice.buddyId);
                    onDraftChange({ enabledBuddyIds });
                  }}
                />
                <span>{choice.label}</span>
              </label>
              <select
                value={draft.buddyEdges[choice.buddyId] ?? choice.defaultEdge}
                onChange={(event) =>
                  onDraftChange({
                    buddyEdges: {
                      ...draft.buddyEdges,
                      [choice.buddyId]: event.target.value as OnboardingSurfaceDraft["buddyEdges"][string],
                    },
                  })}
              >
                {model.edgeChoices.map((edge) => (
                  <option key={edge.edge} value={edge.edge}>
                    {edge.label}
                  </option>
                ))}
              </select>
            </div>
          );
        })}
      </div>

      <label className="onboarding-panel__field">
        <span>Target display</span>
        <select
          value={String(draft.outputIndex)}
          onChange={(event) => onDraftChange({ outputIndex: Number(event.target.value) })}
        >
          {model.outputChoices.map((output) => (
            <option key={output.value} value={output.value}>
              {output.label}
            </option>
          ))}
        </select>
      </label>

      <button type="button" className="onboarding-panel__primary" onClick={() => onAdvance("panel:next")}>
        Save placement
      </button>
    </>
  );
}

function SummarySection({
  model,
  onAdvance,
}: {
  model: SummarySectionModel;
  onAdvance: (event: string) => void;
}) {
  return (
    <>
      <div className="onboarding-panel__summary">
        {model.rows.map((row) => (
          <div key={row.receipt} className="onboarding-panel__summary-row">
            <div>
              <strong>{row.title}</strong>
              <span>{row.receipt}</span>
            </div>
            <em>{row.status === "recorded" ? "Recorded" : "Pending"}</em>
          </div>
        ))}
      </div>
      <button type="button" className="onboarding-panel__primary" onClick={() => onAdvance("panel:done")}>
        Finish handoff
      </button>
    </>
  );
}

function fieldValue(draft: OnboardingSurfaceDraft, key: ConnectSectionModel["fields"][number]["key"]) {
  switch (key) {
    case "provider":
      return BUDDY_PROVIDER_LABELS[draft.provider === "xai" ? "grok" : draft.provider];
    case "apiBase":
      return draft.apiBase;
    case "apiKey":
      return draft.apiKey;
    case "model":
      return draft.model;
    case "systemPrompt":
      return draft.systemPrompt;
  }
}
