import { expect, test, type Page } from '@playwright/test';

const crows = (page: Page) => page.locator('[aria-label="Collector links"] [role="row"][data-kind]');
const crow = (page: Page, text: string | RegExp) => crows(page).filter({ hasText: text });
const toast = (page: Page) => page.getByRole('status');

async function paste(page: Page, text: string) {
  await page.evaluate((t) => {
    const dt = new DataTransfer();
    dt.setData('text/plain', t);
    document.body.dispatchEvent(new ClipboardEvent('paste', { clipboardData: dt, bubbles: true, cancelable: true }));
  }, text);
}

const openCollector = (page: Page) => page.getByRole('navigation', { name: 'Views' }).getByRole('button', { name: /Collector/ }).click();

test('paste_goes_to_collector', async ({ page }) => {
  await page.goto('/');
  await paste(page, 'https://x.org/a.bin https://x.org/b.bin');
  await expect(toast(page)).toContainText('Sent 2 links to the Collector');
  await toast(page).getByRole('button', { name: 'Show' }).click();
  await expect(page.getByRole('heading', { name: 'Collector' })).toBeVisible();
  const pkg = crows(page).filter({ hasText: 'Pasted list · ' });
  await expect(pkg).toHaveCount(1);
  await expect(pkg).toContainText('2 files');
  await expect(crow(page, 'a.bin')).toContainText('Online');
  await expect(crow(page, 'b.bin')).toContainText('Online');
});

test('paste_twice_says_already', async ({ page }) => {
  await page.goto('/');
  await paste(page, 'https://x.org/a.bin https://x.org/b.bin');
  await expect(toast(page)).toContainText('Sent 2 links');
  await paste(page, 'https://x.org/a.bin https://x.org/b.bin');
  await expect(toast(page)).toContainText('Already in the Collector');
  await openCollector(page);
  await expect(crows(page).filter({ hasText: /^(?!.*Pasted list)/ })).toHaveCount(2);
});

test('archive_parts_grouped', async ({ page }) => {
  await page.goto('/');
  await openCollector(page);
  await paste(page, 'https://x.org/m.7z.001 https://x.org/m.7z.002 https://x.org/m.7z.003 https://x.org/lone.bin');
  const pkg = crows(page).filter({ hasText: '7z · 3 parts' });
  await expect(pkg).toHaveCount(1);
  await expect(pkg).toHaveAttribute('aria-level', '1');
  await expect(crow(page, 'lone.bin')).toHaveAttribute('aria-level', '1');
  await expect(crow(page, 'm.7z.002')).toHaveAttribute('aria-level', '2');
});

test('offline_flagged_and_cleared', async ({ page }) => {
  await page.goto('/');
  await openCollector(page);
  await paste(page, 'https://x.org/404.bin');
  await expect(crow(page, '404.bin')).toContainText('Offline (404)');
  await page.getByRole('button', { name: 'Clear offline' }).click();
  await expect(crow(page, '404.bin')).toHaveCount(0);
  await expect(toast(page)).toContainText('Cleared 1 offline link');
  await toast(page).getByRole('button', { name: 'Undo' }).click();
  await expect(crow(page, '404.bin')).toHaveCount(1);
});

test('start_package_moves_to_queue', async ({ page }) => {
  await page.goto('/');
  await openCollector(page);
  await paste(page, 'https://x.org/a.bin https://x.org/b.bin');
  await expect(crow(page, 'b.bin')).toContainText('Online');
  await page.getByRole('button', { name: 'Start package' }).click();
  await expect(toast(page)).toContainText('Started 2 downloads');
  await expect(page.getByText('Nothing waiting.', { exact: false })).toBeVisible();
  await page.getByRole('navigation', { name: 'Views' }).getByRole('button', { name: /Queue/ }).click();
  const qrows = page.locator('[aria-label="Downloads"] [role="row"][data-kind]');
  await expect(qrows.filter({ hasText: 'Pasted list · ' })).toHaveCount(1);
  await expect(qrows.filter({ hasText: 'a.bin' })).toHaveCount(1);
  await expect(qrows.filter({ hasText: 'b.bin' })).toHaveCount(1);
});

test('filter_chip_hides_and_persists', async ({ page }) => {
  await page.goto('/?demo=1');
  await openCollector(page);
  await expect(crow(page, 'openimages-v7-sample.csv')).toHaveCount(1);
  await page.getByRole('button', { name: '+ Type' }).click();
  const box = page.getByRole('textbox', { name: 'File type to hide' });
  await box.fill('csv');
  await box.press('Enter');
  await expect(crow(page, 'openimages-v7-sample.csv')).toHaveCount(0);
  await expect(page.getByText(/· 1 hidden/)).toBeVisible();
  await page.getByRole('button', { name: 'under 1 MB' }).click();
  await expect(page.getByRole('button', { name: 'under 1 MB' })).toHaveAttribute('aria-pressed', 'true');
  await page.reload();
  await openCollector(page);
  await expect(page.getByRole('button', { name: '.csv', exact: true })).toHaveAttribute('aria-pressed', 'true');
  await expect(page.getByRole('button', { name: 'under 1 MB' })).toHaveAttribute('aria-pressed', 'true');
  await expect(crow(page, 'openimages-v7-sample.csv')).toHaveCount(0);
});

test('collector_keyboard', async ({ page }) => {
  await page.goto('/');
  await openCollector(page);
  await paste(page, 'https://x.org/a.bin https://x.org/b.bin https://x.org/c.bin');
  await expect(crow(page, 'c.bin')).toContainText('Online');
  const pkg = crows(page).filter({ hasText: 'Pasted list · ' });
  await pkg.click();
  await page.keyboard.press('ArrowDown');
  await expect(crow(page, 'a.bin')).toHaveAttribute('aria-selected', 'true');
  await page.keyboard.press('Delete');
  await expect(crow(page, 'a.bin')).toHaveCount(0);
  await expect(toast(page)).toContainText('Removed a.bin');
  await pkg.click();
  await page.keyboard.press('Enter');
  await expect(toast(page)).toContainText('Started 2 downloads');
});

test('clipboard_toggle', async ({ page }) => {
  await page.goto('/');
  const tog = page.getByRole('button', { name: 'Watch clipboard' });
  await expect(tog).toHaveAttribute('aria-pressed', 'false');
  await tog.click();
  await expect(tog).toHaveAttribute('aria-pressed', 'true');
  const calls = await page.evaluate(() => (window as unknown as { __haulMockCalls: string[] }).__haulMockCalls);
  expect(calls).toContain('setWatchClipboard');
});

test('rail_badge_counts_links', async ({ page }) => {
  await page.goto('/?demo=1');
  await expect(page.getByRole('navigation', { name: 'Views' }).getByRole('button', { name: /Collector/ })).toContainText('9');
});
