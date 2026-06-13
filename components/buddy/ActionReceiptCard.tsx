import type { ActionReceipt } from "../../src/core";
import { isActionEntry, readReceiptLedger } from "../../src/receiptLedger";

const DECISION_LABEL: Record<ActionReceipt["decision"], string> = {
  allow: "Allowed",
  needs_confirmation: "Needs confirmation",
  blocked: "Blocked",
};

type ActionReceiptCardProps = {
  receipt: ActionReceipt;
  /** Present the confirm affordance (only meaningful when decision is needs_confirmation). */
  onConfirm?: () => void;
};

/**
 * Renders the outcome of the governance action gate: the decision, the soul's derivation
 * trail (the same DerivationStep[] the memory receipts use), a Confirm affordance when the
 * gate is waiting on the user, and — once allowed — the read-only receipt ledger that
 * `receipt_review` exists to open.
 */
export function ActionReceiptCard({ receipt, onConfirm }: ActionReceiptCardProps) {
  const ledger = receipt.decision === "allow" ? readReceiptLedger().filter(isActionEntry) : [];

  return (
    <div className="action-receipt-card" data-decision={receipt.decision}>
      <header className="action-receipt-card__head">
        <span className="action-receipt-card__badge" data-decision={receipt.decision}>
          {DECISION_LABEL[receipt.decision]}
        </span>
        <span className="action-receipt-card__effector">{receipt.effector}</span>
        <span className="action-receipt-card__risk">risk: {receipt.risk}</span>
      </header>

      <ol className="action-receipt-card__rules">
        {receipt.rules.map((rule, index) => (
          <li key={`${rule.policy_rule}:${index}`}>
            <code>{rule.policy_rule}</code>
            <span>{rule.reason}</span>
          </li>
        ))}
      </ol>

      {receipt.decision === "needs_confirmation" && onConfirm ? (
        <button type="button" className="action-receipt-card__confirm" onClick={onConfirm}>
          Confirm and run
        </button>
      ) : null}

      {receipt.decision === "allow" ? (
        <section className="action-receipt-card__ledger" aria-label="Recent action receipts">
          <h4>Action ledger ({ledger.length})</h4>
          <ul>
            {ledger
              .slice(-5)
              .reverse()
              .map((entry) => (
                <li key={entry.entryId}>
                  <code>{entry.decision}</code> {entry.effector} · {entry.posture}
                </li>
              ))}
          </ul>
        </section>
      ) : null}
    </div>
  );
}
