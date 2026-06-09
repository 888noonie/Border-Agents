import { describe, expect, test } from "vitest";
import {
  DEFAULT_USER_MODE_STATE,
  normalizeUserModeState,
  updateUserModeSettings,
} from "../userModes";

describe("user mode state", () => {
  test("defaults adjust mode to layout and recovery posture", () => {
    expect(DEFAULT_USER_MODE_STATE.modes.adjust.gateway.autoConnect).toBe(true);
    expect(DEFAULT_USER_MODE_STATE.modes.adjust.filePosture).toBe("confirm_each_step");
  });

  test("normalizes unknown stored values without widening mode behavior", () => {
    const state = normalizeUserModeState({
      activeMode: "private",
      modes: {
        work: {
          dock: { collapsed: "yes", renderMode: "unknown" },
          gateway: { url: " ", autoConnect: "yes" },
          receiptDetail: "maximum",
          filePosture: "all_files",
        },
      },
    });

    expect(state.activeMode).toBe("adjust");
    expect(state.modes.work).toMatchObject({
      dock: DEFAULT_USER_MODE_STATE.modes.work.dock,
      gateway: DEFAULT_USER_MODE_STATE.modes.work.gateway,
      receiptDetail: DEFAULT_USER_MODE_STATE.modes.work.receiptDetail,
      filePosture: DEFAULT_USER_MODE_STATE.modes.work.filePosture,
    });
  });

  test("updates one mode without changing the other remembered modes", () => {
    const state = updateUserModeSettings(DEFAULT_USER_MODE_STATE, "play", {
      dock: { collapsed: true, renderMode: "bubble", fullscreen: false },
    });

    expect(state.modes.play.dock).toEqual({ collapsed: true, renderMode: "bubble", fullscreen: false });
    expect(state.modes.work.dock).toEqual(DEFAULT_USER_MODE_STATE.modes.work.dock);
    expect(state.modes.adjust.gateway.autoConnect).toBe(true);
  });

  test("migrates legacy private settings into adjust mode", () => {
    const state = normalizeUserModeState({
      activeMode: "private",
      modes: {
        private: {
          dock: { collapsed: true, renderMode: "bubble", fullscreen: false },
          gateway: { url: "ws://localhost:19000", autoConnect: false },
        },
      },
    });

    expect(state.activeMode).toBe("adjust");
    expect(state.modes.adjust.dock).toEqual({ collapsed: true, renderMode: "bubble", fullscreen: false });
    expect(state.modes.adjust.gateway).toEqual({ url: "ws://localhost:19000", autoConnect: false });
  });
});
