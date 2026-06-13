// Harm demo — `npm run demo:harm`. The skeptic-converter: a buddy's act-effector (repo_edit)
// actually writes to disk, and you watch the SAME effector be refused, permanently, when it
// aims at a protected target. The gate is the only thing standing between the two outcomes.
//
// Everything here runs the REAL membrane (handleActionRequest → action gate → live executor):
// no mock decisions. The only thing wired for the demo is a Node executor that writes to disk
// (scripts/liveEffectorExecutors.ts) and an in-memory ledger.

import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";

import { handleActionRequest } from "../src/soulActions";
import { createLiveRepoEditExecutor, liveExecutorSandbox } from "./liveEffectorExecutors";
import type { BuddySettings } from "../src/buddyProfiles";
import type { ActionIntent } from "../src/core";
import type { SessionChatLine } from "../src/liveGovernance";

const ROOT = process.cwd();
const executors = { repo_edit: createLiveRepoEditExecutor({ root: ROOT }) };

// Forge, acting (high-risk agent_action), with a trusted assistant memory that carries
// may_use_for_action — the action-backing the gate requires before any write is even eligible.
const SETTINGS: BuddySettings = {
  enabled: true,
  provider: "grok",
  modelLabel: "Grok subscription",
  connectionLabel: "Connected",
  allowAction: true,
  allowExternalShare: false,
  memoryMode: "purpose_graded",
};
const HISTORY: SessionChatLine[] = [{ role: "assistant", text: "Applied the reviewed patch." }];

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

function intent(path: string, operation = "write_patch"): ActionIntent {
  return {
    effectorId: "repo_edit",
    operation,
    target: { kind: "repo_path", path },
    payloadDigest: "sha256:demo",
    summary: `${operation} ${path}`,
  };
}

const storage = memoryStorage();
const line = (s = "") => console.log(s);
const rule = (decision: string, policy?: string) => `${decision}${policy ? ` (${policy})` : ""}`;

function attempt(label: string, target: string, confirmed: boolean) {
  const result = handleActionRequest({
    buddy: "forge",
    effectorId: "repo_edit",
    settings: SETTINGS,
    posture: "work",
    history: HISTORY,
    intent: intent(target),
    executors,
    confirmed,
    storage,
    now: new Date().toISOString(),
  });
  const lastRule = result.receipt.rules[result.receipt.rules.length - 1];
  line(`  ${label}`);
  line(`    decision:  ${rule(result.receipt.decision, lastRule?.policy_rule)}`);
  if (result.execution) {
    line(`    execution: ${result.execution.outcome} — ${result.execution.detail ?? ""}`);
  } else {
    line(`    execution: (none — executor was never reached)`);
  }
  return result;
}

line();
line("Border Agents — harm demo");
line("Forge wants to write files. The action gate decides which writes are allowed to happen.");
line();

// 1) A protected target — blocked even WITH confirmation. A hard block is not confirm-able.
line("1) Forge tries to edit a PROTECTED target (AGENTS.md), already confirmed:");
const blocked = attempt("repo_edit AGENTS.md", "AGENTS.md", true);
line(`    on disk:   nothing written — the executor is unreachable on a block: ${blocked.execution === undefined ? "confirmed" : "NO — INVARIANT BROKEN"}`);
line();

// 2) A safe sandbox target — needs confirmation first, then actually writes.
const safe = ".border-agents/proofs/first-act.patch";
line(`2) Forge proposes a SAFE write (${safe}):`);
attempt("repo_edit (proposed)", safe, false);
line("   ...user confirms...");
const done = attempt("repo_edit (confirmed)", safe, true);
const writtenPath = join(ROOT, safe);
if (done.execution?.outcome === "ok" && existsSync(writtenPath)) {
  line(`    on disk:   wrote ${safe}`);
  line();
  line("    --- file contents ---");
  for (const l of readFileSync(writtenPath, "utf8").trimEnd().split("\n")) {
    line(`    | ${l}`);
  }
} else {
  line(`    on disk:   expected a file at ${safe} but found none`);
}
line();
line(`Sandbox: ${liveExecutorSandbox(ROOT)} (the ONLY place repo_edit can write).`);
line("Buddies persist. Providers rotate. The human remains sovereign — and the gate, not the buddy, decides.");
line();
