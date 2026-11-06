import { expect, test, type Page } from '@playwright/test';

const toast = (page: Page) => page.getByRole('status');
const rules = (page: Page) => page.getByRole('list', { name: 'Your rules' }).getByRole('listitem');

async function openRules(page: Page) {
  await page.goto('/');
  await page.getByRole('navigation', { name: 'Views' }).getByRole('button', { name: 'Rules' }).click();
  await expect(page.getByRole('heading', { name: 'Rules', level: 1 })).toBeVisible();
}

async function addRule(page: Page, name: string, pattern: string, folder: string) {
  await page.getByRole('button', { name: 'Add rule' }).click();
  const ed = page.getByRole('group', { name: 'Rule editor' });
  await ed.getByLabel('Name', { exact: true }).fill(name);
  await ed.getByLabel('File name', { exact: true }).fill(pattern);
  await ed.getByLabel('Folder', { exact: true }).fill(folder);
  await ed.getByRole('button', { name: 'Save' }).click();
}

test('switches_save', async ({ page }) => {
  await openRules(page);
  const auto = page.getByRole('button', { name: 'Process finished packages automatically' });
  const scan = page.getByRole('button', { name: 'Scan with Windows Defender' });
  await expect(auto).toHaveAttribute('aria-pressed', 'true');
  await expect(page.getByRole('button', { name: 'Sort into category folders' })).toHaveAttribute('aria-pressed', 'true');
  await expect(page.getByRole('button', { name: 'Send archives to the Recycle Bin after extracting' })).toHaveAttribute('aria-pressed', 'false');
  await expect(scan).toHaveAttribute('aria-pressed', 'false');
  await scan.click();
  await expect(scan).toHaveAttribute('aria-pressed', 'true');
  const calls = await page.evaluate(() => (window as unknown as { __haulMockCalls: string[] }).__haulMockCalls);
  expect(calls).toContain('setPostprocSettings');
});

test('categories_show_defaults', async ({ page }) => {
  await openRules(page);
  await expect(page.getByLabel('Video extensions')).toHaveValue(/mp4, mkv/);
  await expect(page.getByLabel('Other folder')).toHaveValue('Other');
});

test('add_rule_and_try_name', async ({ page }) => {
  await openRules(page);
  await expect(page.getByText('No rules yet. Files go to their category folder.')).toBeVisible();
  await addRule(page, 'ISO images', '*.iso', 'Programs\\ISO');
  await expect(toast(page)).toContainText('Rules saved');
  await expect(rules(page)).toHaveCount(1);
  await expect(rules(page).first()).toContainText('*.iso → Programs\\ISO');
  await page.getByLabel('Try a file name').fill('ubuntu.iso');
  await expect(page.locator('.tryout')).toContainText('→ C:\\Users\\you\\Downloads\\Programs\\ISO\\ubuntu.iso (Rule “ISO images”)');
  await page.getByLabel('Try a file name').fill('film.mkv');
  await expect(page.locator('.tryout')).toContainText('→ C:\\Users\\you\\Downloads\\Video\\film.mkv (Video)');
});

test('rule_editor_rejects_bad_regex', async ({ page }) => {
  await openRules(page);
  await addRule(page, 'Bad', '/(/', 'x');
  await expect(toast(page)).toContainText('Bad pattern in “Bad”');
  await expect(page.getByRole('group', { name: 'Rule editor' })).toBeVisible();
  await expect(rules(page)).toHaveCount(0);
});

test('reorder_rules_with_buttons', async ({ page }) => {
  await openRules(page);
  await addRule(page, 'Alpha', '*.a', 'A');
  await addRule(page, 'Beta', '*.b', 'B');
  await expect(rules(page)).toHaveCount(2);
  await rules(page).first().getByRole('button', { name: 'Move down' }).click();
  await expect(rules(page).first()).toContainText('Beta');
  await expect(rules(page).nth(1)).toContainText('Alpha');
  await rules(page).first().getByRole('button', { name: 'Delete' }).click();
  await expect(rules(page)).toHaveCount(1);
});

test('add_and_remove_password_masked', async ({ page }) => {
  await openRules(page);
  await page.getByLabel('New password').fill('hunter2-secret');
  await page.getByRole('button', { name: 'Add password' }).click();
  await expect(toast(page)).toContainText('Password saved');
  const list = page.getByRole('list', { name: 'Saved passwords' }).getByRole('listitem');
  await expect(list).toHaveCount(1);
  await expect(list.first()).toContainText('•••••••• (1)');
  expect(await page.locator('body').innerText()).not.toContain('hunter2-secret');
  await expect(page.getByLabel('New password')).toHaveValue('');
  await list.first().getByRole('button', { name: 'Delete' }).click();
  await expect(list).toHaveCount(0);
});
