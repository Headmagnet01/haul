import AxeBuilder from '@axe-core/playwright';
import { expect, test, type Page } from '@playwright/test';

type Mock = { __haulMock: { pairRequest(b: string): number } };

const welcome = (page: Page) => page.getByRole('region', { name: 'Welcome to Haul' });
const step = (page: Page, n: number) => welcome(page).getByRole('listitem').nth(n);

test('welcome_shows_three_steps', async ({ page }) => {
  await page.goto('/');
  await expect(welcome(page)).toBeVisible();
  await expect(welcome(page).getByRole('listitem')).toHaveCount(3);
  await expect(step(page, 0)).toContainText('Copy a link or drop one here');
  await expect(step(page, 1)).toContainText('nothing is sent anywhere');
  await expect(step(page, 2)).toContainText('Pair your browser');
  for (let i = 0; i < 3; i++) await expect(step(page, i)).not.toContainText('Done');
  await expect(welcome(page).getByRole('button', { name: 'Open the extension folder' })).toBeVisible();
});

const FF_HELP = 'Drag haul-firefox.xpi onto Firefox, choose Add, then click the Haul button and Pair with Haul.';
const FF_MISSING = "The Firefox add-on wasn't found. Sign it with npm run sign:firefox in the extension folder.";
const mockCalls = (page: Page) => page.evaluate(() => (window as unknown as { __haulMockCalls: string[] }).__haulMockCalls);

test('firefox_folder_button', async ({ page }) => {
  await page.goto('/');
  await expect(step(page, 2)).toContainText(FF_HELP);
  // The mock has no signed add-on, like a fresh dev checkout, so the error shows as a toast.
  await step(page, 2).getByRole('button', { name: 'Open the Firefox add-on folder' }).click();
  await expect(page.getByText(FF_MISSING)).toBeVisible();
  expect((await mockCalls(page)).filter((c) => c === 'openFirefoxFolder')).toHaveLength(1);
});

test('clipboard_switch_ticks_step_two', async ({ page }) => {
  await page.goto('/');
  const sw = step(page, 1).getByRole('switch', { name: 'Watch clipboard' });
  await expect(sw).toHaveAttribute('aria-checked', 'false');
  await sw.click();
  await expect(sw).toHaveAttribute('aria-checked', 'true');
  await expect(step(page, 1)).toContainText('Done');
  // The status bar's toggle is the same setting.
  await expect(page.getByRole('button', { name: /Watch clipboard/ })).toHaveAttribute('aria-pressed', 'true');
});

test('pairing_ticks_step_three', async ({ page }) => {
  await page.goto('/');
  await page.evaluate(() => (window as unknown as Mock).__haulMock.pairRequest('Microsoft Edge'));
  await page.getByRole('alertdialog').getByRole('button', { name: 'Allow' }).click();
  await expect(step(page, 2)).toContainText('Paired with Microsoft Edge');
  await expect(step(page, 2)).toContainText('Done');
});

test('first_link_ends_welcome_for_good', async ({ page }) => {
  await page.goto('/');
  await expect(welcome(page)).toBeVisible();
  await page.getByRole('button', { name: /Add links/ }).click();
  await page.getByLabel('Links to add').fill('https://x.org/first.bin');
  await page.getByRole('button', { name: 'Add', exact: true }).click();
  await expect(welcome(page)).toBeHidden();
  // Start it from the Collector, remove it from the Queue: the plain empty state, not the welcome.
  const views = page.getByRole('navigation', { name: 'Views' });
  await views.getByRole('button', { name: /Collector/ }).click();
  const link = page.locator('[aria-label="Collector links"] [role="row"][data-kind="link"]').filter({ hasText: 'first.bin' });
  await expect(link).toContainText('Online', { timeout: 15_000 });
  await link.getByRole('button', { name: 'Start' }).click();
  await views.getByRole('button', { name: /Queue/ }).click();
  const row = page.locator('[role="row"][data-kind="file"]').filter({ hasText: 'first.bin' });
  await row.click();
  await page.keyboard.press('Delete');
  await expect(row).toHaveCount(0);
  // Remove is sent after the 6 s Undo window.
  await expect(page.getByText('Copy a link or drop one here')).toBeVisible({ timeout: 10_000 });
  await expect(welcome(page)).toBeHidden();
  await page.reload();
  await expect(page.getByText('Copy a link or drop one here')).toBeVisible();
  await expect(welcome(page)).toBeHidden();
});

test('welcome_has_no_violations', async ({ page }) => {
  await page.goto('/');
  await expect(welcome(page)).toBeVisible();
  await page.waitForTimeout(300);
  const r = await new AxeBuilder({ page }).include('main').withTags(['wcag2a', 'wcag2aa']).analyze();
  expect(r.violations.flatMap((v) => v.nodes.map((n) => `${v.id} ${n.target.join(' ')}: ${n.failureSummary}`))).toEqual([]);
});
