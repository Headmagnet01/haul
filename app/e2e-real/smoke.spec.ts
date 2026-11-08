// Real-app smoke run. Needs: `node scripts/range-server.mjs 8765` and the app started with
// WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222 npx tauri dev
import { chromium, expect, test, type Page } from '@playwright/test';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

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

type Invoke = (cmd: string, args?: Record<string, unknown>) => Promise<unknown>;
const invoke = (page: Page, cmd: string, args?: Record<string, unknown>) =>
  page.evaluate(
    ([c, a]) => (window as unknown as { __TAURI_INTERNALS__: { invoke: Invoke } }).__TAURI_INTERNALS__.invoke(c as string, a as Record<string, unknown>),
    [cmd, args] as const,
  );
const isVisible = (page: Page) => invoke(page, 'plugin:window|is_visible', { label: 'main' }) as Promise<boolean>;

const nav = (page: Page, name: RegExp) => page.getByRole('navigation', { name: 'Views' }).getByRole('button', { name });
const collectorRow = (page: Page, name: string) =>
  page.locator('[aria-label="Collector links"] [role="row"][data-kind="link"]').filter({ hasText: name }).last();

/** Pastes a link, which lands in the Collector, checks it online, starts it, and returns to the Queue. */
async function sendToQueue(page: Page, url: string, name: string) {
  await page.evaluate((t) => {
    const dt = new DataTransfer();
    dt.setData('text/plain', t);
    document.body.dispatchEvent(new ClipboardEvent('paste', { clipboardData: dt, bubbles: true, cancelable: true }));
  }, url);
  await nav(page, /Collector/).click();
  const row = collectorRow(page, name);
  await expect(row).toContainText('Online', { timeout: 10_000 });
  await row.getByRole('button', { name: 'Start' }).click();
  await expect(page.getByRole('status')).toContainText('Started 1 download');
  await nav(page, /Queue/).click();
}

test('real download: add, pause, resume, complete, verify', async () => {
  const page = await appPage();
  await sendToQueue(page, URL_, 'file.bin');
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
  expect(pausedAt).toBeLessThan(100);
  // A finished download leaves the Queue for Finished, so follow it by id: complete, then its
  // post-processing run done (the sort step moves it into Other\), then read where it is now.
  const id = Number(await row.getAttribute('data-id'));
  const mine = async () => {
    const s = (await invoke(page, 'snapshot')) as { downloads: { id: number; state: string; targetPath: string | null }[] };
    return s.downloads.find((d) => d.id === id);
  };
  await expect.poll(async () => (await mine())?.state, { timeout: 60_000 }).toBe('completed');
  await expect
    .poll(async () => {
      const runs = (await invoke(page, 'postproc_runs')) as { downloadId: number | null; state: string }[];
      return runs.find((r) => r.downloadId === id)?.state;
    }, { timeout: 30_000 })
    .toBe('done');

  const saved = (await mine())?.targetPath ?? '';
  expect(saved.toLowerCase().endsWith('.bin')).toBe(true);
  const got = createHash('sha256').update(readFileSync(saved)).digest('hex');
  expect(got).toBe(expectedSha());
});

test('removing a running download leaves no ghost row', async () => {
  const page = await appPage();
  await sendToQueue(page, `${URL_}?remove`, 'file.bin');
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

test('a copied link reaches the Collector while Watch clipboard is on', async () => {
  const page = await appPage();
  const tog = page.getByRole('button', { name: 'Watch clipboard' });
  if ((await tog.getAttribute('aria-pressed')) !== 'true') await tog.click();
  await expect(tog).toHaveAttribute('aria-pressed', 'true');
  const url = `http://127.0.0.1:${PORT}/clip-${Date.now()}.bin`;
  const name = url.split('/').pop()!;
  try {
    execFileSync('powershell', ['-NoProfile', '-Command', `Set-Clipboard -Value '${url}'`]);
    await nav(page, /Collector/).click();
    const row = collectorRow(page, name);
    await expect(row).toContainText('Online', { timeout: 5000 });
    await expect(row).toContainText('Clipboard');
    await row.click();
    await page.keyboard.press('Delete');
    await expect(row).toHaveCount(0);
  } finally {
    await tog.click();
    await expect(tog).toHaveAttribute('aria-pressed', 'false');
    await nav(page, /Queue/).click();
  }
});

// The app's save folder; the dev DB's `download_dir` points here (see handoff.md).
const SMOKE_DIR = process.env.HAUL_SMOKE_DIR ?? resolve(dirname(fileURLToPath(import.meta.url)), '../../_tmp/smoke');

test('a split 7z package downloads, extracts and sorts', async () => {
  const page = await appPage();
  const started = Date.now();
  // A leftover "split" set from an earlier run would join this one (archive sets group across pastes).
  await nav(page, /Collector/).click();
  const leftovers = page.locator('[aria-label="Collector links"] [role="row"][data-kind="package"]').filter({ hasText: 'split' });
  if ((await leftovers.count()) > 0) {
    while ((await leftovers.count()) > 0) await leftovers.first().getByRole('button', { name: /^Remove / }).click();
    await page.waitForTimeout(6500); // the Undo window, then the removal is committed
  }
  const urls = [1, 2, 3].map((n) => `http://127.0.0.1:${PORT}/fixtures/split.7z.00${n}?run=${started}`);
  await page.evaluate((t) => {
    const dt = new DataTransfer();
    dt.setData('text/plain', t);
    document.body.dispatchEvent(new ClipboardEvent('paste', { clipboardData: dt, bubbles: true, cancelable: true }));
  }, urls.join('\n'));
  await nav(page, /Collector/).click();
  const pkg = page.locator('[aria-label="Collector links"] [role="row"][data-kind="package"]').filter({ hasText: 'split' }).last();
  await expect(pkg).toContainText('3 / 3 online', { timeout: 15_000 });
  await nav(page, /Queue/).click();
  const qrows = page.locator('[role="row"][data-kind="package"]').filter({ hasText: 'split' });
  const before = new Set(await qrows.evaluateAll((rs) => rs.map((r) => r.getAttribute('data-id'))));
  await nav(page, /Collector/).click();
  await pkg.getByRole('button', { name: 'Start package' }).click();
  await nav(page, /Queue/).click();
  // Earlier runs left "split" packages in the Queue; pick the one this run made.
  let fresh: string | null = null;
  await expect
    .poll(async () => {
      const ids = await qrows.evaluateAll((rs) => rs.map((r) => r.getAttribute('data-id')));
      fresh = ids.find((id) => !before.has(id)) ?? null;
      return fresh;
    }, { timeout: 10_000 })
    .not.toBeNull();
  await page.locator(`[role="row"][data-kind="package"][data-id="${fresh}"]`).click();
  await expect(page.locator('.steps li[data-step="notify"]')).toHaveAttribute('data-state', 'done', { timeout: 60_000 });
  await expect(page.locator('.steps li[data-step="extract"]')).toContainText('Extracted to');
  // A folder made by this run: Other\split, or Other\split (n) when an earlier run left one.
  const other = join(SMOKE_DIR, 'Other');
  const made = readdirSync(other).filter(
    (n) => /^split( \(\d+\))?$/.test(n) && statSync(join(other, n)).birthtimeMs >= started - 1000,
  );
  expect(made).toHaveLength(1);
  expect(readFileSync(join(other, made[0], 'hello.txt'), 'utf8')).toBe('hello haul\n');
});

test('close_keeps_downloading', async () => {
  const page = await appPage();
  await invoke(page, 'set_settings', { patch: { closeToTray: true, speedLimit: { enabled: true, bps: 2_000_000 } } });
  try {
    const name = `close-${Date.now()}`;
    await sendToQueue(page, `http://127.0.0.1:${PORT}/${name}.bin`, name);
    const written = async () => {
      const s = (await invoke(page, 'snapshot')) as { downloads: { name: string; written: number; state: string }[] };
      return s.downloads.find((d) => d.name.startsWith(name));
    };
    await expect.poll(async () => (await written())?.state, { timeout: 10_000 }).toBe('running');
    // Like the X button: CloseRequested, which Haul turns into hide-to-tray.
    await invoke(page, 'plugin:window|close', { label: 'main' });
    await expect.poll(() => isVisible(page), { timeout: 5_000 }).toBe(false);
    const before = (await written())!.written;
    await page.waitForTimeout(3_000);
    const after = await written();
    expect(after!.state).toBe('running');
    expect(after!.written).toBeGreaterThan(before);
    const s = (await invoke(page, 'snapshot')) as { downloads: { id: number; name: string }[] };
    await invoke(page, 'remove', { ids: [s.downloads.find((d) => d.name.startsWith(name))!.id] });
  } finally {
    await invoke(page, 'set_settings', { patch: { speedLimit: { enabled: false, bps: 2_000_000 } } });
  }
});

test('second_launch_shows_first', async () => {
  const page = await appPage();
  if (await isVisible(page)) {
    await invoke(page, 'plugin:window|close', { label: 'main' });
    await expect.poll(() => isVisible(page), { timeout: 5_000 }).toBe(false);
  }
  const exe = resolve(dirname(fileURLToPath(import.meta.url)), '../../target/debug/haul.exe');
  const t = Date.now();
  // Throws if the second copy doesn't exit within 5 s.
  execFileSync(exe, [], { timeout: 5_000, stdio: 'ignore' });
  expect(Date.now() - t).toBeLessThan(5_000);
  await expect.poll(() => isVisible(page), { timeout: 5_000 }).toBe(true);
  const port = readFileSync(join(process.env.APPDATA ?? '', 'dev.haul.desktop', 'api-port'), 'utf8').trim();
  const listening = execFileSync('netstat', ['-ano', '-p', 'TCP'], { encoding: 'utf8' })
    .split('\n')
    .filter((l) => l.includes(`127.0.0.1:${port} `) && l.includes('LISTENING'));
  expect(listening).toHaveLength(1);
});
