export const SELF_HEAL_INTERVAL_MS = 12_000;
export const STUCK_DRAG_TIMEOUT_MS = 5_000;
export const DOCK_RECOVER_SHORTCUT = "CommandOrControl+Alt+Shift+R";

export type DockHealAction =
  | "cleared-stuck-drag"
  | "restored-pointer"
  | "refreshed-hitboxes"
  | "expanded-dock"
  | "disabled-pass-through"
  | "recalled-buddies"
  | "restored-overlay";

export type DockHealReport = {
  healed: boolean;
  actions: DockHealAction[];
  at: number;
};

export function createHealReport(actions: DockHealAction[]): DockHealReport {
  return {
    healed: actions.length > 0,
    actions,
    at: Date.now(),
  };
}
