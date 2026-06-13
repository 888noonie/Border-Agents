import { afterEach, beforeEach, describe, expect, test } from "vitest";
import { existsSync, mkdtempSync, readFileSync, readdirSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { handleActionRequest } from "../../src/soulActions";
import { readReceiptLedger } from "../../src/receiptLedger";
import { createLiveRepoEditExecutor, liveExecutorSandbox } from "../liveEffectorExecutors";
import type { BuddySettings } from "../../src/buddyProfiles";
import type { SessionChatLine } from "../../src/liveGovernance";

// The full membrane on REAL disk: gate authorizes (intent-level) → the live executor performs
// the file write → ExecutionReceipt. This is the "harm demo" as an assertion — a safe sandbox
// write actually lands, while a protected target leaves the filesystem untouched.

const ACTING: BuddySettings = {
  enabled: true,
  provider: "grok",
  modelLabel: "Grok subscription",
  connectionLabel: "Connected",
  allowAction: true, // agent_action (high risk) — needs trusted action-backing + confirmation
  allowExternalShare: false,
  memoryMode: "purpose_graded",
};

// An assistant line grades `trusted` and (with allowAction) carries may_use_for_action — it backs the action.
const ACTION_HISTORY: SessionChatLine[] = [{ role: "assistant", text: "Applied the reviewed patch." }];

function memoryStorage(): Storage {
  const map = new Map<string, string>();
  return {
    get length() {
      return map.size;
    },
    clear: () => map.clear(),
    getItem: (k) => (map.has(k) ? map.get(k)! : null),
    key: (i) => Array.from(map.keys())[i] ?? null,
    removeItem: (k) => void map.delete(k),
    setItem: (k, v) => void map.set(k, String(v)),
  };
}

function repoIntent(path: string, operation = "write_patch") {
  return {
    effectorId: "repo_edit" as const,
    operation,
    target: { kind: "repo_path" as const, path },
    payloadDigest: "sha256:patch",
    summary: `${operation} ${path}`,
  };
}

describe("live repo_edit executor — the membrane on real disk", () => {
  let root: string;

  beforeEach(() => {
    root = mkdtempSync(join(tmpdir(), "border-harm-"));
  });

  afterEach(() => {
    rmSync(root, { recursive: true, force: true });
  });

  test("a backed+confirmed safe target actually writes a file under the sandbox", () => {
    const storage = memoryStorage();
    const executors = { repo_edit: createLiveRepoEditExecutor({ root }) };
    const target = ".border-agents/proofs/first-act.patch";

    const done = handleActionRequest({
      buddy: "forge",
      effectorId: "repo_edit",
      settings: ACTING,
      posture: "work",
      history: ACTION_HISTORY,
      intent: repoIntent(target),
      executors,
      confirmed: true,
      storage,
      now: "2026-06-13T12:00:05Z",
    });

    expect(done.receipt.decision).toBe("allow");
    expect(done.execution?.executor_called).toBe(true);
    expect(done.execution?.outcome).toBe("ok");

    // The file is really on disk, and its content cites the authorizing ActionReceipt.
    const written = join(root, target);
    expect(existsSync(written)).toBe(true);
    expect(readFileSync(written, "utf8")).toContain(done.receipt.receipt_id);

    // Ledger records authorization THEN execution.
    const ledger = readReceiptLedger(storage);
    expect(ledger.some((e) => e.kind === "execution")).toBe(true);
  });

  test("a protected target is blocked and leaves the filesystem completely untouched", () => {
    const storage = memoryStorage();
    const executors = { repo_edit: createLiveRepoEditExecutor({ root }) };

    const done = handleActionRequest({
      buddy: "forge",
      effectorId: "repo_edit",
      settings: ACTING,
      posture: "work",
      history: ACTION_HISTORY,
      intent: repoIntent("AGENTS.md", "apply_patch"),
      executors,
      confirmed: true, // confirmation can NEVER clear a hard block
      storage,
      now: "2026-06-13T12:00:05Z",
    });

    expect(done.receipt.decision).toBe("blocked");
    expect(done.receipt.rules.some((r) => r.policy_rule === "action.blocked.protected_target")).toBe(true);
    expect(done.execution).toBeUndefined();

    // Nothing was created anywhere under the demo root — no sandbox dir, no stray AGENTS.md.
    expect(existsSync(join(root, "AGENTS.md"))).toBe(false);
    expect(existsSync(liveExecutorSandbox(root))).toBe(false);
    expect(readdirSync(root)).toHaveLength(0);
  });

  test("even if allowed, a non-sandbox target is refused by the executor's own guard (nothing written)", () => {
    const storage = memoryStorage();
    const executors = { repo_edit: createLiveRepoEditExecutor({ root }) };
    // Not protected, but outside the sandbox — the gate allows it, the executor refuses (skipped).
    const target = "src/components/widget.tsx";

    const done = handleActionRequest({
      buddy: "forge",
      effectorId: "repo_edit",
      settings: ACTING,
      posture: "work",
      history: ACTION_HISTORY,
      intent: repoIntent(target),
      executors,
      confirmed: true,
      storage,
      now: "2026-06-13T12:00:05Z",
    });

    expect(done.receipt.decision).toBe("allow");
    expect(done.execution?.outcome).toBe("skipped");
    expect(existsSync(join(root, target))).toBe(false);
    expect(readdirSync(root)).toHaveLength(0);
  });
});
