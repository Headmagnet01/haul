import AxeBuilder from '@axe-core/playwright';
import { expect, test, type Page } from '@playwright/test';

const nav = (page: Page, name: RegExp) => page.getByRole('navigation', { name: 'Views' }).getByRole('button', { name });
const queue = (page: Page) => page.getByRole('treegrid', { name: 'Downloads' });
const finished = (page: Page) => page.getByRole('treegrid', { name: 'Finished downloads' });
const rows = (page: Page) => finished(page).locator('[role="row"][data-kind]');
const row = (page: Page, text: string) => rows(page).filter({ hasText: text });
const drawer = (page: Page) => page.getByRole('complementary', { name: 'Details' });

async function openFinished(page: Page) {
  await page.goto('/?demo=1');
  await expect(queue(page)).toBeVisible();
  await nav(page, /Finished/).click();
  await expect(page.getByRole('heading', { name: 'Finished', level: 1 })).toBeVisible();
}

test('finished_package_leaves_queue', async ({ page }) => {
  await page.goto('/?demo=1');
  await expect(queue(page)).toContainText('sprite-fright-4k');
  await expect(queue(page)).not.toContainText('holiday-photos');
  await expect(queue(page)).not.toContainText('handbook-2026.pdf');
  await nav(page, /Finished/).click();
  await expect(row(page, 'holiday-photos')).toHaveCount(1);
  await expect(row(page, 'handbook-2026.pdf')).toHaveCount(1);
  await expect(finished(page)).not.toContainText('sprite-fright-4k');
  // Collapsed by default; the files show once it is opened.
  await expect(row(page, 'holiday-photos.part1.rar')).toHaveCount(0);
  await row(page, 'holiday-photos').getByRole('button', { name: 'Expand package' }).click();
  await expect(row(page, 'holiday-photos.part1.rar')).toHaveCount(1);
});

test('finished_rail_count_matches', async ({ page }) => {
  await openFinished(page);
  // 2 package files + 2 lone files.
  await expect(nav(page, /Finished/)).toContainText('4');
  await expect(page.locator('.top .sub')).toHaveText('4 files');
});

test('finished_time_and_location', async ({ page }) => {
  await openFinished(page);
  await expect(row(page, 'holiday-photos')).toContainText('09:44');
  await expect(row(page, 'handbook-2026.pdf')).toContainText(/\d{1,2} [A-Z][a-z]{2} \d{4}/);
  await expect(row(page, 'handbook-2026.pdf')).toContainText('C:\\Users\\you\\Downloads');
});

test('remove_from_list_with_undo', async ({ page }) => {
  await openFinished(page);
  await row(page, 'handbook-2026.pdf').click();
  await page.keyboard.press('Delete');
  await expect(row(page, 'handbook-2026.pdf')).toHaveCount(0);
  await expect(page.getByRole('status')).toContainText('Removed handbook-2026.pdf');
  await page.getByRole('status').getByRole('button', { name: 'Undo' }).click();
  await expect(row(page, 'handbook-2026.pdf')).toHaveCount(1);
});

test('delete_file_confirms', async ({ page }) => {
  await openFinished(page);
  await row(page, 'handbook-2026.pdf').click();
  await page.keyboard.press('Shift+Delete');
  const dlg = page.getByRole('alertdialog');
  await expect(dlg).toContainText('Move handbook-2026.pdf to the Recycle Bin?');
  await expect(dlg.getByRole('button', { name: 'Cancel' })).toBeFocused();
  await page.keyboard.press('Escape');
  await expect(dlg).toHaveCount(0);
  await expect(row(page, 'handbook-2026.pdf')).toHaveCount(1);
  await drawer(page).getByRole('button', { name: /Delete file/ }).click();
  await dlg.getByRole('button', { name: 'Delete file' }).click();
  await expect(row(page, 'handbook-2026.pdf')).toHaveCount(0);
  await expect(page.getByRole('status')).toContainText('Recycle Bin');
  expect(await page.evaluate(() => (window as unknown as { __haulMockCalls: string[] }).__haulMockCalls)).toContain('deleteFiles');
});

test('missing_file_disables_open', async ({ page }) => {
  await openFinished(page);
  await expect(row(page, 'old-setup.exe')).toContainText('Missing');
  await row(page, 'old-setup.exe').click();
  await expect(drawer(page).getByRole('button', { name: /^Open/ }).first()).toBeDisabled();
  await expect(drawer(page).getByRole('button', { name: 'Open folder' })).toBeDisabled();
  await row(page, 'handbook-2026.pdf').click();
  await expect(drawer(page).getByRole('button', { name: /^Open/ }).first()).toBeEnabled();
  await page.keyboard.press('Enter');
  expect(await page.evaluate(() => (window as unknown as { __haulMockCalls: string[] }).__haulMockCalls)).toContainEqual(
    expect.stringMatching(/^openFile:/),
  );
});

test('download_again_adds_to_queue', async ({ page }) => {
  await openFinished(page);
  await row(page, 'handbook-2026.pdf').click();
  await drawer(page).getByRole('button', { name: 'Download again' }).click();
  await expect(page.getByRole('status')).toContainText('Added handbook-2026.pdf to the Queue');
  await nav(page, /Queue/).click();
  await expect(queue(page)).toContainText('handbook-2026.pdf');
});

test('finished_has_no_violations', async ({ page }) => {
  await openFinished(page);
  await row(page, 'handbook-2026.pdf').click();
  await page.waitForTimeout(300);
  const r = await new AxeBuilder({ page }).withRules(['color-contrast']).analyze();
  const where = r.violations.flatMap((v) => v.nodes.map((n) => `${n.target.join(' ')}: ${n.failureSummary}`));
  expect(where).toEqual([]);
  const roles = await new AxeBuilder({ page }).include('main').withTags(['wcag2a', 'wcag2aa']).analyze();
  expect(roles.violations.map((v) => `${v.id}: ${v.nodes.length}`)).toEqual([]);
});
