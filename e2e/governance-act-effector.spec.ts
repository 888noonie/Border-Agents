import { expect, test, type Page } from '@playwright/test';

/**
 * Execution membrane — the first real `act` effector (`repo_edit`) refusing, visibly, on
 * the real Forge surface. This is the "product claim" the roundtable said was still owed:
 * the user types the dangerous thing, and the gate says no — permanently, with a receipt.
 *
 * The gate authorizes the EFFECT (typed intent + target), not just the grant. So the SAME
 * `repo_edit` effector is hard-blocked for `AGENTS.md` / a traversal path, and blocked for a
 * safe path that lacks trusted action-backing — all visible in the ActionReceipt's derivation
 * trail. Crucially, a protected target never even offers a Confirm affordance: you cannot
 * confirm your way past a hard block.
 *
 * Forge wears the "crab" persona (character "Claw"); the gate authorizes under the governance
 * id "forge" via resolveManifestId — the same seam that bit receipt_review, now proven for an
 * act effector on a live surface.
 *
 * The `allow` path for an act effector requires an action-backed memory turn carrying
 * may_use_for_action. On a live surface that backing comes from a graded provider turn; in
 * the browser preview it is seeded deterministically (allowAction + one real memory turn),
 * letting us prove the PERMIT twin on the real UI too (Case C below). The same proof path
 * Case A blocks for lack of backing flows needs_confirmation → Confirm → allow once backed —
 * and the membrane stays honest: the proof executor runs (completing the ActionReceipt +
 * ExecutionReceipt audit pair) but is sandboxed to .border-agents/proofs/ and writes nothing
 * to disk in this build, and the receipt says so. The soul-layer proof of the action-backed
 * derivation lives in src/__tests__/soulActions.test.ts ("Case C").
 */

async function seedForgeUndocked(page: Page) {
  await page.goto('/');
  await page.evaluate(() => {
    ['border-agents:dock-chrome:v1', 'border-agents:user-modes:v1', 'border-buddies:dock:v2'].forEach((key) =>
      localStorage.removeItem(key),
    );
    localStorage.setItem(
      'border-buddies:placements:v4',
      JSON.stringify({ crab: { state: 'free', edge: 'top', x: 420, y: 320 } }),
    );
  });
  await page.reload();
}

async function openForgeComposer(page: Page) {
  await page.getByRole('button', { name: /Minimize Trust Workbench preview/i }).click();
  await page.getByRole('button', { name: /Latest output/i }).first().click();
  return page.getByLabel('Ask Claw');
}

async function review(page: Page, command: string) {
  const composer = await openForgeComposer(page);
  await expect(composer).toBeVisible();
  await composer.fill(command);
  await page.locator('.buddy-panel__composer button').first().click();
  return page.locator('.action-receipt-card');
}

test.describe('Execution membrane (repo_edit refuses, visibly)', () => {
  test.beforeEach(async ({ page }) => {
    await seedForgeUndocked(page);
  });

  test('Case B: a protected target (AGENTS.md) is blocked and offers no way to confirm past it', async ({
    page,
  }) => {
    const card = await review(page, '/review repo_edit AGENTS.md');

    await expect(card).toHaveAttribute('data-decision', 'blocked');
    await expect(page.locator('.action-receipt-card__badge')).toHaveText('Blocked');
    // The reason is intent-level: the target itself is protected.
    await expect(card.locator('.action-receipt-card__rules code')).toContainText('action.blocked.protected_target');
    // You cannot confirm your way past a hard block — no Confirm affordance is rendered.
    await expect(card.locator('.action-receipt-card__confirm')).toHaveCount(0);
  });

  test('a traversal target cannot disguise a protected one', async ({ page }) => {
    const card = await review(page, '/review repo_edit src/foo/../../AGENTS.md');

    await expect(card).toHaveAttribute('data-decision', 'blocked');
    await expect(card.locator('.action-receipt-card__rules code')).toContainText('action.blocked.protected_target');
  });

  test('Case A: a safe target with no trusted action-backing is blocked (no_action_grant)', async ({ page }) => {
    const card = await review(page, '/review repo_edit .border-agents/proofs/first-act.patch');

    await expect(card).toHaveAttribute('data-decision', 'blocked');
    // Not a protected-target block — the intent reached the gate and failed the action-backing
    // floor instead, proving the membrane discriminates by target AND by trusted backing.
    await expect(card.locator('.action-receipt-card__rules code')).toContainText('action.blocked.no_action_grant');
  });

  test('Case C: the permit twin — the SAME safe target, now action-backed, is allowed and bounded', async ({
    page,
  }) => {
    // Seed the action-backing Case A lacks: turn on allowAction for Forge (crab). The seed must be
    // in place BEFORE the app's first mount (an init script), else the surface's settings-persist
    // effect races it back to the default. With backing present, the no_action_grant floor clears
    // and the gate authorizes the EFFECT — the deliberate "yes" contrast to Case A's "no" on one path.
    await page.addInitScript(() => {
      ['border-agents:dock-chrome:v1', 'border-agents:user-modes:v1', 'border-buddies:dock:v2'].forEach((key) =>
        localStorage.removeItem(key),
      );
      localStorage.setItem(
        'border-buddies:placements:v4',
        JSON.stringify({ crab: { state: 'free', edge: 'top', x: 420, y: 320 } }),
      );
      localStorage.setItem('border-buddies:settings:v2', JSON.stringify({ crab: { allowAction: true } }));
    });
    await page.goto('/');

    const composer = await openForgeComposer(page);
    await expect(composer).toBeVisible();

    // One real turn so the soul retrieves a trusted, action-backed memory to authorize against.
    await composer.fill('Stage the first-act proof patch for review.');
    await page.locator('.buddy-panel__composer button').first().click();

    // The same proof-dir path Case A blocks — now it reaches the risk floor, not the backing floor.
    await composer.fill('/review repo_edit .border-agents/proofs/first-act.patch');
    await page.locator('.buddy-panel__composer button').first().click();

    const card = page.locator('.action-receipt-card');
    await expect(card).toHaveAttribute('data-decision', 'needs_confirmation');
    await expect(page.locator('.action-receipt-card__badge')).toHaveText('Needs confirmation');

    // Confirmation re-runs the SAME intent with confirmed:true and clears the risk floor → allow.
    await card.locator('.action-receipt-card__confirm').click();

    await expect(card).toHaveAttribute('data-decision', 'allow');
    await expect(page.locator('.action-receipt-card__badge')).toHaveText('Allowed');

    // Honest membrane: allow completes the audit pair. The proof executor runs (outcome ok) but is
    // sandboxed to .border-agents/proofs/ and writes nothing to disk in this build — the receipt
    // says so in plain words, so an authorized act is never a silent or unbounded one.
    const exec = card.locator('.action-receipt-card__execution');
    await expect(exec).toHaveAttribute('data-outcome', 'ok');
    await expect(exec).toHaveAttribute('data-executed', 'true');
    await expect(exec).toContainText('no disk write in this build');
  });
});
