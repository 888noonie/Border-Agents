import { expect, test } from '@playwright/test';

test.describe('Dock controls', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    await page.evaluate(() => {
      [
        'border-agents:dock-chrome:v1',
        'border-agents:user-modes:v1',
        'border-buddies:dock:v2',
        'border-buddies:placements:v4',
      ].forEach((key) => localStorage.removeItem(key));
    });
    await page.reload();
  });

  test('keeps Work Play Adjust compact and opens dock tools on demand', async ({ page }) => {
    await expect(page.getByRole('button', { name: 'Work', exact: true })).toBeVisible();
    await expect(page.getByRole('button', { name: 'Play', exact: true })).toBeVisible();
    await expect(page.getByRole('button', { name: 'Adjust', exact: true })).toBeVisible();
    await expect(page.getByLabel('Dock adjustment controls')).toHaveCount(0);

    await page.getByRole('button', { name: 'Adjust', exact: true }).click();

    await expect(page.getByLabel('Dock adjustment controls')).toBeVisible();
    await expect(page.getByRole('button', { name: /Cycle dock render mode/i })).toBeVisible();
    await expect(page.getByRole('button', { name: 'Enter fullscreen' })).toBeVisible();
    await expect(page.getByRole('button', { name: 'Move dock controls' })).toBeVisible();
    await expect(page.getByRole('button', { name: 'Centre dock controls' })).toBeVisible();
    await expect(page.getByRole('button', { name: /Pass through/i })).toBeVisible();
    await expect(page.getByRole('button', { name: 'Hide dock' })).toBeVisible();
    await expect(page.getByRole('button', { name: 'Fit buddies to border' })).toBeVisible();
    await expect(page.getByRole('button', { name: 'Heal dock controls' })).toBeVisible();
  });

  test('moves dock chrome and persists placement in browser preview', async ({ page }) => {
    await page.getByRole('button', { name: 'Adjust', exact: true }).click();

    const chrome = page.locator('.dock-chrome');
    const before = await chrome.boundingBox();
    expect(before).not.toBeNull();

    const moveButton = page.getByRole('button', { name: 'Move dock controls' });
    await moveButton.hover();
    await page.mouse.down();
    await page.mouse.move((before?.x ?? 0) + 150, (before?.y ?? 0) + 85, { steps: 8 });
    await page.mouse.up();

    const after = await chrome.boundingBox();
    expect(after).not.toBeNull();
    expect(Math.hypot((after?.x ?? 0) - (before?.x ?? 0), (after?.y ?? 0) - (before?.y ?? 0))).toBeGreaterThan(40);

    await page.reload();
    const afterReload = await chrome.boundingBox();
    expect(afterReload).not.toBeNull();
    expect(Math.abs((afterReload?.x ?? 0) - (after?.x ?? 0))).toBeLessThan(2);
    expect(Math.abs((afterReload?.y ?? 0) - (after?.y ?? 0))).toBeLessThan(2);
  });
});
