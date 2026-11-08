import { expect, test, type Page } from '@playwright/test';

const QUESTION = 'Allow Microsoft Edge to send downloads to Haul?';

type Mock = { __haulMock: { pairRequest(b: string): number; pairTimeout(): void } };

async function ask(page: Page, browser: string) {
  await page.evaluate((b) => (window as unknown as Mock).__haulMock.pairRequest(b), browser);
}

async function openRules(page: Page) {
  await page.getByRole('navigation', { name: 'Views' }).getByRole('button', { name: 'Rules' }).click();
  await expect(page.getByRole('heading', { name: 'Rules', level: 1 })).toBeVisible();
}

const browsers = (page: Page) => page.getByRole('region', { name: 'Browsers' });

test('pair_dialog_allow', async ({ page }) => {
  await page.goto('/');
  await ask(page, 'Microsoft Edge');
  const dlg = page.getByRole('alertdialog', { name: QUESTION });
  await expect(dlg).toBeVisible();
  await expect(dlg.getByRole('button', { name: 'Allow' })).toBeFocused();
  await dlg.getByRole('button', { name: 'Allow' }).click();
  await expect(dlg).toBeHidden();
  await openRules(page);
  await expect(browsers(page).getByRole('listitem')).toHaveCount(1);
  await expect(browsers(page).getByRole('listitem').first()).toContainText('Microsoft Edge');
  await expect(browsers(page).getByRole('listitem').first()).toContainText('Paired today');
  await expect(browsers(page).getByRole('listitem').first()).toContainText('Last used never');
});

test('pair_dialog_escape_denies', async ({ page }) => {
  await page.goto('/');
  await ask(page, 'Microsoft Edge');
  const dlg = page.getByRole('alertdialog', { name: QUESTION });
  await expect(dlg).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(dlg).toBeHidden();
  const calls = await page.evaluate(() => (window as unknown as { __haulMockCalls: string[] }).__haulMockCalls);
  expect(calls).toContain('pairAnswer:false');
  await openRules(page);
  await expect(browsers(page).getByRole('listitem')).toHaveCount(0);
});

test('browser_remove', async ({ page }) => {
  await page.goto('/');
  await ask(page, 'Microsoft Edge');
  await page.getByRole('alertdialog').getByRole('button', { name: 'Allow' }).click();
  await openRules(page);
  await browsers(page).getByRole('button', { name: 'Remove Microsoft Edge' }).click();
  await expect(browsers(page).getByRole('listitem')).toHaveCount(0);
  await expect(browsers(page)).toContainText('No browsers paired. Install the Haul extension and press Pair in its popup.');
});

test('dialog_closes_when_request_ends', async ({ page }) => {
  await page.goto('/');
  await ask(page, 'Microsoft Edge');
  const dlg = page.getByRole('alertdialog', { name: QUESTION });
  await expect(dlg).toBeVisible();
  await page.evaluate(() => (window as unknown as Mock).__haulMock.pairTimeout());
  await expect(dlg).toBeHidden();
});
