// @vitest-environment jsdom

import "@testing-library/jest-dom/vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, test, vi } from "vitest";
import { BuddyUiBubble, type BuddyUiBubbleTab } from "../BuddyUiBubble";

describe("BuddyUiBubble", () => {
  test("switches tabs without activating the bubble", async () => {
    const user = userEvent.setup();
    const onActivate = vi.fn();
    const onAction = vi.fn();
    let activeTab = "message";

    const tabs: BuddyUiBubbleTab[] = [
      {
        id: "message",
        label: "Message",
        icon: "M",
        tone: "message",
        content: "Ready on the border.",
      },
      {
        id: "settings",
        label: "Settings",
        icon: "S",
        tone: "settings",
        content: <button type="button" onClick={onAction}>Open settings</button>,
      },
    ];

    const { rerender } = render(
      <BuddyUiBubble
        activeTab={activeTab}
        bubbleSide="left"
        bubbleVertical="above"
        clickable
        mounted
        phase="visible"
        tabs={tabs}
        text="Ready on the border."
        onActivate={onActivate}
        onTabChange={(tabId) => {
          activeTab = tabId;
        }}
      />,
    );

    await user.click(screen.getByRole("button", { name: "Settings" }));
    rerender(
      <BuddyUiBubble
        activeTab={activeTab}
        bubbleSide="left"
        bubbleVertical="above"
        clickable
        mounted
        phase="visible"
        tabs={tabs}
        text="Ready on the border."
        onActivate={onActivate}
        onTabChange={(tabId) => {
          activeTab = tabId;
        }}
      />,
    );

    expect(onActivate).not.toHaveBeenCalled();
    expect(screen.getByRole("button", { name: "Open settings" })).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Open settings" }));

    expect(onAction).toHaveBeenCalledTimes(1);
    expect(onActivate).not.toHaveBeenCalled();
  });
});
