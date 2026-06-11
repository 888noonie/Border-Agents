import { describe, expect, it } from "vitest";
import {
  DEFAULT_HERMES_COMMANDS,
  HERMES_CAPABILITIES,
  TEXT_ONLY_CAPABILITIES,
  formatCommandHelp,
  isOutputSurfaceKind,
  parseSlashCommand,
} from "../buddyCapabilities";
import { BUDDY_PROFILES } from "../buddyProfiles";

describe("parseSlashCommand", () => {
  it("returns null for free text (no leading slash)", () => {
    expect(parseSlashCommand("draw me a bike", DEFAULT_HERMES_COMMANDS)).toBeNull();
    expect(parseSlashCommand("  hello /image", DEFAULT_HERMES_COMMANDS)).toBeNull();
  });

  it("returns null for an unknown command", () => {
    expect(parseSlashCommand("/teleport now", DEFAULT_HERMES_COMMANDS)).toBeNull();
  });

  it("parses a command with args and trims the rest", () => {
    const parsed = parseSlashCommand("/image  a red bicycle  ", DEFAULT_HERMES_COMMANDS);
    expect(parsed?.spec.name).toBe("image");
    expect(parsed?.spec.action).toEqual({ kind: "generate_image" });
    expect(parsed?.rest).toBe("a red bicycle");
  });

  it("matches the command name case-insensitively and tolerates leading space", () => {
    const parsed = parseSlashCommand("  /HELP", DEFAULT_HERMES_COMMANDS);
    expect(parsed?.spec.name).toBe("help");
    expect(parsed?.rest).toBe("");
  });

  it("parses a no-arg command with an empty rest", () => {
    const parsed = parseSlashCommand("/clear", DEFAULT_HERMES_COMMANDS);
    expect(parsed?.spec.action).toEqual({ kind: "clear" });
    expect(parsed?.rest).toBe("");
  });
});

describe("capability manifests", () => {
  it("Hermes exposes all five commands and every output surface", () => {
    expect(HERMES_CAPABILITIES.inputs.commands.map((c) => c.name)).toEqual([
      "image",
      "help",
      "clear",
      "model",
      "retry",
    ]);
    expect(HERMES_CAPABILITIES.outputs).toEqual(["text", "image", "file", "session"]);
  });

  it("text-only buddies take free text but expose no commands and only text/session", () => {
    expect(TEXT_ONLY_CAPABILITIES.inputs.freeText).toBe(true);
    expect(TEXT_ONLY_CAPABILITIES.inputs.commands).toEqual([]);
    expect(TEXT_ONLY_CAPABILITIES.outputs).toEqual(["text", "session"]);
  });

  it("every buddy profile carries a capability blueprint", () => {
    for (const profile of Object.values(BUDDY_PROFILES)) {
      expect(profile.capabilities.inputs.freeText).toBe(true);
      expect(Array.isArray(profile.capabilities.outputs)).toBe(true);
      expect(profile.capabilities.outputs.every(isOutputSurfaceKind)).toBe(true);
    }
  });

  it("only Hermes ships commands today; the rest are conversational", () => {
    expect(BUDDY_PROFILES.hermes.capabilities.inputs.commands.length).toBe(5);
    expect(BUDDY_PROFILES.crab.capabilities.inputs.commands.length).toBe(0);
    expect(BUDDY_PROFILES.owl.capabilities.inputs.commands.length).toBe(0);
    expect(BUDDY_PROFILES.fox.capabilities.inputs.commands.length).toBe(0);
  });
});

describe("formatCommandHelp", () => {
  it("renders usage + summary per command", () => {
    const help = formatCommandHelp(DEFAULT_HERMES_COMMANDS);
    expect(help).toContain("/image <prompt> — ");
    expect(help).toContain("/help — ");
    expect(help.split("\n")).toHaveLength(5);
  });

  it("states free-text-only when there are no commands", () => {
    expect(formatCommandHelp([])).toBe("This buddy takes free text only.");
  });
});
