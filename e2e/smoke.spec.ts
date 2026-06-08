import { test, expect } from '@playwright/test';

/**
 * Basic smoke for the Border Agents web surface.
 * Verifies the dev server + React app boots and key chrome is present.
 *
 * This is the entry point for agents exploring the UI via codegen or scripted tests.
 */

test.describe('Border Agents web surface (browser preview / dev server)', () => {
  test('loads the app at the Vite dev server', async ({ page }) => {
    await page.goto('/');

    // Basic document readiness
    await expect(page).toHaveTitle(/Border Agents|border-agents/i);

    // The main app container from src/main.tsx + BorderDock should be present
    // We look for stable structural markers that survive both Tauri and browser-preview modes.
    const root = page.locator('#root');
    await expect(root).toBeVisible();
  });

  test('renders the primary border dock / surface area', async ({ page }) => {
    await page.goto('/');

    // BorderDock is the central chrome. Look for characteristic content or role.
    // In browser preview mode it still renders the dock + preview surfaces.
    await expect(page.getByText(/Border Agents|Trust Workbench|Nexus|dock/i).first()).toBeVisible({
      timeout: 10_000,
    });

    // At minimum the React root has children (not an empty shell)
    const rootChildren = page.locator('#root > *');
    await expect(rootChildren.first()).toBeVisible();
  });

  test('has interactive elements for governance surfaces (buddies / workbench)', async ({ page }) => {
    await page.goto('/');

    // The primary visible "border" is the Border Buddies dock (unified/preview mode).
    // Buddies are actively showing memory grading / receipt status.
    await expect(
      page.getByRole('main', { name: /Border Buddies dock/i })
    ).toBeVisible();

    // At least one buddy bubble with governance-relevant text is present and interactive.
    await expect(
      page.getByRole('button', { name: /Memory graded|Hermes gateway|receipt checks/i }).first()
    ).toBeVisible();
  });
});
