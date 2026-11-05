import { expect, test } from '@playwright/test';

test('shell_renders', async ({ page }) => {
  await page.goto('/');
  const nav = page.getByRole('navigation', { name: 'Views' });
  await expect(nav).toBeVisible();
  for (const v of ['Queue', 'Collector', 'Finished', 'Rules', 'Settings']) {
    await expect(nav.getByRole('button', { name: v })).toBeVisible();
  }
  await expect(nav.getByRole('button', { name: 'Queue' })).toHaveAttribute('aria-current', 'page');
  await nav.getByRole('button', { name: 'Rules' }).click();
  await expect(page.getByRole('heading', { name: 'Rules' })).toBeVisible();
  await expect(page.getByText('Arrives in a later version.')).toBeVisible();
  const font = await page.evaluate(() => getComputedStyle(document.body).fontFamily);
  expect(font.startsWith('"IBM Plex Sans"')).toBe(true);
});
