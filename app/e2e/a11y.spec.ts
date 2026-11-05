import AxeBuilder from '@axe-core/playwright';
import { expect, test } from '@playwright/test';

test('contrast_aa_both_themes', async ({ page }) => {
  await page.goto('/?demo=1');
  await page.locator('[role="row"][data-kind="file"]').first().click();
  await page.waitForTimeout(500);
  const r = await new AxeBuilder({ page }).withRules(['color-contrast']).analyze();
  const where = r.violations.flatMap((v) => v.nodes.map((n) => `${n.target.join(' ')}: ${n.failureSummary}`));
  expect(where).toEqual([]);
});

test('reduced_motion_stops_animation', async ({ page }) => {
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await page.goto('/?demo=1');
  const r = await page.evaluate(() => {
    const anim = [...document.querySelectorAll('.spin, .pulse')].map((e) => getComputedStyle(e).animationName);
    const sw = document.querySelector('.sw');
    return { anim, sw: sw ? getComputedStyle(sw).transitionDuration : null };
  });
  expect(r.anim.every((a) => a === 'none')).toBe(true);
  expect(r.sw).toBe('0s');
});

test('keyboard_only_flow', async ({ page }) => {
  await page.goto('/?demo=1');
  await expect(page.locator('[role="row"][data-kind]').first()).toBeVisible();
  let onRow = false;
  for (let i = 0; i < 15 && !onRow; i++) {
    await page.keyboard.press('Tab');
    onRow = await page.evaluate(() => document.activeElement?.getAttribute('role') === 'row');
  }
  expect(onRow).toBe(true);
  await page.keyboard.press('ArrowDown');
  const sel = page.locator('[role="row"][aria-selected="true"]');
  await expect(sel).toContainText('ubuntu');
  await expect(sel).toBeFocused();
  await page.keyboard.press('Space');
  await expect(sel).toContainText('Paused');
  let onOpen = false;
  for (let i = 0; i < 10 && !onOpen; i++) {
    await page.keyboard.press('Tab');
    onOpen = await page.evaluate(() => document.activeElement?.textContent?.trim() === 'Open folder');
  }
  expect(onOpen).toBe(true);
});

test('focus_ring_is_focus_token', async ({ page }) => {
  await page.goto('/?demo=1');
  await page.keyboard.press('Tab');
  const outline = await page.evaluate(() => {
    const s = getComputedStyle(document.activeElement!);
    return `${s.outlineStyle} ${s.outlineWidth}`;
  });
  expect(outline).toBe('solid 2px');
});
