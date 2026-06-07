// @vitest-environment jsdom

import "@testing-library/jest-dom/vitest";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, test } from "vitest";
import { TrustWorkbenchPanel } from "../TrustWorkbenchPanel";

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
    expect(screen.getByText("mem_pkt_active_goal")).toBeInTheDocument();
    expect(screen.getByText("excluded from context")).toBeInTheDocument();

    const policyRules = screen.getByLabelText("Policy rules");
    expect(within(policyRules).getByText("Purpose requires explicit permission")).toBeInTheDocument();
    expect(within(policyRules).getByText("purpose.require_permissions")).toBeInTheDocument();

    await user.click(activeGoalButton);

    expect(activeGoalButton).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByText("mem_pkt_active_goal")).not.toBeInTheDocument();
  });
});
