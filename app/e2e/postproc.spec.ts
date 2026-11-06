import { expect, test, type Page } from '@playwright/test';
import { addViaCollector } from './helpers';

test.setTimeout(60_000);

const row = (page: Page, text: string) => page.locator('[role="row"][data-kind]').filter({ hasText: text });
const step = (page: Page, kind: string) => page.locator(`.steps li[data-step="${kind}"]`);
const toast = (page: Page) => page.getByRole('status');

async function finished(page: Page, url: string, name: string) {
  await page.goto('/');
  await addViaCollector(page, url);
  await row(page, name).click();
}

test('drawer_shows_steps_after_finish', async ({ page }) => {
  await finished(page, 'https://x.org/clip.mp4', 'clip.mp4');
  const panel = page.getByRole('complementary', { name: 'Details' });
  await expect(panel.getByRole('heading', { name: 'After download' })).toBeVisible();
  await expect(step(page, 'sort')).toHaveAttribute('data-state', 'done', { timeout: 30_000 });
  await expect(step(page, 'sort')).toContainText('Video → C:\\Users\\you\\Downloads\\Video');
  await expect(step(page, 'verify')).toContainText('No checksum');
  await expect(step(page, 'extract')).toContainText('Not an archive');
  await expect(step(page, 'scan')).toContainText('Off');
});

test('password_prompt_wrong_then_right', async ({ page }) => {
  await finished(page, 'https://x.org/secret.7z', 'secret.7z');
  await expect(row(page, 'secret.7z')).toContainText('Needs password', { timeout: 30_000 });
  const pw = page.getByLabel('Archive password');
  await expect(page.getByLabel('Remember this password')).toBeChecked();
  await pw.fill('nope');
  await page.getByRole('button', { name: 'Use password' }).click();
  await expect(step(page, 'extract')).toContainText('Wrong password');
  await expect(pw).toBeVisible();
  await pw.fill('haul');
  await page.getByRole('button', { name: 'Use password' }).click();
  await expect(step(page, 'extract')).toHaveAttribute('data-state', 'done');
  await expect(step(page, 'notify')).toHaveAttribute('data-state', 'done');
  await expect(pw).toHaveCount(0);
});

test('damaged_shows_retry', async ({ page }) => {
  await finished(page, 'https://x.org/damaged.zip', 'damaged.zip');
  await expect(step(page, 'extract')).toHaveAttribute('data-state', 'failed', { timeout: 30_000 });
  await expect(step(page, 'extract')).toContainText('Archive is damaged');
  await expect(row(page, 'damaged.zip')).toContainText('Needs attention');
  await page.getByRole('button', { name: 'Retry' }).click();
  await expect(step(page, 'extract')).toHaveAttribute('data-state', 'failed');
});

test('expected_checksum_saves_and_rejects', async ({ page }) => {
  await finished(page, 'https://x.org/a.bin', 'a.bin');
  const field = page.getByLabel('Expected checksum');
  await expect(field).toHaveAttribute('placeholder', 'Paste MD5, SHA-1, SHA-256 or CRC32');
  await field.fill('0123456789');
  await page.getByRole('button', { name: 'Save', exact: true }).click();
  await expect(toast(page)).toContainText('Not a checksum: use 8, 32, 40 or 64 hex characters');
  await field.fill('d41d8cd98f00b204e9800998ecf8427e');
  await page.getByRole('button', { name: 'Save', exact: true }).click();
  await expect(toast(page)).toContainText('Checksum saved');
  await expect(field).toHaveValue('');
});
