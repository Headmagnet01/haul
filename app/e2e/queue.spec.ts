import { expect, test, type Page } from '@playwright/test';
import { addViaCollector, paste, startAllInCollector } from './helpers';

const rows = (page: Page) => page.locator('[role="treegrid"] [role="row"][data-kind]');
const row = (page: Page, text: string | RegExp) => rows(page).filter({ hasText: text });

test('empty_state_shown', async ({ page }) => {
  // Past the first run (a fresh start shows the welcome instead: welcome.spec.ts).
  await page.addInitScript(() => sessionStorage.setItem('haul.mock.settings', JSON.stringify({ firstDownloadSeen: true })));
  await page.goto('/');
  await expect(page.getByText('Copy a link or drop one here')).toBeVisible();
  await expect(page.getByText('Press Ctrl+V anywhere to add links.')).toBeVisible();
});

test('paste_adds_downloads', async ({ page }) => {
  await page.goto('/');
  await addViaCollector(page, 'https://a.org/one.bin https://a.org/two.bin');
  const pkg = rows(page).filter({ hasText: 'Pasted list · ' });
  await expect(pkg).toHaveCount(1);
  await expect(pkg).toHaveAttribute('aria-level', '1');
  await expect(page.locator('[role="row"][aria-level="2"]')).toHaveCount(2);
  await expect(page.getByRole('status')).toContainText('Started 2 downloads');
});

test('paste_without_links_says_so', async ({ page }) => {
  await page.goto('/');
  await paste(page, 'hello');
  await expect(page.getByRole('status')).toContainText('No links found');
  await expect(rows(page)).toHaveCount(0);
});

test('add_links_panel', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('button', { name: /Add links/ }).click();
  const box = page.getByRole('textbox', { name: 'Links to add' });
  await box.fill('https://a.org/panel.bin');
  await page.getByRole('button', { name: 'Add', exact: true }).click();
  await expect(page.getByRole('status')).toContainText('Sent 1 link to the Collector');
  await expect(box).toBeHidden();
  await startAllInCollector(page);
  await expect(row(page, 'panel.bin')).toHaveCount(1);
});

test('rows_progress_live', async ({ page }) => {
  await page.goto('/?demo=1');
  const iso = row(page, 'ubuntu-24.04.2-desktop-amd64.iso');
  await expect(iso).toContainText('Downloading');
  const pct = iso.locator('.pct');
  const first = await pct.textContent();
  await expect(pct).not.toHaveText(first ?? '', { timeout: 2000 });
});

test('space_pauses_and_resumes', async ({ page }) => {
  await page.goto('/?demo=1');
  const iso = row(page, 'ubuntu-24.04.2-desktop-amd64.iso');
  await iso.click();
  await page.keyboard.press('Space');
  await expect(iso).toContainText('Paused');
  await page.keyboard.press('Space');
  await expect(iso).toContainText(/Downloading|Waiting/);
});

test('typing_in_filter_does_not_pause', async ({ page }) => {
  await page.goto('/?demo=1');
  const iso = row(page, 'ubuntu-24.04.2-desktop-amd64.iso');
  await iso.click();
  const filter = page.getByPlaceholder('Filter downloads');
  await filter.click();
  await page.keyboard.type('iso ');
  await expect(filter).toHaveValue('iso ');
  await page.waitForTimeout(600);
  await expect(iso).toContainText('Downloading');
});

test('filter_narrows', async ({ page }) => {
  await page.goto('/?demo=1');
  await page.getByPlaceholder('Filter downloads').fill('sprite');
  await expect(rows(page)).toHaveCount(6);
  await expect(rows(page).first()).toHaveAttribute('data-kind', 'package');
});

test('collapse_package', async ({ page }) => {
  await page.goto('/?demo=1');
  const pkg = rows(page).filter({ hasText: 'sprite-fright-4k' }).and(page.locator('[data-kind="package"]'));
  await pkg.click();
  await expect(page.locator('[role="row"][aria-level="2"]')).toHaveCount(5);
  await page.keyboard.press('ArrowLeft');
  await expect(page.locator('[role="row"][aria-level="2"]')).toHaveCount(0);
  await expect(pkg).toHaveAttribute('aria-expanded', 'false');
  await page.keyboard.press('ArrowRight');
  await expect(page.locator('[role="row"][aria-level="2"]')).toHaveCount(5);
});

test('long_name_no_overflow', async ({ page }) => {
  await page.goto('/');
  await addViaCollector(page, `https://a.org/${'a'.repeat(300)}.bin`);
  await expect(rows(page)).toHaveCount(1);
  const fits = await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth);
  expect(fits).toBe(true);
});
