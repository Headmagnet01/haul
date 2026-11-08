import AxeBuilder from '@axe-core/playwright';
import { expect, test, type Page } from '@playwright/test';

type Mock = { __haulMock: { queueDrained(): void } };
const calls = (page: Page) => page.evaluate(() => (window as unknown as { __haulMockCalls: string[] }).__haulMockCalls);
const select = (page: Page) => page.getByLabel('When done');

async function drainWith(page: Page, mode: 'sleep' | 'shutdown') {
  await page.goto('/');
  await select(page).selectOption(mode);
  await page.evaluate(() => (window as unknown as Mock).__haulMock.queueDrained());
}

test('shutdown_countdown_esc_cancels', async ({ page }) => {
  await drainWith(page, 'shutdown');
  const dlg = page.getByRole('alertdialog');
  await expect(dlg).toContainText('Shutting down in 60 s');
  await expect(dlg.getByRole('button', { name: 'Cancel' })).toBeFocused();
  await expect(dlg).toContainText(/Shutting down in 5\d s/, { timeout: 5000 });
  await page.keyboard.press('Escape');
  await expect(dlg).toHaveCount(0);
  await expect(select(page)).toHaveValue('nothing');
  expect(await calls(page)).toContain('whenDoneCancel');
});

test('sleep_countdown_do_it_now', async ({ page }) => {
  await drainWith(page, 'sleep');
  const dlg = page.getByRole('alertdialog');
  await expect(dlg).toContainText('Going to sleep in 60 s');
  await dlg.getByRole('button', { name: 'Do it now' }).click();
  await expect(dlg).toHaveCount(0);
  expect(await calls(page)).toContain('whenDoneNow');
  await expect(select(page)).toHaveValue('nothing');
});

test('nothing_chosen_drain_does_nothing', async ({ page }) => {
  await page.goto('/');
  await page.evaluate(() => (window as unknown as Mock).__haulMock.queueDrained());
  await page.waitForTimeout(500);
  await expect(page.getByRole('alertdialog')).toHaveCount(0);
});

test('countdown_has_no_violations', async ({ page }) => {
  await drainWith(page, 'shutdown');
  await expect(page.getByRole('alertdialog')).toBeVisible();
  const r = await new AxeBuilder({ page }).include('[role="alertdialog"]').withTags(['wcag2a', 'wcag2aa']).analyze();
  expect(r.violations.map((v) => `${v.id}: ${v.nodes.length}`)).toEqual([]);
});
