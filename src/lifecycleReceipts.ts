// Lifecycle receipt ledger — the durable, timestamped record of onboarding /
// configuration milestones (credential.stored, posture.set, placement.set,
// onboarding.completed).
//
// This is deliberately separate from `receiptLedger.ts`: that ledger records
// per-frame governance grade receipts (trusted/limited/blocked content from a
// BuddyGovernanceSnapshot). Lifecycle receipts are a different shape — they are
// milestones in the user's setup, each one a thing that actually happened, with a
// timestamp and optional detail. Keeping them honest means we only ever record a
// milestone when the underlying action really took place (a real settings write,
// not a panel click).
//
// The store is an append-only log (capped), not a flag set: re-running a step in
// the hub genuinely records a new dated milestone. `lifecycleReceiptKinds` collapses
// the log to the unique kinds that drive the wizard's linear-vs-hub entry mode.

export const LIFECYCLE_RECEIPT_STORAGE_KEY = "border-agents:lifecycle-receipts:v1";

const DEFAULT_MAX_LIFECYCLE_ENTRIES = 100;

export interface LifecycleReceipt {
  kind: string;
  recordedAt: string;
  detail?: Record<string, string | number | boolean>;
}

export function readLifecycleReceipts(storage: Storage = window.localStorage): LifecycleReceipt[] {
  try {
    const raw = storage.getItem(LIFECYCLE_RECEIPT_STORAGE_KEY);
    if (!raw) {
      return [];
    }
    const parsed = JSON.parse(raw) as unknown;
    if (!Array.isArray(parsed)) {
      return [];
    }
    return parsed.filter(isLifecycleReceipt);
  } catch {
    return [];
  }
}

export function recordLifecycleReceipt(args: {
  kind: string;
  detail?: Record<string, string | number | boolean>;
  storage?: Storage;
  now?: () => Date;
  maxEntries?: number;
}): LifecycleReceipt[] {
  const storage = args.storage ?? window.localStorage;
  const now = args.now ?? (() => new Date());
  const current = readLifecycleReceipts(storage);

  const entry: LifecycleReceipt = {
    kind: args.kind,
    recordedAt: now().toISOString(),
    ...(args.detail ? { detail: args.detail } : {}),
  };

  const maxEntries = args.maxEntries ?? DEFAULT_MAX_LIFECYCLE_ENTRIES;
  const next = [...current, entry].slice(-maxEntries);

  try {
    storage.setItem(LIFECYCLE_RECEIPT_STORAGE_KEY, JSON.stringify(next));
  } catch {
    return current;
  }
  return next;
}

// The unique receipt kinds present in the log, in first-seen order. This is what the
// wizard reads to decide linear (first run) vs hub (re-entry) and to mark summary rows
// as recorded.
export function lifecycleReceiptKinds(entries: readonly LifecycleReceipt[]): string[] {
  const seen: string[] = [];
  for (const entry of entries) {
    if (!seen.includes(entry.kind)) {
      seen.push(entry.kind);
    }
  }
  return seen;
}

// The most recent receipt of each kind, keyed by kind — useful for surfacing "when"
// alongside a milestone in governance views.
export function latestLifecycleReceiptByKind(
  entries: readonly LifecycleReceipt[],
): Record<string, LifecycleReceipt> {
  const latest: Record<string, LifecycleReceipt> = {};
  for (const entry of entries) {
    latest[entry.kind] = entry;
  }
  return latest;
}

function isLifecycleReceipt(value: unknown): value is LifecycleReceipt {
  if (!value || typeof value !== "object") {
    return false;
  }
  const candidate = value as Partial<LifecycleReceipt>;
  return typeof candidate.kind === "string" && typeof candidate.recordedAt === "string";
}
