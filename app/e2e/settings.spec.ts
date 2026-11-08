import { expect, test, type Page } from '@playwright/test';

type Mock = { __haulMock: { pairRequest(b: string): number } };

async function openSettings(page: Page) {
  await page.getByRole('navigation', { name: 'Views' }).getByRole('button', { name: 'Settings' }).click();
  await expect(page.getByRole('heading', { name: 'Settings', level: 1 })).toBeVisible();
}

const region = (page: Page, name: string) => page.getByRole('region', { name });

test('settings_groups_show', async ({ page }) => {
  await page.goto('/');
  await openSettings(page);
  for (const name of ['Downloads', 'Adding links', 'Schedule', 'App', 'Browsers']) {
    await expect(region(page, name)).toBeVisible();
  }
  await expect(region(page, 'Downloads')).toContainText('C:\\Users\\you\\Downloads');
});

test('downloads_at_once_saves_and_clamps', async ({ page }) => {
  await page.goto('/');
  await openSettings(page);
  const n = page.getByLabel('Downloads at once');
  await expect(n).toHaveValue('3');
  await n.fill('25');
  await n.press('Enter');
  await expect(n).toHaveValue('20');
  await page.reload();
  await openSettings(page);
  await expect(page.getByLabel('Downloads at once')).toHaveValue('20');
});

test('clipboard_switch_matches_status_bar', async ({ page }) => {
  await page.goto('/');
  await openSettings(page);
  const sw = page.getByRole('switch', { name: 'Watch the clipboard' });
  await expect(sw).toHaveAttribute('aria-checked', 'false');
  await page.getByRole('button', { name: /clipboard/i }).click();
  await expect(sw).toHaveAttribute('aria-checked', 'true');
  await sw.click();
  await expect(sw).toHaveAttribute('aria-checked', 'false');
  await expect(page.getByRole('button', { name: /clipboard/i })).toHaveAttribute('aria-pressed', 'false');
});

test('folder_change_and_refusal', async ({ page }) => {
  await page.goto('/');
  await openSettings(page);
  await page.getByRole('button', { name: 'Change…' }).click();
  await expect(region(page, 'Downloads')).toContainText('C:\\Users\\you\\Downloads\\Picked');
  await page.evaluate(() => ((window as unknown as { __haulMockNextFolder: string }).__haulMockNextFolder = 'Z:\\Refused'));
  await page.getByRole('button', { name: 'Change…' }).click();
  await expect(region(page, 'Downloads').getByRole('alert')).toContainText("Haul can't save files there");
  await expect(region(page, 'Downloads')).toContainText('C:\\Users\\you\\Downloads\\Picked');
});

test('schedule_on_off', async ({ page }) => {
  await page.goto('/');
  await openSettings(page);
  const on = page.getByRole('switch', { name: 'Only download at certain times' });
  await on.click();
  await expect(page.getByLabel('Start at', { exact: true })).toBeVisible();
  await expect(page.getByRole('checkbox', { name: 'Mon' })).toBeChecked();
  await on.click();
  await expect(page.getByLabel('Start at', { exact: true })).toBeHidden();
});

test('browsers_has_both_folder_buttons', async ({ page }) => {
  await page.goto('/');
  await openSettings(page);
  const browsers = region(page, 'Browsers');
  await browsers.getByRole('button', { name: 'Open the extension folder' }).click();
  await browsers.getByRole('button', { name: 'Open the Firefox add-on folder' }).click();
  await expect(page.getByText("The Firefox add-on wasn't found. Sign it with npm run sign:firefox in the extension folder.")).toBeVisible();
  const calls = await page.evaluate(() => (window as unknown as { __haulMockCalls: string[] }).__haulMockCalls);
  expect(calls).toContain('openExtensionFolder');
  expect(calls).toContain('openFirefoxFolder');
});

test('browsers_listed_in_settings', async ({ page }) => {
  await page.goto('/');
  await page.evaluate(() => (window as unknown as Mock).__haulMock.pairRequest('Microsoft Edge'));
  await page.getByRole('alertdialog').getByRole('button', { name: 'Allow' }).click();
  await openSettings(page);
  await expect(region(page, 'Browsers').getByRole('listitem')).toHaveCount(1);
  await expect(region(page, 'Browsers')).toContainText('Microsoft Edge');
});
