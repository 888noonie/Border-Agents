import { describe, expect, test } from "vitest";
import {
  handleActionRequest,
  parseActionCommand,
  presenceIntentToActionIntent,
  decisionEmotion,
  decisionAlertLevel,
  routeHealthFromSoul,
} from "../soulActions";
import { readReceiptLedger } from "../receiptLedger";
import type { BuddySettings } from "../buddyProfiles";
import type { SessionChatLine } from "../liveGovernance";

const BASE_SETTINGS: BuddySettings = {
  enabled: true,
  provider: "grok",
  modelLabel: "Grok subscription",
  connectionLabel: "Connected",
  allowAction: false,
  allowExternalShare: false,
  memoryMode: "purpose_graded",
};

// An assistant line grades `trusted` (it may_assert) and, with allowAction on, carries
// may_use_for_action — so it backs a high-risk action. A user line would grade `limited`.
const ACTION_HISTORY: SessionChatLine[] = [{ role: "assistant", text: "Applied the reviewed patch." }];

/** Minimal synchronous in-memory Storage so the handler tests stay pure (no jsdom). */
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

describe("handleActionRequest", () => {
  test("blocks an unknown effector without throwing, and appends a receipt", () => {
    const storage = memoryStorage();
    const { receipt, result } = handleActionRequest({
      buddy: "veritas",
      effectorId: "definitely_not_an_effector",
      settings: BASE_SETTINGS,
      posture: "work",
      history: ACTION_HISTORY,
      storage,
      now: "2026-06-13T12:00:00Z",
    });

    expect(receipt.decision).toBe("blocked");
    expect(receipt.rules.some((r) => r.policy_rule === "action.blocked.unknown_effector")).toBe(true);
    expect(result.decision).toBe("blocked");
    expect(result.receiptId).toBe(receipt.receipt_id);
    expect(readReceiptLedger(storage)).toHaveLength(1);
  });

  test("the action_result cue carries alertLevel as the chrome twin of its decision", () => {
    const storage = memoryStorage();
    // blocked path — unknown effector → decision "blocked" → alertLevel "blocked"
    const blocked = handleActionRequest({
      buddy: "veritas",
      effectorId: "definitely_not_an_effector",
      settings: BASE_SETTINGS,
      posture: "work",
      history: ACTION_HISTORY,
      storage,
      now: "2026-06-13T12:00:00Z",
    });
    expect(blocked.result.decision).toBe("blocked");
    expect(blocked.result.alertLevel).toBe("blocked");

    // allow path — low-risk receipt_review under play → decision "allow" → alertLevel "ready"
    const allowed = handleActionRequest({
      buddy: "veritas",
      effectorId: "receipt_review",
      settings: BASE_SETTINGS,
      posture: "play",
      history: [{ role: "user", text: "What changed today?" }],
      storage,
      now: "2026-06-13T12:00:01Z",
    });
    expect(allowed.result.decision).toBe("allow");
    expect(allowed.result.alertLevel).toBe("ready");
  });

  test("low-risk receipt_review under play posture is allowed and recorded", () => {
    const storage = memoryStorage();
    const { receipt, result } = handleActionRequest({
      buddy: "veritas",
      effectorId: "receipt_review",
      settings: BASE_SETTINGS, // allowAction false → summarize_history (low risk)
      posture: "play",
      history: [{ role: "user", text: "What changed today?" }],
      requestId: "req-7",
      storage,
      now: "2026-06-13T12:00:00Z",
    });

    expect(receipt.decision).toBe("allow");
    expect(result.decision).toBe("allow");
    expect(result.requestId).toBe("req-7");
    const ledger = readReceiptLedger(storage);
    expect(ledger).toHaveLength(1);
    expect(ledger[0].kind).toBe("action");
  });

  test("memory-off, action-enabled request fails closed (high risk, no backing)", () => {
    const storage = memoryStorage();
    const { receipt } = handleActionRequest({
      buddy: "veritas",
      effectorId: "receipt_review",
      settings: { ...BASE_SETTINGS, allowAction: true, memoryMode: "off" },
      posture: "work",
      history: ACTION_HISTORY,
      storage,
      now: "2026-06-13T12:00:00Z",
    });

    expect(receipt.decision).toBe("blocked");
    expect(receipt.rules.some((r) => r.policy_rule === "action.blocked.no_action_grant")).toBe(true);
  });

  test("confirm round-trip records two receipts: needs_confirmation then allow", () => {
    const storage = memoryStorage();
    const settings: BuddySettings = { ...BASE_SETTINGS, allowAction: true }; // agent_action (high)

    const first = handleActionRequest({
      buddy: "veritas",
      effectorId: "receipt_review",
      settings,
      posture: "work",
      history: ACTION_HISTORY,
      requestId: "req-1",
      storage,
      now: "2026-06-13T12:00:00Z",
    });
    expect(first.receipt.decision).toBe("needs_confirmation");

    const second = handleActionRequest({
      buddy: "veritas",
      effectorId: "receipt_review",
      settings,
      posture: "work",
      history: ACTION_HISTORY,
      confirmed: true,
      requestId: "req-1",
      storage,
      now: "2026-06-13T12:00:05Z", // later — distinct receipt id, so both persist
    });
    expect(second.receipt.decision).toBe("allow");
    expect(second.receipt.confirmed).toBe(true);

    expect(readReceiptLedger(storage)).toHaveLength(2);
  });

  // Regression: the dock/body addresses buddies by persona id ("owl"), but grants are keyed
  // by governance id ("veritas"). The request must resolve to the governance identity, not
  // fall through to a blocked/ungranted receipt. The result cue stays addressed to the persona.
  test("a persona-id request resolves to the governance identity (owl → veritas)", () => {
    const storage = memoryStorage();
    const { receipt, result } = handleActionRequest({
      buddy: "owl",
      effectorId: "receipt_review",
      settings: BASE_SETTINGS,
      posture: "work",
      history: [{ role: "user", text: "What changed today?" }],
      storage,
      now: "2026-06-13T12:00:00Z",
    });

    // Authorized (not ungranted) — under work, low-risk reach hits the confirmation floor.
    expect(receipt.decision).toBe("needs_confirmation");
    expect(receipt.rules.some((r) => r.policy_rule === "action.blocked.ungranted")).toBe(false);
    // Receipt/ledger carry the governance identity; the cue is addressed back to the persona.
    expect(receipt.buddy).toBe("veritas");
    expect(result.buddy).toBe("owl");
    const ledger = readReceiptLedger(storage);
    expect(ledger).toHaveLength(1);
    expect(ledger[0].buddyId).toBe("veritas");
  });

  test("aether can attach local_chat under private posture without an ungranted block", () => {
    const storage = memoryStorage();
    const { receipt } = handleActionRequest({
      buddy: "aether",
      effectorId: "local_chat",
      settings: BASE_SETTINGS,
      posture: "private",
      history: [{ role: "user", text: "hello local model" }],
      intent: {
        effectorId: "local_chat",
        operation: "open_session",
        target: { kind: "url", path: "LM Studio" },
        summary: "open private local chat",
      },
      route: { provider: "lm_studio", locality: "local", downgraded: false },
      storage,
      now: "2026-06-13T12:00:00Z",
    });

    expect(receipt.decision).toBe("needs_confirmation");
    expect(receipt.rules.some((r) => r.policy_rule === "action.blocked.ungranted")).toBe(false);
    expect(receipt.rules.some((r) => r.policy_rule === "action.confirm.risk_floor")).toBe(true);
  });

  // Commandeer is the first window-driving act effector. Its governance is load-bearing:
  // keystroke injection has no OS consent, so the gate is the only safety boundary. These
  // lock the contract — granted to forge only, act-floored to confirm, allowed on confirm,
  // and producing an execution receipt (the world-effect runs in the driver).
  test("commandeer is granted to forge and confirms before it may run (act floor)", () => {
    const storage = memoryStorage();
    const intent = {
      effectorId: "commandeer" as const,
      operation: "control",
      target: { kind: "command" as const, path: "win-42" },
      summary: "control win-42",
    };
    const { receipt } = handleActionRequest({
      buddy: "crab", // forge's persona id — resolves to the governance grant
      effectorId: "commandeer",
      settings: { ...BASE_SETTINGS, allowAction: true },
      posture: "work",
      history: ACTION_HISTORY,
      intent,
      storage,
      now: "2026-06-26T12:00:00Z",
    });
    expect(receipt.buddy).toBe("forge");
    expect(receipt.decision).toBe("needs_confirmation");
    expect(receipt.rules.some((r) => r.policy_rule === "action.blocked.ungranted")).toBe(false);
    expect(receipt.rules.some((r) => r.policy_rule === "action.confirm.risk_floor")).toBe(true);
  });

  test("a confirmed commandeer allows and emits an execution receipt", () => {
    const storage = memoryStorage();
    const intent = {
      effectorId: "commandeer" as const,
      operation: "control",
      target: { kind: "command" as const, path: "win-42" },
      summary: "control win-42",
    };
    const { receipt, result, execution } = handleActionRequest({
      buddy: "forge",
      effectorId: "commandeer",
      settings: { ...BASE_SETTINGS, allowAction: true },
      posture: "work",
      history: ACTION_HISTORY,
      intent,
      confirmed: true,
      executors: { commandeer: () => ({ outcome: "ok", detail: "cue dispatched" }) },
      storage,
      now: "2026-06-26T12:00:05Z",
    });
    expect(receipt.decision).toBe("allow");
    expect(execution?.outcome).toBe("ok");
    expect(result.outcome?.executed).toBe(true);
    // Authorization receipt + execution receipt both land in the ledger.
    expect(readReceiptLedger(storage)).toHaveLength(2);
  });

  test("commandeer is blocked for a buddy that wasn't granted it", () => {
    const storage = memoryStorage();
    const { receipt } = handleActionRequest({
      buddy: "aether", // not granted commandeer
      effectorId: "commandeer",
      settings: { ...BASE_SETTINGS, allowAction: true },
      posture: "work",
      history: ACTION_HISTORY,
      intent: {
        effectorId: "commandeer",
        operation: "pin",
        target: { kind: "command", path: "win-1" },
        summary: "pin win-1",
      },
      storage,
      now: "2026-06-26T12:00:00Z",
    });
    expect(receipt.decision).toBe("blocked");
    expect(receipt.rules.some((r) => r.policy_rule === "action.blocked.ungranted")).toBe(true);
  });

  test("aether placeholder surfaces can block as known-but-unwired effectors", () => {
    const storage = memoryStorage();
    const { receipt } = handleActionRequest({
      buddy: "aether",
      effectorId: "summarize_long",
      settings: BASE_SETTINGS,
      posture: "work",
      history: [{ role: "user", text: "open the placeholder" }],
      storage,
      now: "2026-06-13T12:00:00Z",
    });

    expect(receipt.decision).toBe("blocked");
    expect(receipt.rules.some((r) => r.policy_rule === "action.blocked.unwired")).toBe(true);
    expect(receipt.rules.some((r) => r.policy_rule === "action.blocked.unknown_effector")).toBe(false);
    expect(receipt.rules.some((r) => r.policy_rule === "action.blocked.ungranted")).toBe(false);
  });
});

// Launcher reach effectors (Slice 0): open a tool the user already has, through the gate.
// They are low-risk reach, so the confirmation floor asks once under work posture, then the
// executor opens the tool on the confirmed allow — proving the launch path runs the executor.
describe("launcher reach effectors", () => {
  /** The launcher intent the soul synthesises: open the workspace, file_path (never a repo_path,
   * so the protected-target block can't apply), low-risk reach. */
  function launcherIntent(effectorId: "open_cursor" | "open_vscode" | "open_terminal", root = "/work/space") {
    return {
      effectorId,
      operation: "open",
      target: { kind: "file_path" as const, path: root },
      summary: `open ${root} in ${effectorId}`,
    };
  }

  test("forge launching open_cursor asks once under work, then opens on confirm", () => {
    const storage = memoryStorage();
    const opened: string[] = [];
    const executors = {
      open_cursor: (ctx: { intent: { target: { path: string } } }) => {
        opened.push(ctx.intent.target.path);
        return { outcome: "ok" as const, detail: "opened" };
      },
    };

    // First request (unconfirmed): granted + wired, but low-risk reach hits the work-posture
    // confirmation floor — needs_confirmation, and crucially NOT blocked and NOT yet executed.
    const first = handleActionRequest({
      buddy: "forge",
      effectorId: "open_cursor",
      settings: BASE_SETTINGS,
      posture: "work",
      history: [],
      intent: launcherIntent("open_cursor"),
      executors,
      storage,
      now: "2026-06-25T12:00:00Z",
    });
    expect(first.receipt.decision).toBe("needs_confirmation");
    expect(first.receipt.rules.some((r) => r.policy_rule === "action.blocked.ungranted")).toBe(false);
    expect(first.execution).toBeUndefined();
    expect(opened).toEqual([]);

    // Confirmed: allow, and the executor opens the tool exactly once.
    const second = handleActionRequest({
      buddy: "forge",
      effectorId: "open_cursor",
      settings: BASE_SETTINGS,
      posture: "work",
      history: [],
      intent: launcherIntent("open_cursor"),
      executors,
      confirmed: true,
      storage,
      now: "2026-06-25T12:00:01Z",
    });
    expect(second.receipt.decision).toBe("allow");
    expect(second.receipt.risk).toBe("low");
    expect(second.execution?.outcome).toBe("ok");
    expect(opened).toEqual(["/work/space"]);
  });

  test("a launcher whose CLI is missing surfaces the error, not a false 'Running'", () => {
    const storage = memoryStorage();
    const executors = {
      open_cursor: () => ({ outcome: "error" as const, detail: "cursor not found on PATH" }),
    };
    const { receipt, result } = handleActionRequest({
      buddy: "forge",
      effectorId: "open_cursor",
      settings: BASE_SETTINGS,
      posture: "work",
      history: [],
      intent: launcherIntent("open_cursor"),
      executors,
      confirmed: true,
      storage,
      now: "2026-06-25T12:00:02Z",
    });
    expect(receipt.decision).toBe("allow");
    expect(result.summary).toContain("didn't run");
    expect(result.summary).toContain("cursor not found on PATH");
    expect(result.summary).not.toContain("Running");
  });

  test("a launcher is never blocked as an unbacked action — reach needs no action grant", () => {
    const storage = memoryStorage();
    const { receipt } = handleActionRequest({
      buddy: "forge",
      effectorId: "open_vscode",
      settings: BASE_SETTINGS, // allowAction false: a reach effector needs no may_use_for_action
      posture: "work",
      history: [],
      intent: launcherIntent("open_vscode"),
      storage,
      now: "2026-06-25T12:00:00Z",
    });
    expect(receipt.rules.some((r) => r.policy_rule === "action.blocked.no_action_grant")).toBe(false);
    expect(receipt.rules.some((r) => r.policy_rule === "action.blocked.unwired")).toBe(false);
    expect(receipt.decision).toBe("needs_confirmation");
  });
});

// The first true `act` effector through the full seam: body request → resolve → gate
// (intent-level) → executor (only on allow) → ledger. Proves the membrane end-to-end,
// not just the core gate — the layer the owl→veritas seam slipped through.
describe("repo_edit through the execution membrane", () => {
  const ACTING: BuddySettings = { ...BASE_SETTINGS, allowAction: true }; // agent_action (high risk)

  function repoIntent(path: string, operation = "write_patch") {
    return {
      effectorId: "repo_edit" as const,
      operation,
      target: { kind: "repo_path" as const, path },
      payloadDigest: "sha256:patch",
      summary: `${operation} ${path}`,
    };
  }

  // Case A — blocked before confirmation: a safe target but no action-backed memory.
  test("Case A: safe target with memory off is blocked (no_action_grant), executor never runs", () => {
    const storage = memoryStorage();
    const { receipt, execution } = handleActionRequest({
      buddy: "forge",
      effectorId: "repo_edit",
      settings: { ...ACTING, memoryMode: "off" },
      posture: "work",
      history: ACTION_HISTORY,
      intent: repoIntent(".border-agents/proofs/first-act.patch"),
      storage,
      now: "2026-06-13T12:00:00Z",
    });
    expect(receipt.decision).toBe("blocked");
    expect(receipt.rules.some((r) => r.policy_rule === "action.blocked.no_action_grant")).toBe(true);
    expect(execution).toBeUndefined();
    expect(readReceiptLedger(storage).some((e) => e.kind === "execution")).toBe(false);
  });

  // Case B — blocked EVEN AFTER confirmation: a protected target is a hard block.
  test("Case B: protected target (AGENTS.md) is blocked even when backed and confirmed; nothing executes", () => {
    const storage = memoryStorage();
    const { receipt, execution } = handleActionRequest({
      buddy: "forge",
      effectorId: "repo_edit",
      settings: ACTING,
      posture: "work",
      history: ACTION_HISTORY,
      intent: repoIntent("AGENTS.md", "apply_patch"),
      confirmed: true,
      storage,
      now: "2026-06-13T12:00:00Z",
    });
    expect(receipt.decision).toBe("blocked");
    expect(receipt.rules.some((r) => r.policy_rule === "action.blocked.protected_target")).toBe(true);
    expect(execution).toBeUndefined();
    expect(readReceiptLedger(storage).some((e) => e.kind === "execution")).toBe(false);
  });

  // Case C — allowed on a safe target, with backing + confirmation; executor runs once,
  // and the ledger records the ActionReceipt BEFORE the ExecutionReceipt.
  test("Case C: safe target needs confirmation, then on confirm the executor runs and an ExecutionReceipt lands after the ActionReceipt", () => {
    const storage = memoryStorage();
    const intent = repoIntent(".border-agents/proofs/first-act.patch");

    const pending = handleActionRequest({
      buddy: "forge",
      effectorId: "repo_edit",
      settings: ACTING,
      posture: "work",
      history: ACTION_HISTORY,
      intent,
      storage,
      now: "2026-06-13T12:00:00Z",
    });
    expect(pending.receipt.decision).toBe("needs_confirmation");
    expect(pending.execution).toBeUndefined();

    const done = handleActionRequest({
      buddy: "forge",
      effectorId: "repo_edit",
      settings: ACTING,
      posture: "work",
      history: ACTION_HISTORY,
      intent,
      confirmed: true,
      storage,
      now: "2026-06-13T12:00:05Z",
    });
    expect(done.receipt.decision).toBe("allow");
    expect(done.execution?.executor_called).toBe(true);
    expect(done.execution?.outcome).toBe("ok");
    // Route provenance rides on the execution receipt — "buddies persist, providers rotate".
    expect(done.execution?.route.provider).toBe("claude"); // forge's preferred route
    expect(done.execution?.action_receipt_id).toBe(done.receipt.receipt_id);

    const ledger = readReceiptLedger(storage);
    const actionIdx = ledger.findIndex((e) => e.kind === "action" && e.recordedAt === "2026-06-13T12:00:05Z");
    const execIdx = ledger.findIndex((e) => e.kind === "execution");
    expect(actionIdx).toBeGreaterThanOrEqual(0);
    expect(execIdx).toBeGreaterThan(actionIdx); // authorized, THEN executed
  });

  // The executor's own sandbox guard is independent of the gate: even if the gate allowed a
  // non-sandbox target, the default executor refuses to act on it (defense in depth).
  test("executor refuses a non-sandbox target even when allowed (skipped, not ok)", () => {
    const storage = memoryStorage();
    const { receipt, execution } = handleActionRequest({
      buddy: "forge",
      effectorId: "repo_edit",
      settings: ACTING,
      posture: "work",
      history: ACTION_HISTORY,
      intent: repoIntent("src/components/widget.tsx"), // not protected, but outside the sandbox
      confirmed: true,
      storage,
      now: "2026-06-13T12:00:00Z",
    });
    expect(receipt.decision).toBe("allow");
    expect(execution?.executor_called).toBe(true);
    expect(execution?.outcome).toBe("skipped");
  });

  // The act path must also resolve persona ids — the same seam that bit receipt_review.
  test("a persona-id (crab) repo_edit resolves to forge for grant + audit identity", () => {
    const storage = memoryStorage();
    const { receipt, result } = handleActionRequest({
      buddy: "crab",
      effectorId: "repo_edit",
      settings: ACTING,
      posture: "work",
      history: ACTION_HISTORY,
      intent: repoIntent(".border-agents/proofs/first-act.patch"),
      storage,
      now: "2026-06-13T12:00:00Z",
    });
    expect(receipt.rules.some((r) => r.policy_rule === "action.blocked.ungranted")).toBe(false);
    expect(receipt.buddy).toBe("forge");
    expect(result.buddy).toBe("crab");
  });
});

describe("parseActionCommand", () => {
  test("/review defaults to receipt_review", () => {
    expect(parseActionCommand("/review")).toEqual({ kind: "review", effectorId: "receipt_review" });
    expect(parseActionCommand("  /review  ")).toEqual({ kind: "review", effectorId: "receipt_review" });
  });

  test("/review <effector> carries the named effector", () => {
    expect(parseActionCommand("/review terminal")).toEqual({ kind: "review", effectorId: "terminal" });
  });

  test("/review <effector> <target> carries a typed target", () => {
    expect(parseActionCommand("/review repo_edit scratch.md")).toEqual({
      kind: "review",
      effectorId: "repo_edit",
      target: "scratch.md",
    });
    expect(parseActionCommand("/review repo_edit .border-agents/proofs/x.patch")).toEqual({
      kind: "review",
      effectorId: "repo_edit",
      target: ".border-agents/proofs/x.patch",
    });
    // No target → no target field (back-compat with the effector-only form).
    expect(parseActionCommand("/review repo_edit")).toEqual({ kind: "review", effectorId: "repo_edit" });
  });

  test("/confirm is the confirm command", () => {
    expect(parseActionCommand("/confirm")).toEqual({ kind: "confirm" });
  });

  test("free text and near-misses are not action commands", () => {
    expect(parseActionCommand("hello there")).toBeNull();
    expect(parseActionCommand("/reviewer")).toBeNull(); // must be /review or "/review "
    expect(parseActionCommand("/confirming")).toBeNull();
    expect(parseActionCommand("")).toBeNull();
  });
});

describe("presenceIntentToActionIntent", () => {
  test("lifts a concrete repo_path intent into a core ActionIntent", () => {
    const intent = presenceIntentToActionIntent("repo_edit", {
      operation: "write_patch",
      target: { kind: "repo_path", value: ".border-agents/proofs/notes.md" },
      summary: "write notes.md",
    });
    expect(intent).toEqual({
      effectorId: "repo_edit",
      operation: "write_patch",
      target: { kind: "repo_path", path: ".border-agents/proofs/notes.md" },
      summary: "write notes.md",
    });
  });

  test("synthesizes a summary when the body omits one", () => {
    const intent = presenceIntentToActionIntent("repo_edit", {
      operation: "write_patch",
      target: { kind: "repo_path", value: "x.md" },
    });
    expect(intent?.summary).toBe("write_patch x.md");
  });

  test("carries an optional payloadDigest through verbatim", () => {
    const intent = presenceIntentToActionIntent("repo_edit", {
      operation: "apply_patch",
      target: { kind: "repo_path", value: "x.md" },
      payloadDigest: "sha256:abc",
    });
    expect(intent?.payloadDigest).toBe("sha256:abc");
  });

  test("a none / value-less / absent target degrades to undefined (grant-only, fails closed)", () => {
    expect(presenceIntentToActionIntent("repo_edit", undefined)).toBeUndefined();
    expect(
      presenceIntentToActionIntent("repo_edit", { operation: "noop", target: { kind: "none" } }),
    ).toBeUndefined();
    expect(
      presenceIntentToActionIntent("repo_edit", { operation: "write_patch", target: { kind: "repo_path" } }),
    ).toBeUndefined();
    expect(presenceIntentToActionIntent("repo_edit", { operation: "write_patch" })).toBeUndefined();
  });
});

describe("decisionEmotion", () => {
  test("each gate decision maps to a distinct, honest face", () => {
    expect(decisionEmotion("allow")).toBe("happy");
    expect(decisionEmotion("needs_confirmation")).toBe("curious");
    expect(decisionEmotion("blocked")).toBe("alert");
    // distinctness — a glance must tell the three border outcomes apart
    const faces = new Set(["allow", "needs_confirmation", "blocked"].map(decisionEmotion));
    expect(faces.size).toBe(3);
  });

  test("any unknown decision fails loud (alert), never a reassuring face", () => {
    expect(decisionEmotion("garbage")).toBe("alert");
    expect(decisionEmotion("")).toBe("alert");
  });

  test("stays in lockstep with the native body's for_decision twin", () => {
    // These pairings ARE the cross-surface contract (desktop-body render.rs Emotion::for_decision):
    // if either side drifts, the body and soul would disagree on what the gate just did.
    expect(decisionEmotion("allow")).toBe("happy"); //              → Emotion::Happy
    expect(decisionEmotion("needs_confirmation")).toBe("curious"); // → Emotion::Curious
    expect(decisionEmotion("blocked")).toBe("alert"); //            → Emotion::Alert
  });
});

describe("decisionAlertLevel", () => {
  test("each gate decision maps to its passport/ring alert tier", () => {
    expect(decisionAlertLevel("allow")).toBe("ready");
    expect(decisionAlertLevel("needs_confirmation")).toBe("confirm");
    expect(decisionAlertLevel("blocked")).toBe("blocked");
  });

  test("any unknown decision fails loud at the top tier (critical), never quiet", () => {
    expect(decisionAlertLevel("garbage")).toBe("critical");
    expect(decisionAlertLevel("")).toBe("critical");
  });

  test("face and chrome derive from one decision (Law 7 — body never infers policy)", () => {
    // The emotion twin colours the face; the alert twin colours the passport/ring. They are
    // sent together on action_result so the body reads chrome from a cue, not from the face.
    expect([decisionEmotion("allow"), decisionAlertLevel("allow")]).toEqual(["happy", "ready"]);
    expect([decisionEmotion("needs_confirmation"), decisionAlertLevel("needs_confirmation")]).toEqual([
      "curious",
      "confirm",
    ]);
    expect([decisionEmotion("blocked"), decisionAlertLevel("blocked")]).toEqual(["alert", "blocked"]);
  });
});

describe("routeHealthFromSoul", () => {
  test("maps the ready path from real route state", () => {
    expect(
      routeHealthFromSoul({
        hasRoute: true,
        availability: "available",
        downgraded: false,
      }),
    ).toBe("ready");
  });

  test("treats gated as reachable route infrastructure", () => {
    expect(
      routeHealthFromSoul({
        hasRoute: true,
        availability: "gated",
        downgraded: false,
        lastOutcome: "ok",
      }),
    ).toBe("ready");
  });

  test("marks missing or unwired routes unavailable", () => {
    expect(
      routeHealthFromSoul({
        hasRoute: false,
        availability: "available",
        downgraded: false,
      }),
    ).toBe("unavailable");
    expect(
      routeHealthFromSoul({
        hasRoute: true,
        availability: "unwired",
        downgraded: false,
      }),
    ).toBe("unavailable");
  });

  test("marks downgraded routes and degraded outcomes degraded", () => {
    expect(
      routeHealthFromSoul({
        hasRoute: true,
        availability: "available",
        downgraded: true,
      }),
    ).toBe("degraded");
    expect(
      routeHealthFromSoul({
        hasRoute: true,
        availability: "available",
        downgraded: false,
        lastOutcome: "degraded",
      }),
    ).toBe("degraded");
  });

  test("uses most-severe-wins precedence", () => {
    expect(
      routeHealthFromSoul({
        hasRoute: true,
        availability: "unwired",
        downgraded: true,
        lastOutcome: "degraded",
      }),
    ).toBe("unavailable");
    expect(
      routeHealthFromSoul({
        hasRoute: true,
        availability: "available",
        downgraded: true,
        lastOutcome: "failed",
      }),
    ).toBe("unavailable");
  });
});
