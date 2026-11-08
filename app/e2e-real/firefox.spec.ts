// The Firefox add-on → Haul, for real. Needs, as for extension.spec.ts:
//   node scripts/range-server.mjs 8765
//   WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222 npx tauri dev
// and the extension built (`npm run build` in extension/), which makes extension/dist-firefox.
//
// The installed Firefox is driven over WebDriver BiDi (see firefox-bidi.ts) with a throwaway profile
// and a test copy of the add-on installed temporarily. Playwright still drives Haul's own window.
// Under BiDi, Firefox never even requests a navigation whose path ends in .bin or .zip (seen with
// Firefox 159 on 9 Nov 2026; the same URL with a query string goes through), so every download URL
// here carries ?t=<stamp>.
// A Haul already running on 17654 from an older build refuses Firefox's origin, so the add-on
// skips it and finds the dev build on the next port.
import { chromium, expect, test, type Page } from '@playwright/test';
import { mkdtempSync, rmSync } from 'node:fs';
import { createServer, type Server } from 'node:http';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { EXT_ORIGIN, Firefox, type Tab } from './firefox-bidi';

const HERE = dirname(fileURLToPath(import.meta.url));
const EXT = resolve(HERE, '../../extension/dist-firefox');
const BIDI_PORT = 9335;
const PAGE_PORT = 8767;
const PORT = Number(process.env.HAUL_SMOKE_PORT ?? 8765);
const STAMP = Date.now();
const SMALL = Buffer.alloc(10 * 1024, 7);

test.describe.configure({ mode: 'serial' });

let ff: Firefox;
let profile: string;
let pages: Server;
/** An add-on page, where chrome.* is available to the test. */
let addon: Tab;

async function haulPage(): Promise<Page> {
  const browser = await chromium.connectOverCDP('http://127.0.0.1:9222');
  const page = browser.contexts().flatMap((c) => c.pages()).find((p) => p.url().startsWith('http://localhost:5173'));
  if (!page) throw new Error('Haul window not found on the DevTools port');
  return page;
}

const pageFor = (href: string) => `http://127.0.0.1:${PAGE_PORT}/?href=${encodeURIComponent(href)}`;
const downloads = (url: string) =>
  addon.eval<{ state: string }[]>(`chrome.downloads.search({ url: ${JSON.stringify(url)} }).then((l) => l.map((d) => ({ state: d.state })))`);

test.beforeAll(async () => {
  // A page whose link clicks itself, and a small file Haul should leave to Firefox.
  pages = createServer((req, res) => {
    const u = new URL(req.url ?? '/', 'http://x');
    if (u.pathname.startsWith('/small-')) {
      res.writeHead(200, { 'Content-Type': 'application/octet-stream', 'Content-Length': SMALL.length });
      res.end(SMALL);
      return;
    }
    const href = u.searchParams.get('href') ?? '';
    res.writeHead(200, { 'Content-Type': 'text/html' });
    res.end(`<!doctype html><title>Test page</title><a id="dl" href="${href}">Get the file</a>
      <script>setTimeout(() => document.getElementById('dl')?.click(), 300)</script>`);
  });
  await new Promise<void>((ok) => pages.listen(PAGE_PORT, '127.0.0.1', () => ok()));
  profile = mkdtempSync(join(tmpdir(), 'haul-ff-'));
  ff = await Firefox.launch({ profileDir: profile, port: BIDI_PORT });
  await ff.installTemporary(Firefox.testCopy(EXT, join(profile, 'addon')));
  addon = await ff.openTab(`http://127.0.0.1:${PAGE_PORT}/`);
  await addon.openAddonPage(`${EXT_ORIGIN}/popup.html`);
});

test.afterAll(async () => {
  await ff?.close();
  pages?.close();
  await new Promise((r) => setTimeout(r, 1000));
  rmSync(profile, { recursive: true, force: true, maxRetries: 5, retryDelay: 500 });
});

test('firefox_pair_and_capture', async () => {
  const state = () => addon.eval<string>("document.getElementById('state')?.textContent ?? ''").catch(() => '');
  await expect.poll(state, { timeout: 15_000 }).toBe('Not paired');
  await addon.eval("document.getElementById('pair').click()");

  const haul = await haulPage();
  const dlg = haul.getByRole('alertdialog', { name: /Allow Firefox to send downloads to Haul\?/ });
  await expect(dlg).toBeVisible({ timeout: 10_000 });
  await dlg.getByRole('button', { name: 'Allow' }).click();
  await expect.poll(state, { timeout: 10_000 }).toBe('Paired with Haul');

  const name = `ff-${STAMP}.bin`;
  const url = `http://127.0.0.1:${PORT}/${name}?t=${STAMP}`;
  await ff.openTab(pageFor(url));
  const row = haul.locator('[role="row"][data-kind="file"]').filter({ hasText: `ff-${STAMP}` });
  await expect(row)
    .toBeVisible({ timeout: 15_000 })
    .catch(async (e) => {
      const all = await addon.eval('chrome.downloads.search({}).then((l) => JSON.stringify(l.map((d) => [d.url, d.state, d.fileSize, d.totalBytes, d.filename, d.error])))');
      const last = await addon.eval('chrome.storage.local.get(null).then((v) => JSON.stringify(v))');
      throw new Error(`${e}\nFirefox downloads: ${all}\nadd-on storage: ${last}`);
    });
  // Firefox's own copy was cancelled and erased.
  await expect.poll(() => downloads(url), { timeout: 10_000 }).toEqual([]);
  // A completed download leaves Queue for Finished.
  await expect(row).toHaveCount(0, { timeout: 90_000 });
  await haul.getByRole('navigation', { name: 'Views' }).getByRole('button', { name: /Finished/ }).click();
  const done = haul.getByRole('treegrid', { name: 'Finished downloads' }).locator('[role="row"][data-kind]').filter({ hasText: `ff-${STAMP}` });
  await expect(done).toHaveCount(1, { timeout: 10_000 });
  await haul.getByRole('navigation', { name: 'Views' }).getByRole('button', { name: /Queue/ }).click();
});

test('firefox_small_file_stays', async () => {
  const url = `http://127.0.0.1:${PAGE_PORT}/small-${STAMP}.bin?t=${STAMP}`;
  await ff.openTab(pageFor(url));
  await expect.poll(async () => (await downloads(url))[0]?.state ?? 'none', { timeout: 15_000 }).toBe('complete');
  const haul = await haulPage();
  await expect(haul.locator('[role="row"][data-kind="file"]').filter({ hasText: `small-${STAMP}` })).toHaveCount(0);
});

test('firefox_kept_when_unpaired', async () => {
  await addon.eval("chrome.storage.local.set({ token: 'bogus' })");
  const name = `ffkept-${STAMP}.bin`;
  const url = `http://127.0.0.1:${PORT}/${name}?t=${STAMP}`;
  await ff.openTab(pageFor(url));
  // Haul refused the old key, so Firefox keeps the download.
  await expect.poll(async () => (await downloads(url))[0]?.state ?? 'none', { timeout: 15_000 }).toBe('in_progress');
  const haul = await haulPage();
  await expect(haul.locator('[role="row"][data-kind="file"]').filter({ hasText: `ffkept-${STAMP}` })).toHaveCount(0);
  await addon.eval(`chrome.downloads.search({ url: ${JSON.stringify(url)} }).then(([d]) => d && chrome.downloads.cancel(d.id))`);
});
