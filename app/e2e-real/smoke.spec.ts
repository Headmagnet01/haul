// Real-app smoke run. Needs: `node scripts/range-server.mjs 8765` and the app started with
// WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222 npx tauri dev
import { chromium, expect, test, type Page } from '@playwright/test';
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';

const PORT = Number(process.env.HAUL_SMOKE_PORT ?? 8765);
const URL_ = `http://127.0.0.1:${PORT}/file.bin`;

function expectedSha(): string {
  const size = 64 * 1024 * 1024;
  const b = Buffer.alloc(size);
  for (let i = 0; i < size; i++) b[i] = (i * 31 + 7) % 251;
  return createHash('sha256').update(b).digest('hex');
}

async function appPage(): Promise<Page> {
  const browser = await chromium.connectOverCDP('http://127.0.0.1:9222');
  const page = browser.contexts().flatMap((c) => c.pages()).find((p) => p.url().startsWith('http://localhost:5173'));
  if (!page) throw new Error('Haul window not found on the DevTools port');
  return page;
}

test('real download: add, pause, resume, complete, verify', async () => {
  const page = await appPage();
  await page.evaluate((t) => {
    const dt = new DataTransfer();
    dt.setData('text/plain', t);
    document.body.dispatchEvent(new ClipboardEvent('paste', { clipboardData: dt, bubbles: true, cancelable: true }));
  }, URL_);
  const row = page.locator('[role="row"][data-kind="file"]').filter({ hasText: 'file' }).last();
  await expect(row).toContainText('Downloading', { timeout: 5000 });
  await row.click();

  const pct = row.locator('.pct');
  await expect
    .poll(async () => parseFloat((await pct.textContent()) ?? '0'), { timeout: 30_000, intervals: [100] })
    .toBeGreaterThanOrEqual(30);
  await page.keyboard.press('Space');
  await expect(row).toContainText('Paused');
  const pausedAt = parseFloat((await pct.textContent()) ?? '0');
  await page.keyboard.press('Space');
  await expect(row).toContainText('Downloading');
  await expect(row).toContainText('Complete', { timeout: 60_000 });
  expect(pausedAt).toBeLessThan(100);

  const saved = (await page.locator('aside[aria-label="Details"] dd').nth(3).textContent())?.trim() ?? '';
  expect(saved.toLowerCase().endsWith('.bin')).toBe(true);
  const got = createHash('sha256').update(readFileSync(saved)).digest('hex');
  expect(got).toBe(expectedSha());
});

test('removing a running download leaves no ghost row', async () => {
  const page = await appPage();
  await page.evaluate((t) => {
    const dt = new DataTransfer();
    dt.setData('text/plain', t);
    document.body.dispatchEvent(new ClipboardEvent('paste', { clipboardData: dt, bubbles: true, cancelable: true }));
  }, `${URL_}?remove`);
  const rows = page.locator('[role="row"][data-kind="file"]');
  const row = rows.last();
  await expect(row).toContainText('Downloading', { timeout: 5000 });
  const before = await rows.count();
  const id = await row.getAttribute('data-id');
  await row.click();
  await page.keyboard.press('Delete');
  const mine = page.locator(`[role="row"][data-id="${id}"]`);
  await expect(mine).toHaveCount(0);
  // Undo window (6 s), then the engine stops the transfer and deletes the row.
  const seen: number[] = [];
  for (let i = 0; i < 80; i++) {
    seen.push(await mine.count());
    await page.waitForTimeout(100);
  }
  expect(Math.max(...seen)).toBe(0);
  await expect(rows).toHaveCount(before - 1);
});
