import { afterEach, beforeEach, describe, expect, test } from "vitest";
import { mkdtempSync, rmSync, writeFileSync, chmodSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, delimiter } from "node:path";

import { commandOnPath } from "../liveEffectorExecutors";

// Direct unit tests for the PATH resolver the launcher executors lean on. The launcher's
// "fail closed" guarantee (missing CLI → clean error bubble, never a silent no-op) rests on
// this returning false reliably, so we pin the three branches it branches on: a bare command
// found on PATH, a bare command absent from PATH, and an explicit absolute path checked directly.

describe("commandOnPath", () => {
  const originalPath = process.env.PATH;
  let dir: string;

  beforeEach(() => {
    dir = mkdtempSync(join(tmpdir(), "bb-cmdpath-"));
    // Scope PATH to the temp dir plus /bin so `sh`-style lookups still resolve if needed.
    process.env.PATH = [dir, "/bin", "/usr/bin"].filter(Boolean).join(delimiter);
  });

  afterEach(() => {
    process.env.PATH = originalPath;
    rmSync(dir, { recursive: true, force: true });
  });

  test("finds a bare command present on PATH", () => {
    writeFileSync(join(dir, "bb-fake-launcher"), "#!/bin/sh\necho hi\n");
    chmodSync(join(dir, "bb-fake-launcher"), 0o755);
    expect(commandOnPath("bb-fake-launcher")).toBe(true);
  });

  test("returns false for a bare command absent from PATH", () => {
    expect(commandOnPath("bb-definitely-not-installed-xyz")).toBe(false);
  });

  test("checks an explicit absolute path directly (ignores PATH)", () => {
    const abs = join(dir, "explicit-tool");
    writeFileSync(abs, "#!/bin/sh\necho hi\n");
    chmodSync(abs, 0o755);
    expect(commandOnPath(abs)).toBe(true);
  });

  test("returns false for an explicit path that is not executable", () => {
    const abs = join(dir, "not-exec");
    writeFileSync(abs, "no exec bit here");
    chmodSync(abs, 0o644);
    expect(commandOnPath(abs)).toBe(false);
  });

  test("returns false for an empty command", () => {
    expect(commandOnPath("")).toBe(false);
  });
});
