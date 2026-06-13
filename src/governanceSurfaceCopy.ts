import { buildNexusPanelData, buildVeritasPanelData } from "./core";
import { buildGovernanceBuddyMessages, type BuddyGovernanceSnapshot } from "./liveGovernance";
import type { ReceiptLedgerSummary } from "./receiptLedger";

export interface GovernanceSurfaceCopy {
  buddyMessages: ReturnType<typeof buildGovernanceBuddyMessages> | null;
  tickerMessages: string[];
  passThroughMessage: string;
}

export function buildGovernanceSurfaceCopy(args: {
  snapshot: BuddyGovernanceSnapshot | null;
  ledgerSummary: ReceiptLedgerSummary;
}): GovernanceSurfaceCopy {
  const { snapshot, ledgerSummary } = args;

  if (!snapshot) {
    return {
      buddyMessages: null,
      tickerMessages: [
        ledgerSummary.frameCount > 0
          ? `Ledger: ${ledgerSummary.frameCount} frames and ${ledgerSummary.receiptCount} receipts saved locally`
          : "Ledger: awaiting the first live graded frame",
        ...withActionLedgerLine(ledgerSummary),
        "Nexus: live context grades appear after the first governed session",
        "Veritas: receipt warnings will surface here when real frames are graded",
      ],
      passThroughMessage:
        ledgerSummary.frameCount > 0
          ? `Pass-through on - Border Wizard is not clickable. ${ledgerSummary.frameCount} graded frames remain saved locally.`
          : "Pass-through on - Border Wizard is not clickable. No live graded frame has been saved yet.",
    };
  }

  const nexus = buildNexusPanelData({
    frame: snapshot.frame,
    prompt: snapshot.prompt,
  });
  const veritas = buildVeritasPanelData({
    frame: snapshot.frame,
    prompt: snapshot.prompt,
  });
  const heldCount =
    nexus.frameBuckets.reference_only +
    nexus.frameBuckets.blocked +
    nexus.frameBuckets.quarantined;

  return {
    buddyMessages: buildGovernanceBuddyMessages(snapshot),
    tickerMessages: [
      `Purpose: ${purposeLabel(snapshot.purpose)} live`,
      `Nexus: ${nexus.frameBuckets.trusted} trusted, ${nexus.frameBuckets.limited} limited, ${heldCount} held back`,
      veritas.warnings.length > 0
        ? `Veritas: ${veritas.warnings.length} receipt warnings`
        : `Veritas: ${veritas.evidenceReady.length} prompt entries backed by receipts`,
      `Prompt: ${nexus.promptSummary.included} in context, ${nexus.promptSummary.excluded} excluded`,
      `Ledger: ${ledgerSummary.frameCount} frames and ${ledgerSummary.receiptCount} receipts saved locally`,
      ...withActionLedgerLine(ledgerSummary),
    ],
    passThroughMessage: `Pass-through on - Border Wizard is not clickable. Latest ${purposeLabel(snapshot.purpose).toLowerCase()} frame keeps ${nexus.promptSummary.included} prompt entries and ${veritas.warnings.length} warning${veritas.warnings.length === 1 ? "" : "s"}.`,
  };
}

/**
 * Forge's action membrane, made visible in the same ticker as the memory ledger: how many
 * effector invocations were authorized, the latest decision, and how many actually executed.
 * Returns an empty array (spread to nothing) until the first action is gated, so the ledger
 * copy stays quiet on a memory-only session.
 */
function withActionLedgerLine(summary: { actionCount: number; latestActionDecision: string | null; executionCount: number; latestExecutionOutcome: string | null }): string[] {
  if (summary.actionCount === 0) {
    return [];
  }
  const decision = summary.latestActionDecision ? ` · latest ${summary.latestActionDecision.replace(/_/g, " ")}` : "";
  const executed =
    summary.executionCount > 0
      ? `, ${summary.executionCount} executed${summary.latestExecutionOutcome ? ` (${summary.latestExecutionOutcome})` : ""}`
      : "";
  return [`Actions: ${summary.actionCount} gated${decision}${executed}`];
}

function purposeLabel(purpose: string) {
  if (purpose === "summarize_history") {
    return "History";
  }

  if (purpose === "answer_current_policy") {
    return "Policy";
  }

  if (purpose === "agent_action") {
    return "Action";
  }

  if (purpose === "external_share") {
    return "Share";
  }

  return purpose;
}
