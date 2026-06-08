// @vitest-environment jsdom

import "@testing-library/jest-dom/vitest";
import { afterEach, describe, expect, test } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { TrustWorkbenchPanel } from "../TrustWorkbenchPanel";

afterEach(() => {
  cleanup();
});

describe("TrustWorkbenchPanel", () => {
  test("collapses Nexus details while keeping the colored blocked counter visible", async () => {
    const user = userEvent.setup();

    render(<TrustWorkbenchPanel mode="nexus" compact />);

    expect(screen.getByRole("button", { name: /Nexus/i })).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByText("3 blocked")).toBeInTheDocument();
    expect(screen.getByLabelText("Grade buckets")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: /Nexus/i }));

    expect(screen.getByRole("button", { name: /Nexus/i })).toHaveAttribute("aria-expanded", "false");
    expect(screen.getByText("3 blocked")).toBeInTheDocument();
    expect(screen.queryByLabelText("Grade buckets")).not.toBeInTheDocument();
  });

  test("expands Veritas receipts with readable labels and raw policy keys", async () => {
    const user = userEvent.setup();

    render(<TrustWorkbenchPanel mode="veritas" compact />);

    expect(screen.getByText("4 receipt warnings")).toBeInTheDocument();
    expect(screen.getByText("4 warnings")).toBeInTheDocument();
    expect(screen.getAllByText("Blocked by missing permission").length).toBeGreaterThan(0);

    const activeGoalButton = screen.getByRole("button", { name: /chunk_active_goal/i });
    expect(activeGoalButton).toHaveAttribute("aria-expanded", "true");
    expect(screen.getAllByText("grade:agent_action:chunk_active_goal:2026-06-07T12:00:00Z").length).toBeGreaterThan(0);
    expect(screen.getAllByText("mem_pkt_active_goal").length).toBeGreaterThan(0);
    expect(screen.getAllByText("excluded from context").length).toBeGreaterThan(0);

    const policyRules = screen.getByLabelText("Policy rules");
    expect(within(policyRules).getByText("Purpose requires explicit permission")).toBeInTheDocument();
    expect(within(policyRules).getByText("purpose.require_permissions")).toBeInTheDocument();

    await user.click(activeGoalButton);

    expect(activeGoalButton).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByLabelText("Policy rules")).not.toBeInTheDocument();
  });

  test("shows receipt detail actions for the selected decision", async () => {
    const user = userEvent.setup();

    render(<TrustWorkbenchPanel mode="veritas" compact />);

    expect(screen.getByLabelText("Receipt detail")).toBeInTheDocument();
    expect(screen.getAllByText("grade:agent_action:chunk_active_goal:2026-06-07T12:00:00Z").length).toBeGreaterThan(0);
    expect(screen.getAllByText("Purpose requires explicit permission").length).toBeGreaterThan(0);

    await user.click(screen.getByRole("button", { name: "Open source" }));

    expect(screen.getByText("Selected source")).toBeInTheDocument();
    expect(screen.getByText("chat_session:hermes/active_goal")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Export receipt" }));

    expect(screen.getByText("Receipt export")).toBeInTheDocument();
    expect(screen.getByText(/"receiptId": "grade:agent_action:chunk_active_goal:2026-06-07T12:00:00Z"/)).toBeInTheDocument();
    expect(screen.getByText(/"policy_rule": "purpose.require_permissions"/)).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Trusted only" }));

    expect(screen.getByText("Trusted-only context")).toBeInTheDocument();
    expect(screen.getByText("2 of 2 prompt entries remain trusted")).toBeInTheDocument();
  });
});
