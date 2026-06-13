import { expect, test, type Page } from '@playwright/test';

/**
 * Governance action gate — the confirm→execute round-trip, end to end through the real UI.
 *
 * This is the browser proof for roadmap step 5: a buddy action request runs through the
 * soul-side gate (src/soulActions.ts → src/core/actionGate.ts) and renders an ActionReceipt.
 * Under the default Work posture, a low-risk `receipt_review` hits the confirmation floor
 * (needs_confirmation); confirming clears the floor and yields an `allow` receipt + the
 * action ledger the effector exists to open.
 *
 * It also pins the persona→governance id resolution (the dock addresses Veritas by its
 * persona id "owl"; the gate authorizes under the governance id "veritas"). Before that
 * mapping existed the request fell through to `blocked` (ungranted) — this test guards it.
 */

// The dock keeps buddies "tucked" on the border (ambient bubble only). The interactive
// composer only exists in the "free" state, so we seed Veritas (persona id "owl") free.
async function seedVeritasUndocked(page: Page) {
  await page.goto('/');
  await page.evaluate(() => {
    [
      'border-agents:dock-chrome:v1',
      'border-agents:user-modes:v1',
      'border-buddies:dock:v2',
    ].forEach((key) => localStorage.removeItem(key));
    localStorage.setItem(
      'border-buddies:placements:v4',
      JSON.stringify({ owl: { state: 'free', edge: 'top', x: 420, y: 320 } }),
    );
  });
  await page.reload();
}

// Reveal the Veritas composer: minimize the workbench preview overlay (it covers the top of
// the stage) and expand the buddy's collapsed "Latest output" section.
async function openVeritasComposer(page: Page) {
  await page.getByRole('button', { name: /Minimize Trust Workbench preview/i }).click();
  await page.getByRole('button', { name: /Latest output/i }).first().click();
  return page.getByLabel('Ask Veritas');
}

test.describe('Governance action gate (confirm → execute)', () => {
  test('runs /review through the gate: needs_confirmation → Confirm → allow + ledger', async ({
    page,
  }) => {
    await seedVeritasUndocked(page);
    const composer = await openVeritasComposer(page);
    await expect(composer).toBeVisible();

    // Body emits the action request. Under Work posture this low-risk reach hits the floor.
    // (The composer is a textarea, so submission is the send button, not Enter.)
    await composer.fill('/review');
    await page.locator('.buddy-panel__composer button').first().click();

    const card = page.locator('.action-receipt-card');
    await expect(card).toHaveAttribute('data-decision', 'needs_confirmation');
    await expect(page.locator('.action-receipt-card__badge')).toHaveText('Needs confirmation');
    // The derivation trail (the same DerivationStep[] the memory receipts use) is rendered…
    await expect(card.locator('.action-receipt-card__rules li').first()).toBeVisible();
    // …and crucially NOT a "blocked / ungranted" outcome (the persona→governance id bug).
    await expect(card).not.toHaveAttribute('data-decision', 'blocked');

    // User confirms: a second request carries confirmed:true and clears the risk floor.
    await page.locator('.action-receipt-card__confirm').click();

    await expect(card).toHaveAttribute('data-decision', 'allow');
    await expect(page.locator('.action-receipt-card__badge')).toHaveText('Allowed');
    // On allow, receipt_review opens the read-only action ledger it exists to surface.
    await expect(page.locator('.action-receipt-card__ledger h4')).toContainText('Action ledger');
  });
});
