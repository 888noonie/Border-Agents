// @vitest-environment jsdom

import "@testing-library/jest-dom/vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
import { BuddySurface } from "../BuddySurface";

const baseProps = {
  buddy: { id: "hermes", shortName: "Hermes", message: "Attention required." },
  dockSlot: 0.5,
  edge: "right" as const,
  gatewayAutoConnect: false,
  gatewayBusy: false,
  gatewayDetail: null,
  gatewayState: "idle" as const,
  gatewayUrl: "ws://127.0.0.1:17387",
  hasGateway: true,
  message: "Attention required.",
  settings: {
    allowAction: false,
    allowExternalShare: false,
    connectionLabel: "Not connected",
    enabled: true,
    memoryMode: "purpose_graded" as const,
    modelLabel: "Grok subscription",
    provider: "grok" as const,
  },
  onGatewayConnect: vi.fn(),
  onGatewayDisconnect: vi.fn(),
  onGatewaySettingsChange: vi.fn(),
  onRequestDock: vi.fn(),
  onRequestInteract: vi.fn(),
  onSendChat: vi.fn(),
  onSettingsChange: vi.fn(),
};

describe("BuddySurface", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  afterEach(() => {
    cleanup();
    vi.clearAllMocks();
  });

  test("keeps docked speech output minimal", () => {
    const { container } = render(<BuddySurface {...baseProps} interactive={false} />);

    expect(screen.getByText("Attention required.")).toBeInTheDocument();
    expect(screen.queryByLabelText("Bubble controls")).not.toBeInTheDocument();
    expect(container.querySelectorAll(".buddy-ui-bubble")).toHaveLength(1);
  });

  test("shows collapsed tab sections when undocked and expands on demand", async () => {
    const user = userEvent.setup();

    const { container } = render(<BuddySurface {...baseProps} interactive />);

    expect(screen.getByLabelText("Bubble controls")).toBeInTheDocument();
    expect(container.querySelectorAll(".buddy-ui-bubble")).toHaveLength(1);
    expect(screen.queryByLabelText("Ask Hermes")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Open settings" })).not.toBeInTheDocument();

    const messageSection = screen.getByRole("button", { name: /Latest output/i });
    expect(messageSection).toHaveAttribute("aria-expanded", "false");

    await user.click(messageSection);

    expect(messageSection).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByLabelText("Ask Hermes")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Settings" }));

    const settingsSection = screen.getByRole("button", { name: /Model & gateway settings/i });
    expect(settingsSection).toHaveAttribute("aria-expanded", "false");

    await user.click(settingsSection);

    expect(settingsSection).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByRole("button", { name: "Open settings" })).toBeInTheDocument();
    expect(screen.getByLabelText("Always centre and fit full height")).toBeInTheDocument();
    expect(screen.getByLabelText("Keep settings inside border")).toBeChecked();
  });

  test("persists undocked tab and section state for each buddy", async () => {
    const user = userEvent.setup();

    const view = render(<BuddySurface {...baseProps} interactive />);

    await user.click(screen.getByRole("button", { name: "Settings" }));
    await user.click(screen.getByRole("button", { name: /Model & gateway settings/i }));
    await user.click(screen.getByLabelText("Always centre and fit full height"));

    view.unmount();
    render(<BuddySurface {...baseProps} interactive />);

    expect(screen.getByRole("button", { name: "Settings" })).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByRole("button", { name: /Model & gateway settings/i })).toHaveAttribute(
      "aria-expanded",
      "true",
    );
    expect(screen.getByLabelText("Always centre and fit full height")).toBeChecked();
  });
});
