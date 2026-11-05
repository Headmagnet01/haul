import { expect, test, type Page } from '@playwright/test';

const ISO = 'ubuntu-24.04.2-desktop-amd64.iso';
const fileRows = (page: Page) => page.locator('[role="row"][data-kind="file"]');
const calls = (page: Page) => page.evaluate(() => (window as unknown as { __haulMockCalls: string[] }).__haulMockCalls);

async function paste(page: Page, text: string) {
  await page.evaluate((t) => {
    const dt = new DataTransfer();
    dt.setData('text/plain', t);
    document.body.dispatchEvent(new ClipboardEvent('paste', { clipboardData: dt, bubbles: true, cancelable: true }));
  }, text);
}

test('delete_then_undo_restores', async ({ page }) => {
  test.setTimeout(20_000);
  await page.goto('/?demo=1');
  await fileRows(page).filter({ hasText: ISO }).click();
  await page.keyboard.press('Delete');
  await expect(fileRows(page).filter({ hasText: ISO })).toHaveCount(0);
  await expect(page.getByRole('status')).toContainText(`Removed ${ISO}`);
  await page.getByRole('status').getByRole('button', { name: 'Undo' }).click();
  await expect(fileRows(page).filter({ hasText: ISO })).toHaveCount(1);
  await page.waitForTimeout(7000);
  expect(await calls(page)).not.toContain('remove');
  await expect(fileRows(page).filter({ hasText: ISO })).toHaveCount(1);
});

test('delete_commits_after_6s', async ({ page }) => {
  test.setTimeout(20_000);
  await page.goto('/?demo=1');
  await fileRows(page).filter({ hasText: ISO }).click();
  await page.keyboard.press('Delete');
  await page.waitForTimeout(5000);
  expect(await calls(page)).not.toContain('remove');
  await page.waitForTimeout(2000);
  expect(await calls(page)).toContain('remove');
  await expect(fileRows(page).filter({ hasText: ISO })).toHaveCount(0);
});

test('second_delete_commits_the_first', async ({ page }) => {
  await page.goto('/?demo=1');
  await fileRows(page).filter({ hasText: ISO }).click();
  await page.keyboard.press('Delete');
  await fileRows(page).filter({ hasText: 'lecture-07' }).click();
  await page.keyboard.press('Delete');
  await expect.poll(() => calls(page)).toContain('remove');
  await expect(page.getByRole('status')).toContainText('Removed lecture-07-recording.mp4');
});

test('alt_arrow_reorders', async ({ page }) => {
  await page.goto('/');
  for (const n of ['one', 'two', 'three']) {
    await paste(page, `https://a.org/${n}.bin`);
    await expect(fileRows(page).filter({ hasText: `${n}.bin` })).toHaveCount(1);
  }
  await fileRows(page).filter({ hasText: 'three.bin' }).click();
  await page.keyboard.press('Alt+ArrowUp');
  await expect(fileRows(page).nth(1)).toContainText('three.bin');
  await expect(fileRows(page).nth(2)).toContainText('two.bin');
  expect(await calls(page)).toContain('reorder');
  await expect(fileRows(page).filter({ hasText: 'three.bin' })).toHaveAttribute('aria-selected', 'true');
});
