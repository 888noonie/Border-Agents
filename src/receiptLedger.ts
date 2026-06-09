import type { Grade, GradeReceipt } from "./core";
import type { BuddyGovernanceSnapshot } from "./liveGovernance";

export const RECEIPT_LEDGER_STORAGE_KEY = "border-agents:receipt-ledger:v1";
const DEFAULT_MAX_LEDGER_ENTRIES = 50;

export interface ReceiptLedgerEntry {
  entryId: string;
  buddyId: string;
  purpose: string;
  recordedAt: string;
  frameCounts: Record<Grade, number>;
  promptIncluded: number;
  promptExcluded: number;
  receipts: GradeReceipt[];
}

export interface ReceiptLedgerSummary {
  entryCount: number;
  receiptCount: number;
  latestBuddyId: string | null;
  latestPurpose: string | null;
  latestRecordedAt: string | null;
  latestWarnings: number;
  latestPromptIncluded: number;
  latestPromptExcluded: number;
}

export function readReceiptLedger(storage: Storage = window.localStorage): ReceiptLedgerEntry[] {
  try {
    const raw = storage.getItem(RECEIPT_LEDGER_STORAGE_KEY);
    if (!raw) {
      return [];
    }

    const parsed = JSON.parse(raw) as ReceiptLedgerEntry[];
    return Array.isArray(parsed) ? parsed : [];
  } catch {
    return [];
  }
}

export function appendSnapshotToReceiptLedger(args: {
  buddyId: string;
  snapshot: BuddyGovernanceSnapshot;
  storage?: Storage;
  maxEntries?: number;
}): ReceiptLedgerEntry[] {
  const storage = args.storage ?? window.localStorage;
  const current = readReceiptLedger(storage);
  const entry = toLedgerEntry(args.buddyId, args.snapshot);

  if (current.some((item) => item.entryId === entry.entryId)) {
    return current;
  }

  const maxEntries = args.maxEntries ?? DEFAULT_MAX_LEDGER_ENTRIES;
  const next = [...current, entry].slice(-maxEntries);

  try {
    storage.setItem(RECEIPT_LEDGER_STORAGE_KEY, JSON.stringify(next));
  } catch {
    return current;
  }

  return next;
}

export function summarizeReceiptLedger(entries: ReceiptLedgerEntry[]): ReceiptLedgerSummary {
  const latest = entries[entries.length - 1] ?? null;
  const receiptCount = entries.reduce((count, entry) => count + entry.receipts.length, 0);

  return {
    entryCount: entries.length,
    receiptCount,
    latestBuddyId: latest?.buddyId ?? null,
    latestPurpose: latest?.purpose ?? null,
    latestRecordedAt: latest?.recordedAt ?? null,
    latestWarnings: latest ? latest.frameCounts.blocked + latest.frameCounts.quarantined : 0,
    latestPromptIncluded: latest?.promptIncluded ?? 0,
    latestPromptExcluded: latest?.promptExcluded ?? 0,
  };
}

function toLedgerEntry(buddyId: string, snapshot: BuddyGovernanceSnapshot): ReceiptLedgerEntry {
  const latestReceipt = snapshot.frame.receipts[snapshot.frame.receipts.length - 1];
  const recordedAt = latestReceipt?.derived_at ?? new Date().toISOString();

  return {
    entryId: latestReceipt?.receipt_id
      ? `${buddyId}:${snapshot.purpose}:${latestReceipt.receipt_id}`
      : `${buddyId}:${snapshot.purpose}:${recordedAt}`,
    buddyId,
    purpose: snapshot.purpose,
    recordedAt,
    frameCounts: {
      trusted: snapshot.frame.trusted.length,
      limited: snapshot.frame.limited.length,
      reference_only: snapshot.frame.reference_only.length,
      blocked: snapshot.frame.blocked.length,
      quarantined: snapshot.frame.quarantined.length,
    },
    promptIncluded: snapshot.prompt.included.length,
    promptExcluded: snapshot.prompt.excluded.length,
    receipts: snapshot.frame.receipts,
  };
}
