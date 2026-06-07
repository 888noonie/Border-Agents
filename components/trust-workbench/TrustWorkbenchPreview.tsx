import { useMemo, useState } from "react";
import { runHermesMemoryDemo } from "../../src/core/demo/hermesMemoryDemo";
import type { BuiltInPurpose } from "../../src/core/policies";
import { buildNexusPanelData, buildVeritasPanelData, type NexusPanelData, type VeritasPanelData } from "../../src/core";
import "./TrustWorkbenchPreview.css";

const PURPOSE_LABELS: Record<BuiltInPurpose, string> = {
  summarize_history: "History",
  answer_current_policy: "Current Policy",
  agent_action: "Action",
  external_share: "Share",
};

const GRADE_LABELS = {
  trusted: "Trusted",
  limited: "Limited",
  reference_only: "Reference",
  blocked: "Blocked",
  quarantined: "Quarantined",
} as const;

export function TrustWorkbenchPreview() {
  const results = useMemo(() => runHermesMemoryDemo(), []);
  const [purpose, setPurpose] = useState<BuiltInPurpose>("agent_action");
  const active = results.find((result) => result.purpose === purpose) ?? results[0];
  const nexus = buildNexusPanelData({ frame: active.frame, prompt: active.prompt });
  const veritas = buildVeritasPanelData({ frame: active.frame, prompt: active.prompt });

  return (
    <aside className="trust-preview" aria-label="Trust Workbench preview">
      <div className="trust-preview__header">
        <div>
          <p className="trust-preview__eyebrow">Trust Workbench</p>
          <h1>Memory grading preview</h1>
        </div>
        <span className={`trust-preview__badge trust-preview__badge--${nexus.trustBadgeState}`}>
          {GRADE_LABELS[nexus.trustBadgeState]}
        </span>
      </div>

      <div className="trust-preview__purpose-tabs" aria-label="Purpose">
        {results.map((result) => (
          <button
            className={result.purpose === purpose ? "trust-preview__tab trust-preview__tab--active" : "trust-preview__tab"}
            key={result.purpose}
            type="button"
            onClick={() => setPurpose(result.purpose)}
          >
            {PURPOSE_LABELS[result.purpose]}
          </button>
        ))}
      </div>

      <NexusPreview data={nexus} />
      <VeritasPreview data={veritas} />
    </aside>
  );
}

function NexusPreview({ data }: { data: NexusPanelData }) {
  return (
    <section className="trust-preview__section" aria-labelledby="nexus-preview-title">
      <div className="trust-preview__section-heading">
        <h2 id="nexus-preview-title">Nexus</h2>
        <span>{data.retrievedCount} retrieved</span>
      </div>

      <div className="trust-preview__metrics" aria-label="Grade buckets">
        {Object.entries(data.frameBuckets).map(([grade, count]) => (
          <div className={`trust-preview__metric trust-preview__metric--${grade}`} key={grade}>
            <span>{GRADE_LABELS[grade as keyof typeof GRADE_LABELS]}</span>
            <strong>{count}</strong>
          </div>
        ))}
      </div>

      <div className="trust-preview__prompt">
        <span>Prompt</span>
        <strong>{data.promptSummary.included} in</strong>
        <strong>{data.promptSummary.excluded} out</strong>
      </div>

      <div className="trust-preview__source-list" aria-label="Sources">
        {data.topSources.slice(0, 4).map((source) => (
          <div className="trust-preview__source" key={`${source.sourceType}:${source.sourceId}`}>
            <span>{source.sourceId}</span>
            <strong>{GRADE_LABELS[source.highestGrade]}</strong>
          </div>
        ))}
      </div>
    </section>
  );
}

function VeritasPreview({ data }: { data: VeritasPanelData }) {
  const warnings = data.warnings.slice(0, 4);
  const evidence = data.evidenceReady.slice(0, 4);

  return (
    <section className="trust-preview__section" aria-labelledby="veritas-preview-title">
      <div className="trust-preview__section-heading">
        <h2 id="veritas-preview-title">Veritas</h2>
        <span>{data.warnings.length} warnings</span>
      </div>

      <div className="trust-preview__list" aria-label="Warnings">
        {warnings.length > 0 ? (
          warnings.map((warning) => (
            <div className="trust-preview__row" key={warning.chunkId}>
              <span>{warning.chunkId}</span>
              <strong>{warning.finalReason}</strong>
              <small>{warning.policyRules.join(", ")}</small>
            </div>
          ))
        ) : (
          <div className="trust-preview__empty">No warnings for this purpose.</div>
        )}
      </div>

      <div className="trust-preview__section-heading trust-preview__section-heading--sub">
        <h3>Evidence ready</h3>
        <span>{data.evidenceReady.length} items</span>
      </div>
      <div className="trust-preview__list" aria-label="Evidence ready">
        {evidence.map((item) => (
          <div className="trust-preview__row trust-preview__row--evidence" key={item.chunkId}>
            <span>{item.chunkId}</span>
            <strong>{GRADE_LABELS[item.grade]}</strong>
            <small>{item.promptStatus}</small>
          </div>
        ))}
      </div>
    </section>
  );
}
