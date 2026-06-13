// Buddy capability blueprint — the typed, data-driven description of what a buddy can
// take IN (free text + slash commands) and put OUT (which output surfaces it renders).
//
// This is the single source of truth consumed by three places, so the displayed
// blueprint and the runtime behavior can never drift:
//   - the onboarding wizard renders a buddy's real capabilities (and /help),
//   - the gateway parses slash commands against this manifest,
//   - the bodies (desktop torso + browser surface) render the declared outputs.
//
// It is intentionally self-contained (no imports from buddyProfiles) so buddyProfiles
// can attach a `capabilities` block without a circular dependency. Future custom
// buddies are authored against this shape.

// The output surfaces a buddy is allowed to render. `session` is the idle status card.
export type OutputSurfaceKind = "text" | "image" | "file" | "session";

// What firing a command asks the gateway to do. A tagged union keeps command dispatch
// data-driven rather than a pile of string comparisons.
export type SlashCommandAction =
  | { kind: "generate_image" }
  | { kind: "clear" }
  | { kind: "help" }
  | { kind: "set_model" }
  | { kind: "retry" };

export interface SlashCommandSpec {
  // Canonical name, lowercase, no leading slash (e.g. "image").
  name: string;
  // Usage hint for the args after the name (e.g. "<prompt>"), or "" for no args.
  args: string;
  // One-line description shown in /help and the wizard's capability list.
  summary: string;
  action: SlashCommandAction;
}

export interface BuddyInputCapabilities {
  freeText: boolean;
  commands: readonly SlashCommandSpec[];
}

export interface BuddyCapabilities {
  inputs: BuddyInputCapabilities;
  outputs: readonly OutputSurfaceKind[];
}

// The five v1 commands (user-selected). The gateway hand-mirrors these specs; this
// array stays the canonical definition both sides point at.
export const DEFAULT_HERMES_COMMANDS: readonly SlashCommandSpec[] = [
  {
    name: "image",
    args: "<prompt>",
    summary: "Generate an image from a prompt and show it in the torso.",
    action: { kind: "generate_image" },
  },
  {
    name: "help",
    args: "",
    summary: "List what this buddy can take in and put out.",
    action: { kind: "help" },
  },
  {
    name: "clear",
    args: "",
    summary: "Clear the output surface back to the session card.",
    action: { kind: "clear" },
  },
  {
    name: "model",
    args: "<id>",
    summary: "Switch the active model for this session.",
    action: { kind: "set_model" },
  },
  {
    name: "retry",
    args: "",
    summary: "Re-run your last prompt.",
    action: { kind: "retry" },
  },
] as const;

// Hermes is the fully-wired buddy: free text, all commands, every output surface.
export const HERMES_CAPABILITIES: BuddyCapabilities = {
  inputs: { freeText: true, commands: DEFAULT_HERMES_COMMANDS },
  outputs: ["text", "image", "file", "session"],
};

// Companions that today only converse: free text in, text/session out. They gain
// commands/outputs by widening their own manifest, never by special-casing the runtime.
export const TEXT_ONLY_CAPABILITIES: BuddyCapabilities = {
  inputs: { freeText: true, commands: [] },
  outputs: ["text", "session"],
};

export interface ParsedSlashCommand {
  spec: SlashCommandSpec;
  // Trimmed args after the command name (e.g. the prompt for /image).
  rest: string;
}

// Parse a line of input against a command set. Returns null for non-commands (no
// leading slash) and unknown commands, so callers fall through to free-text handling.
// Command names are matched case-insensitively; the rest of the line is the args.
export function parseSlashCommand(
  text: string,
  commands: readonly SlashCommandSpec[],
): ParsedSlashCommand | null {
  const trimmed = text.trimStart();
  if (!trimmed.startsWith("/")) {
    return null;
  }
  const match = trimmed.slice(1).match(/^(\S+)\s*([\s\S]*)$/);
  if (!match) {
    return null;
  }
  const [, rawName, rest] = match;
  const spec = commands.find((command) => command.name === rawName.toLowerCase());
  if (!spec) {
    return null;
  }
  return { spec, rest: rest.trim() };
}

// Render a buddy's command list as plain lines — used by /help on both surfaces.
export function formatCommandHelp(commands: readonly SlashCommandSpec[]): string {
  if (commands.length === 0) {
    return "This buddy takes free text only.";
  }
  return commands
    .map((command) => {
      const usage = command.args ? `/${command.name} ${command.args}` : `/${command.name}`;
      return `${usage} — ${command.summary}`;
    })
    .join("\n");
}

export function isOutputSurfaceKind(value: unknown): value is OutputSurfaceKind {
  return value === "text" || value === "image" || value === "file" || value === "session";
}
