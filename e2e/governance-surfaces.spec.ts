import { test, expect } from '@playwright/test';

/**
 * Governance surface checks.
 * These tests exercise the visible "border" the product is built to make inspectable.
 *
 * Goal for agents: when you change UI chrome, dock, trust workbench, receipt panels, etc.,
 * these (or expanded versions) should catch regressions in the visible trust boundaries.
 *
 * Per project stance (see AGENTS.md): make trust decisions inspectable.
 */

test.describe('Governance / trust surfaces', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
  });

  test('exposes core purpose-aware areas (preview of memory grading UI)', async ({ page }) => {
    // The Trust Workbench / Nexus / Veritas concepts should eventually surface here.
    // For v0.1 the browser preview shows the dock + workbench preview + live memory grading messages.
    // We assert on the actual rendered governance signals we observed (buddies talking about grades/receipts).
    await expect(
      page.getByRole('button', { name: /Memory graded|Trusted pieces|receipt checks/i }).first()
    ).toBeVisible();

    // The dock itself is the primary "border" surface
    await expect(page.locator('#root')).toBeVisible();
  });

  test('demo CLIs and core logic are not the only story — the UI layer is reachable', async ({ page }) => {
    // Sanity that the frontend is not just a static shell.
    // The dock + live buddy bubbles (showing memory grades and receipt status) prove the React UI is mounted and dynamic.
    await page.goto('/');

    await expect(page.locator('#root > *').first()).toBeVisible();
    await expect(
      page.getByRole('main', { name: /Border Buddies dock/i })
    ).toBeVisible();
  });
});
