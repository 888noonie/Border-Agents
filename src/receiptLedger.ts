import type { ActionDecision, ActionReceipt, Grade, GradeReceipt, PurposePolicy, UserPosture } from "./core";
import type { EffectorId } from "./buddyManifest";
import type { BuddyGovernanceSnapshot } from "./liveGovernance";

export const RECEIPT_LEDGER_STORAGE_KEY = "border-agents:receipt-ledger:v1";
const DEFAULT_MAX_LEDGER_ENTRIES = 50;

/**
 * A memory-grading ledger entry — one per buddy interaction. `kind` is optional so
 * entries written before action receipts existed (no `kind` field) still read back as
 * memory entries (see `isActionEntry`); the storage key stays `:v1`, no migration.
 */
export interface MemoryReceiptLedgerEntry {
  kind?: "memory";
  entryId: string;
  buddyId: string;
  purpose: string;
  recordedAt: string;
  frameCounts: Record<Grade, number>;
  promptIncluded: number;
  promptExcluded: number;
  receipts: GradeReceipt[];
}

/** An action-authorization ledger entry — one per gate decision (`src/core/actionGate.ts`). */
export interface ActionReceiptLedgerEntry {
  kind: "action";
  entryId: string;
  buddyId: string;
  recordedAt: string;
  effector: EffectorId;
  decision: ActionDecision;
  risk: PurposePolicy["risk"];
  posture: UserPosture;
  confirmed: boolean;
  /** Single element — keeps `.receipts` uniform across both entry kinds for counting. */
  receipts: ActionReceipt[];
}

export type ReceiptLedgerEntry = MemoryReceiptLedgerEntry | ActionReceiptLedgerEntry;

export function isActionEntry(entry: ReceiptLedgerEntry): entry is ActionReceiptLedgerEntry {
  return entry.kind === "action";
}

export function isMemoryEntry(entry: ReceiptLedgerEntry): entry is MemoryReceiptLedgerEntry {
  return entry.kind !== "action";
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
  /** How many entries are action-authorization receipts. */
  actionCount: number;
  /** Decision of the most recent action entry, or null if there is none. */
  latestActionDecision: ActionDecision | null;
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

/** Shared append: dedupe by entryId, cap to the most recent `maxEntries`, fail-soft on write. */
function appendEntry(entry: ReceiptLedgerEntry, storage: Storage, maxEntries: number): ReceiptLedgerEntry[] {
  const current = readReceiptLedger(storage);

  if (current.some((item) => item.entryId === entry.entryId)) {
    return current;
  }

  const next = [...current, entry].slice(-maxEntries);

  try {
    storage.setItem(RECEIPT_LEDGER_STORAGE_KEY, JSON.stringify(next));
  } catch {
    return current;
  }

  return next;
}

export function appendSnapshotToReceiptLedger(args: {
  buddyId: string;
  snapshot: BuddyGovernanceSnapshot;
  storage?: Storage;
  maxEntries?: number;
}): ReceiptLedgerEntry[] {
  const storage = args.storage ?? window.localStorage;
  const maxEntries = args.maxEntries ?? DEFAULT_MAX_LEDGER_ENTRIES;
  return appendEntry(toMemoryLedgerEntry(args.buddyId, args.snapshot), storage, maxEntries);
}

export function appendActionReceiptToLedger(args: {
  buddyId: string;
  receipt: ActionReceipt;
  storage?: Storage;
  maxEntries?: number;
}): ReceiptLedgerEntry[] {
  const storage = args.storage ?? window.localStorage;
  const maxEntries = args.maxEntries ?? DEFAULT_MAX_LEDGER_ENTRIES;
  return appendEntry(toActionLedgerEntry(args.buddyId, args.receipt), storage, maxEntries);
}

export function summarizeReceiptLedger(entries: ReceiptLedgerEntry[]): ReceiptLedgerSummary {
  const latest = entries[entries.length - 1] ?? null;
  const receiptCount = entries.reduce((count, entry) => count + entry.receipts.length, 0);
  const actionCount = entries.filter(isActionEntry).length;
  const latestAction = [...entries].reverse().find(isActionEntry) ?? null;

  return {
    entryCount: entries.length,
    receiptCount,
    latestBuddyId: latest?.buddyId ?? null,
    latestPurpose: latest && isMemoryEntry(latest) ? latest.purpose : null,
    latestRecordedAt: latest?.recordedAt ?? null,
    latestWarnings: latest && isMemoryEntry(latest) ? latest.frameCounts.blocked + latest.frameCounts.quarantined : 0,
    latestPromptIncluded: latest && isMemoryEntry(latest) ? latest.promptIncluded : 0,
    latestPromptExcluded: latest && isMemoryEntry(latest) ? latest.promptExcluded : 0,
    actionCount,
    latestActionDecision: latestAction?.decision ?? null,
  };
}

function toMemoryLedgerEntry(buddyId: string, snapshot: BuddyGovernanceSnapshot): MemoryReceiptLedgerEntry {
  const latestReceipt = snapshot.frame.receipts[snapshot.frame.receipts.length - 1];
  const recordedAt = latestReceipt?.derived_at ?? new Date().toISOString();

  return {
    kind: "memory",
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

function toActionLedgerEntry(buddyId: string, receipt: ActionReceipt): ActionReceiptLedgerEntry {
  return {
    kind: "action",
    entryId: `action:${buddyId}:${receipt.receipt_id}`,
    buddyId,
    recordedAt: receipt.derived_at,
    effector: receipt.effector,
    decision: receipt.decision,
    risk: receipt.risk,
    posture: receipt.posture,
    confirmed: receipt.confirmed,
    receipts: [receipt],
  };
}
