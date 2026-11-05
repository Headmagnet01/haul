import { expect, test, type Page } from '@playwright/test';

const ISO = 'ubuntu-24.04.2-desktop-amd64.iso';
const ISO_URL = 'https://releases.ubuntu.com/24.04.2/ubuntu-24.04.2-desktop-amd64.iso';
const drawer = (page: Page) => page.getByRole('complementary', { name: 'Details' });

async function selectIso(page: Page) {
  await page.goto('/?demo=1');
  await page.locator('[role="row"][data-kind="file"]').filter({ hasText: ISO }).click();
}

test('nothing_selected_hint', async ({ page }) => {
  await page.goto('/?demo=1');
  await expect(drawer(page)).toContainText('Select a download to see its connections.');
});

test('drawer_shows_selected', async ({ page }) => {
  await selectIso(page);
  await expect(drawer(page).getByRole('heading', { level: 2 })).toHaveText(ISO);
  expect(await drawer(page).locator('.bigmap .cell').count()).toBeGreaterThanOrEqual(1);
  await expect(drawer(page).locator('.sec h3').first()).toContainText(/\d+ connections/);
  await expect(drawer(page)).toContainText('releases.ubuntu.com');
  await expect(drawer(page)).toContainText('Yes (byte ranges, ETag checked)');
});

test('steal_note_appears', async ({ page }) => {
  await selectIso(page);
  await expect(drawer(page).locator('.steal')).toHaveText(/^A connection finished early/, { timeout: 10_000 });
});

test('priority_change_calls_backend', async ({ page }) => {
  await selectIso(page);
  await drawer(page).getByLabel('Priority').selectOption('high');
  await expect
    .poll(() => page.evaluate(() => (window as unknown as { __haulMockCalls: string[] }).__haulMockCalls))
    .toContain('setPriority');
});

test('copy_link', async ({ page, context }) => {
  await context.grantPermissions(['clipboard-read', 'clipboard-write']);
  await selectIso(page);
  await drawer(page).getByRole('button', { name: 'Copy link' }).click();
  await expect(page.getByRole('status')).toContainText('Link copied');
  expect(await page.evaluate(() => navigator.clipboard.readText())).toBe(ISO_URL);
});

test('package_drawer', async ({ page }) => {
  await page.goto('/?demo=1');
  await page.locator('[data-kind="package"]').click();
  await expect(drawer(page).getByRole('heading', { level: 2 })).toHaveText('sprite-fright-4k');
  await expect(drawer(page)).toContainText('5 files');
  await expect(drawer(page).getByRole('button', { name: 'Pause all' })).toBeVisible();
});

test('speed_limit_toggle_persists', async ({ page }) => {
  await page.goto('/?demo=1');
  const tog = page.getByRole('button', { name: /Speed limit/ });
  await expect(tog).toHaveAttribute('aria-pressed', 'false');
  await expect(tog).toContainText('10 MB/s');
  await tog.click();
  await expect(tog).toHaveAttribute('aria-pressed', 'true');
  await page.reload();
  await expect(page.getByRole('button', { name: /Speed limit/ })).toHaveAttribute('aria-pressed', 'true');
});

test('sparkline_draws', async ({ page }) => {
  await page.goto('/?demo=1');
  await page.waitForTimeout(2000);
  const pts = await page.locator('footer polyline').getAttribute('points');
  expect(pts && pts.length > 0).toBe(true);
  await expect(page.locator('footer')).toContainText(/3 active · \d+ waiting/);
});
