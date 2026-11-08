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

test('collector_has_no_violations', async ({ page }) => {
  await page.goto('/?demo=1');
  await page.getByRole('navigation', { name: 'Views' }).getByRole('button', { name: /Collector/ }).click();
  const list = page.locator('[aria-label="Collector links"]');
  await list.locator('[role="row"][data-kind="link"]').first().click();
  await page.waitForTimeout(500);
  const r = await new AxeBuilder({ page }).include('main').withRules(['color-contrast']).analyze();
  const where = r.violations.flatMap((v) => v.nodes.map((n) => `${n.target.join(' ')}: ${n.failureSummary}`));
  expect(where).toEqual([]);
  const roles = await new AxeBuilder({ page }).include('main').withTags(['wcag2a', 'wcag2aa']).analyze();
  expect(roles.violations.map((v) => `${v.id}: ${v.nodes.length}`)).toEqual([]);
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

test('rules_view_has_no_violations', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('navigation', { name: 'Views' }).getByRole('button', { name: 'Rules' }).click();
  await page.getByRole('button', { name: 'Add rule' }).click();
  await page.waitForTimeout(300);
  const r = await new AxeBuilder({ page }).include('main').withTags(['wcag2a', 'wcag2aa']).analyze();
  const where = r.violations.flatMap((v) => v.nodes.map((n) => `${v.id} ${n.target.join(' ')}: ${n.failureSummary}`));
  expect(where).toEqual([]);
});

test('drawer_steps_have_no_violations', async ({ page }) => {
  test.setTimeout(60_000);
  await page.goto('/');
  const { addViaCollector } = await import('./helpers');
  await addViaCollector(page, 'https://x.org/secret.7z');
  await page.locator('[role="row"][data-kind]').filter({ hasText: 'secret.7z' }).click();
  await expect(page.getByLabel('Archive password')).toBeVisible({ timeout: 30_000 });
  const r = await new AxeBuilder({ page }).include('aside[aria-label="Details"]').withTags(['wcag2a', 'wcag2aa']).analyze();
  const where = r.violations.flatMap((v) => v.nodes.map((n) => `${v.id} ${n.target.join(' ')}: ${n.failureSummary}`));
  expect(where).toEqual([]);
});

test('pairing_dialog_and_browsers_have_no_violations', async ({ page }) => {
  await page.goto('/');
  await page.evaluate(() => (window as unknown as { __haulMock: { pairRequest(b: string): number } }).__haulMock.pairRequest('Microsoft Edge'));
  await expect(page.getByRole('alertdialog')).toBeVisible();
  await page.waitForTimeout(300);
  const d = await new AxeBuilder({ page }).include('[role="alertdialog"]').withTags(['wcag2a', 'wcag2aa']).analyze();
  expect(d.violations.flatMap((v) => v.nodes.map((n) => `${v.id} ${n.target.join(' ')}: ${n.failureSummary}`))).toEqual([]);
  await page.getByRole('alertdialog').getByRole('button', { name: 'Allow' }).click();
  await page.getByRole('navigation', { name: 'Views' }).getByRole('button', { name: 'Settings' }).click();
  await expect(page.getByRole('region', { name: 'Browsers' }).getByRole('listitem')).toHaveCount(1);
  const r = await new AxeBuilder({ page }).include('main').withTags(['wcag2a', 'wcag2aa']).analyze();
  expect(r.violations.flatMap((v) => v.nodes.map((n) => `${v.id} ${n.target.join(' ')}: ${n.failureSummary}`))).toEqual([]);
});
