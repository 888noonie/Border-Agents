import { beforeEach, describe, expect, it } from "vitest";
import {
  LIFECYCLE_RECEIPT_STORAGE_KEY,
  latestLifecycleReceiptByKind,
  lifecycleReceiptKinds,
  readLifecycleReceipts,
  recordLifecycleReceipt,
} from "../lifecycleReceipts";

class MemoryStorage implements Storage {
  private store = new Map<string, string>();
  get length() {
    return this.store.size;
  }
  clear() {
    this.store.clear();
  }
  getItem(key: string) {
    return this.store.has(key) ? this.store.get(key)! : null;
  }
  key(index: number) {
    return Array.from(this.store.keys())[index] ?? null;
  }
  removeItem(key: string) {
    this.store.delete(key);
  }
  setItem(key: string, value: string) {
    this.store.set(key, value);
  }
}

function clockFrom(...isoTimes: string[]): () => Date {
  const queue = [...isoTimes];
  return () => new Date(queue.shift() ?? isoTimes[isoTimes.length - 1]);
}

describe("lifecycleReceipts", () => {
  let storage: MemoryStorage;

  beforeEach(() => {
    storage = new MemoryStorage();
  });

  it("starts empty", () => {
    expect(readLifecycleReceipts(storage)).toEqual([]);
    expect(lifecycleReceiptKinds([])).toEqual([]);
  });

  it("records a timestamped milestone with optional detail", () => {
    const entries = recordLifecycleReceipt({
      kind: "credential.stored",
      detail: { provider: "grok", apiKeyPresent: true },
      storage,
      now: clockFrom("2026-06-11T10:00:00.000Z"),
    });

    expect(entries).toHaveLength(1);
    expect(entries[0]).toEqual({
      kind: "credential.stored",
      recordedAt: "2026-06-11T10:00:00.000Z",
      detail: { provider: "grok", apiKeyPresent: true },
    });
    // Persisted to the durable key, not held only in memory.
    expect(readLifecycleReceipts(storage)).toEqual(entries);
  });

  it("is append-only — re-running a step logs a fresh dated milestone", () => {
    recordLifecycleReceipt({ kind: "posture.set", detail: { posture: "work" }, storage, now: clockFrom("2026-06-11T10:00:00.000Z") });
    const entries = recordLifecycleReceipt({
      kind: "posture.set",
      detail: { posture: "private" },
      storage,
      now: clockFrom("2026-06-11T11:00:00.000Z"),
    });

    expect(entries).toHaveLength(2);
    // Collapsed kinds dedupe for the linear-vs-hub decision.
    expect(lifecycleReceiptKinds(entries)).toEqual(["posture.set"]);
    // The latest-by-kind view reflects the most recent re-run.
    expect(latestLifecycleReceiptByKind(entries)["posture.set"].detail).toEqual({ posture: "private" });
  });

  it("preserves first-seen order across distinct kinds", () => {
    recordLifecycleReceipt({ kind: "credential.stored", storage });
    recordLifecycleReceipt({ kind: "posture.set", storage });
    recordLifecycleReceipt({ kind: "placement.set", storage });
    recordLifecycleReceipt({ kind: "onboarding.completed", storage });

    expect(lifecycleReceiptKinds(readLifecycleReceipts(storage))).toEqual([
      "credential.stored",
      "posture.set",
      "placement.set",
      "onboarding.completed",
    ]);
  });

  it("caps the log to maxEntries, keeping the most recent", () => {
    for (let i = 0; i < 5; i += 1) {
      recordLifecycleReceipt({ kind: `event.${i}`, storage, maxEntries: 3 });
    }
    const kinds = readLifecycleReceipts(storage).map((entry) => entry.kind);
    expect(kinds).toEqual(["event.2", "event.3", "event.4"]);
  });

  it("ignores malformed persisted data", () => {
    storage.setItem(LIFECYCLE_RECEIPT_STORAGE_KEY, "{not json");
    expect(readLifecycleReceipts(storage)).toEqual([]);

    storage.setItem(LIFECYCLE_RECEIPT_STORAGE_KEY, JSON.stringify([{ kind: "ok", recordedAt: "2026-06-11T00:00:00Z" }, { bogus: true }]));
    expect(readLifecycleReceipts(storage).map((entry) => entry.kind)).toEqual(["ok"]);
  });
});
