import AxeBuilder from '@axe-core/playwright';
import { expect, test, type Page } from '@playwright/test';

const calls = (page: Page) => page.evaluate(() => (window as unknown as { __haulMockCalls: string[] }).__haulMockCalls);
const TAG = 'https://github.com/Headmagnet01/haul/releases/tag/v0.2.0';

async function openSettings(page: Page) {
  await page.getByRole('navigation', { name: 'Views' }).getByRole('button', { name: 'Settings' }).click();
  await expect(page.getByRole('heading', { name: 'Settings', level: 1 })).toBeVisible();
}

async function axe(page: Page, sel: string) {
  const r = await new AxeBuilder({ page }).include(sel).withTags(['wcag2a', 'wcag2aa']).analyze();
  expect(r.violations.flatMap((v) => v.nodes.map((n) => `${v.id} ${n.target.join(' ')}: ${n.failureSummary}`))).toEqual([]);
}

// The light and dark projects (playwright.config.ts) run each test in both schemes.
test('status_bar_shows_update', async ({ page }) => {
  await page.goto('/?update=0.2.0');
  const btn = page.getByRole('button', { name: 'Update available 0.2.0' });
  await expect(btn).toBeVisible();
  await axe(page, 'footer');
  await btn.click();
  await expect.poll(() => calls(page)).toContain(`openRelease ${TAG}`);
});

test('settings_check_now_reports', async ({ page }) => {
  await page.goto('/');
  await openSettings(page);
  const app = page.getByRole('region', { name: 'App' });
  const sw = app.getByRole('switch', { name: 'Check for updates' });
  await expect(sw).toHaveAttribute('aria-checked', 'true');
  await app.getByRole('button', { name: 'Check now' }).click();
  await expect(app.getByText('You have the latest version (0.1.0).')).toBeVisible();
  await axe(page, 'main');
});

test('no_notice_when_up_to_date', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByRole('button', { name: /Update available/ })).toHaveCount(0);
});

test('settings_check_now_shows_newer', async ({ page }) => {
  await page.goto('/?update=0.2.0');
  await openSettings(page);
  const app = page.getByRole('region', { name: 'App' });
  await app.getByRole('button', { name: 'Check now' }).click();
  await expect(app.getByText('Version 0.2.0 is available.')).toBeVisible();
  await app.getByRole('link', { name: 'Open the release page' }).click();
  await expect.poll(() => calls(page)).toContain(`openRelease ${TAG}`);
});

test('switch_off_persists', async ({ page }) => {
  await page.goto('/');
  await openSettings(page);
  const sw = () => page.getByRole('region', { name: 'App' }).getByRole('switch', { name: 'Check for updates' });
  await sw().click();
  await expect(sw()).toHaveAttribute('aria-checked', 'false');
  await page.reload();
  await openSettings(page);
  await expect(sw()).toHaveAttribute('aria-checked', 'false');
});
