import { useEffect, useId, useMemo, useState } from "react";
import { runHermesMemoryDemo } from "../../src/core/demo/hermesMemoryDemo";
import type { BuiltInPurpose } from "../../src/core/policies";
import {
  buildNexusPanelData,
  buildVeritasPanelData,
  type NexusPanelData,
  type VeritasPanelData,
} from "../../src/core";
import type { Grade } from "../../src/core/types";
import "./TrustWorkbenchPreview.css";

const PURPOSE_LABELS: Record<BuiltInPurpose, string> = {
  summarize_history: "History",
  answer_current_policy: "Current Policy",
  agent_action: "Action",
  external_share: "Share",
};

const GRADE_LABELS: Record<Grade, string> = {
  trusted: "Trusted",
  limited: "Limited",
  reference_only: "Reference",
  blocked: "Blocked",
  quarantined: "Quarantined",
};

type TrustWorkbenchPanelMode = "full" | "nexus" | "veritas";

type TrustWorkbenchPanelProps = {
  mode?: TrustWorkbenchPanelMode;
  title?: string;
  compact?: boolean;
};

export function TrustWorkbenchPanel({
  mode = "full",
  title = "Memory grading preview",
  compact = false,
}: TrustWorkbenchPanelProps) {
  const results = useMemo(() => runHermesMemoryDemo(), []);
  const [purpose, setPurpose] = useState<BuiltInPurpose>("agent_action");
  const active = results.find((result) => result.purpose === purpose) ?? results[0];
  const nexus = buildNexusPanelData({ frame: active.frame, prompt: active.prompt });
  const veritas = buildVeritasPanelData({ frame: active.frame, prompt: active.prompt });

  return (
    <div className={["trust-panel", compact ? "trust-panel--compact" : ""].join(" ")} aria-label="Trust Workbench">
      <div className="trust-preview__header trust-panel__header">
        <div>
          <p className="trust-preview__eyebrow">Trust Workbench</p>
          <h1>{title}</h1>
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

      {mode === "full" || mode === "nexus" ? <NexusPreview data={nexus} /> : null}
      {mode === "full" || mode === "veritas" ? <VeritasPreview data={veritas} /> : null}
      {mode !== "full" ? <WorkbenchPlaceholderActions /> : null}
    </div>
  );
}

function NexusPreview({ data }: { data: NexusPanelData }) {
  const titleId = useId();
  const bodyId = useId();
  const [open, setOpen] = useState(true);
  const counterGrade = data.frameBuckets.blocked > 0 ? "blocked" : data.trustBadgeState;
  const counterValue = data.frameBuckets[counterGrade];

  return (
    <section className="trust-preview__section trust-preview__disclosure" aria-labelledby={titleId}>
      <button
        className="trust-preview__section-toggle"
        type="button"
        aria-expanded={open}
        aria-controls={bodyId}
        onClick={() => setOpen((current) => !current)}
      >
        <span className="trust-preview__section-title">
          <span className="trust-preview__chevron" aria-hidden="true">
            {open ? "⌄" : "›"}
          </span>
          <span id={titleId}>Nexus</span>
        </span>
        <span className="trust-preview__section-counters">
          <span>{data.retrievedCount} retrieved</span>
          <span className={`trust-preview__counter trust-preview__counter--${counterGrade}`}>
            {counterValue} {counterGrade === "reference_only" ? "reference" : counterGrade}
          </span>
        </span>
      </button>

      {open ? (
        <div id={bodyId}>
          <div className="trust-preview__metrics" aria-label="Grade buckets">
            {Object.entries(data.frameBuckets).map(([grade, count]) => (
              <div className={`trust-preview__metric trust-preview__metric--${grade}`} key={grade}>
                <span>{GRADE_LABELS[grade as Grade]}</span>
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
        </div>
      ) : null}
    </section>
  );
}

function VeritasPreview({ data }: { data: VeritasPanelData }) {
  const titleId = useId();
  const bodyId = useId();
  const [open, setOpen] = useState(true);
  const warnings = data.warnings.slice(0, 4);
  const evidence = data.evidenceReady.slice(0, 4);
  const firstReceipt = warnings[0] ?? evidence[0] ?? data.receiptGroups.trusted[0];
  const [expandedChunkId, setExpandedChunkId] = useState<string | null>(firstReceipt?.chunkId ?? null);

  useEffect(() => {
    setExpandedChunkId(firstReceipt?.chunkId ?? null);
  }, [firstReceipt?.chunkId]);

  return (
    <section className="trust-preview__section trust-preview__disclosure" aria-labelledby={titleId}>
      <button
        className="trust-preview__section-toggle"
        type="button"
        aria-expanded={open}
        aria-controls={bodyId}
        onClick={() => setOpen((current) => !current)}
      >
        <span className="trust-preview__section-title">
          <span className="trust-preview__chevron" aria-hidden="true">
            {open ? "⌄" : "›"}
          </span>
          <span id={titleId}>Veritas</span>
        </span>
        <span className="trust-preview__section-counters">
          <span className="trust-preview__counter trust-preview__counter--warning">
            {data.warnings.length} warnings
          </span>
        </span>
      </button>

      {open ? (
        <div id={bodyId}>
          <div className="trust-preview__list" aria-label="Warnings">
            {warnings.length > 0 ? (
              warnings.map((warning) => (
                <ReceiptRow
                  item={warning}
                  key={warning.chunkId}
                  expanded={expandedChunkId === warning.chunkId}
                  onToggle={() => setExpandedChunkId(expandedChunkId === warning.chunkId ? null : warning.chunkId)}
                />
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
              <ReceiptRow
                item={item}
                key={item.chunkId}
                expanded={expandedChunkId === item.chunkId}
                evidence
                onToggle={() => setExpandedChunkId(expandedChunkId === item.chunkId ? null : item.chunkId)}
              />
            ))}
          </div>
        </div>
      ) : null}
    </section>
  );
}

function ReceiptRow({
  item,
  expanded,
  evidence = false,
  onToggle,
}: {
  item: VeritasPanelData["evidenceReady"][number];
  expanded: boolean;
  evidence?: boolean;
  onToggle: () => void;
}) {
  return (
    <div className="trust-preview__receipt">
      <button
        className={["trust-preview__row-button", evidence ? "trust-preview__row-button--evidence" : ""].join(" ")}
        type="button"
        aria-expanded={expanded}
        onClick={onToggle}
      >
        <span>{item.chunkId}</span>
        <strong>{evidence ? GRADE_LABELS[item.grade] : item.finalReason}</strong>
        <small>{evidence ? item.promptStatus : item.policyRules.join(", ")}</small>
      </button>
      {expanded ? (
        <div className="trust-preview__receipt-detail">
          <span>{item.packetId}</span>
          <span>{GRADE_LABELS[item.grade]}</span>
          <span>{item.promptStatus}</span>
          <small>{item.finalReason}</small>
        </div>
      ) : null}
    </div>
  );
}

function WorkbenchPlaceholderActions() {
  return (
    <div className="trust-panel__actions" aria-label="Trust actions">
      <button type="button" disabled>
        Verify
      </button>
      <button type="button" disabled>
        Open source
      </button>
      <button type="button" disabled>
        Export receipt
      </button>
      <button type="button" disabled>
        Trusted only
      </button>
    </div>
  );
}
