// Browser extension → Haul, for real. Needs, as for smoke.spec.ts:
//   node scripts/range-server.mjs 8765
//   WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222 npx tauri dev
// and the extension built (`npm run build` in extension/).
//
// Edge is driven over the raw DevTools protocol, not Playwright: Playwright takes over browser
// downloads (even when only attached), and in Edge they then fail before any request is sent.
// Playwright still drives Haul's own window.
import { chromium, expect, test, type Page } from '@playwright/test';
import { spawn, type ChildProcess } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { createServer, type Server } from 'node:http';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = dirname(fileURLToPath(import.meta.url));
const EXT = resolve(HERE, '../../extension/dist');
const EDGE = 'C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe';
const CDP_PORT = 9333;
const PAGE_PORT = 8766;
const PORT = Number(process.env.HAUL_SMOKE_PORT ?? 8765);
const STAMP = Date.now();

test.describe.configure({ mode: 'serial' });

/** One DevTools target (a page or the extension's worker), spoken to over its WebSocket. */
class Target {
  private next = 1;
  private waiting = new Map<number, (v: unknown) => void>();
  private constructor(private ws: WebSocket) {
    ws.addEventListener('message', (e) => {
      const m = JSON.parse(String(e.data));
      if (m.id) this.waiting.get(m.id)?.(m);
    });
  }
  static async open(url: string): Promise<Target> {
    const ws = new WebSocket(url);
    await new Promise((ok, bad) => {
      ws.addEventListener('open', ok);
      ws.addEventListener('error', bad);
    });
    return new Target(ws);
  }
  private send(method: string, params: object = {}): Promise<unknown> {
    const id = this.next++;
    this.ws.send(JSON.stringify({ id, method, params }));
    return new Promise((ok) => this.waiting.set(id, ok));
  }
  /** Runs `expr` (awaited) and returns its JSON value. */
  async eval<T>(expr: string): Promise<T> {
    const r = (await this.send('Runtime.evaluate', { expression: expr, awaitPromise: true, returnByValue: true })) as {
      result?: { result?: { value?: T }; exceptionDetails?: { text: string; exception?: { description?: string } } };
    };
    const ex = r.result?.exceptionDetails;
    if (ex) throw new Error(ex.exception?.description ?? ex.text);
    return r.result?.result?.value as T;
  }
  close() {
    this.ws.close();
  }
}

type Listed = { id: string; type: string; url: string; webSocketDebuggerUrl: string };
const list = async (): Promise<Listed[]> => (await fetch(`http://127.0.0.1:${CDP_PORT}/json/list`)).json();

let edge: ChildProcess;
let profile: string;
let pages: Server;
let worker: Target | undefined;
let extId = '';

/** Opens `url` in a new Edge tab and returns its target. */
async function openTab(url: string): Promise<Target> {
  const r = await fetch(`http://127.0.0.1:${CDP_PORT}/json/new?${encodeURI(url)}`, { method: 'PUT' });
  const t = (await r.json()) as Listed;
  return Target.open(t.webSocketDebuggerUrl);
}

async function haulPage(): Promise<Page> {
  const browser = await chromium.connectOverCDP('http://127.0.0.1:9222');
  const page = browser.contexts().flatMap((c) => c.pages()).find((p) => p.url().startsWith('http://localhost:5173'));
  if (!page) throw new Error('Haul window not found on the DevTools port');
  return page;
}

function expectedSha(): string {
  const size = 64 * 1024 * 1024;
  const b = Buffer.alloc(size);
  for (let i = 0; i < size; i++) b[i] = (i * 31 + 7) % 251;
  return createHash('sha256').update(b).digest('hex');
}

/** The browser's own download list for `url`, as Haul's worker sees it. */
const browserDownloads = (url: string) =>
  worker!.eval<{ state: string }[]>(`chrome.downloads.search({ url: ${JSON.stringify(url)} })`);

test.beforeAll(async () => {
  // A page whose link clicks itself: a real navigation that the browser turns into a download.
  pages = createServer((req, res) => {
    const href = new URL(req.url ?? '/', 'http://x').searchParams.get('href') ?? '';
    res.writeHead(200, { 'Content-Type': 'text/html' });
    res.end(`<!doctype html><title>Test page</title><a id="dl" href="${href}">Get the file</a>
      <script>setTimeout(() => document.getElementById('dl').click(), 300)</script>`);
  });
  await new Promise<void>((ok) => pages.listen(PAGE_PORT, '127.0.0.1', () => ok()));

  profile = mkdtempSync(join(tmpdir(), 'haul-ext-'));
  edge = spawn(EDGE, [
    `--remote-debugging-port=${CDP_PORT}`,
    `--user-data-dir=${profile}`,
    `--disable-extensions-except=${EXT}`,
    `--load-extension=${EXT}`,
    '--no-first-run',
    '--no-default-browser-check',
    'about:blank',
  ]);
  for (let i = 0; i < 80 && !worker; i++) {
    const workers = await list().then((l) => l.filter((t) => t.type === 'service_worker'), () => [] as Listed[]);
    for (const w of workers) {
      const t = await Target.open(w.webSocketDebuggerUrl);
      const name = await t.eval<string>('chrome.runtime.getManifest().name').catch(() => '');
      if (name === 'Haul' && !worker) {
        worker = t;
        extId = new URL(w.url).host;
      } else t.close();
    }
    if (!worker) await new Promise((r) => setTimeout(r, 250));
  }
  if (!worker) throw new Error('the Haul extension did not load in Edge (is --load-extension refused?)');
});

test.afterAll(async () => {
  worker?.close();
  edge?.kill();
  pages?.close();
  await new Promise((r) => setTimeout(r, 1000));
  rmSync(profile, { recursive: true, force: true, maxRetries: 5, retryDelay: 500 });
});

test('pair_and_capture_a_click', async () => {
  const popup = await openTab(`chrome-extension://${extId}/popup.html`);
  const state = () => popup.eval<string>("document.getElementById('state')?.textContent ?? ''").catch(() => '');
  await expect.poll(state, { timeout: 10_000 }).toBe('Not paired');
  await popup.eval("document.getElementById('pair').click()");

  const haul = await haulPage();
  const dlg = haul.getByRole('alertdialog', { name: /Allow Microsoft Edge to send downloads to Haul\?/ });
  await expect(dlg).toBeVisible({ timeout: 10_000 });
  await dlg.getByRole('button', { name: 'Allow' }).click();
  await expect.poll(state, { timeout: 10_000 }).toBe('Paired with Haul');
  popup.close();

  const name = `ext-${STAMP}.bin`;
  const url = `http://127.0.0.1:${PORT}/${name}`;
  const tab = await openTab(`http://127.0.0.1:${PAGE_PORT}/?href=${encodeURIComponent(url)}`);

  const row = haul.locator('[role="row"][data-kind="file"]').filter({ hasText: `ext-${STAMP}` });
  await expect(row).toBeVisible({ timeout: 15_000 });
  // A completed download leaves Queue for Finished (since Plan 6a).
  await expect(row).toHaveCount(0, { timeout: 90_000 });
  await haul.getByRole('navigation', { name: 'Views' }).getByRole('button', { name: /Finished/ }).click();
  const done = haul.getByRole('treegrid', { name: 'Finished downloads' }).locator('[role="row"][data-kind]').filter({ hasText: `ext-${STAMP}` });
  await expect(done).toHaveCount(1, { timeout: 10_000 });
  await haul.getByRole('navigation', { name: 'Views' }).getByRole('button', { name: /Queue/ }).click();
  tab.close();

  // The browser's own copy was cancelled and erased.
  expect(await browserDownloads(url)).toEqual([]);

  // The file on disk matches, wherever Haul's save folder (and sorting, which may move it just after) put it.
  const want = expectedSha();
  await expect
    .poll(
      async () => {
        const path = await haul.evaluate(async (n) => {
          const { invoke } = (window as unknown as { __TAURI_INTERNALS__: { invoke: (c: string) => Promise<unknown> } })
            .__TAURI_INTERNALS__;
          const s = (await invoke('snapshot')) as { downloads: { url: string; targetPath: string | null }[] };
          return s.downloads.find((d) => d.url.endsWith(n))?.targetPath ?? null;
        }, name);
        try {
          return path ? createHash('sha256').update(readFileSync(path)).digest('hex') : 'no path';
        } catch {
          return 'unreadable';
        }
      },
      { timeout: 30_000, intervals: [500] },
    )
    .toBe(want);
});

test('kept_when_not_paired', async () => {
  await worker!.eval("chrome.storage.local.set({ token: 'bogus' })");
  const name = `kept-${STAMP}.bin`;
  const url = `http://127.0.0.1:${PORT}/${name}`;
  const tab = await openTab(`http://127.0.0.1:${PAGE_PORT}/?href=${encodeURIComponent(url)}`);
  // The browser keeps going with it: Haul refused the old key, so nothing is cancelled.
  await expect.poll(async () => (await browserDownloads(url))[0]?.state ?? 'none', { timeout: 15_000 }).toBe('in_progress');
  await expect
    .poll(() => worker!.eval<string | undefined>("chrome.storage.local.get('lastResult').then((v) => v.lastResult?.text)"), {
      timeout: 10_000,
    })
    .toBe('Not paired: press Pair');
  const haul = await haulPage();
  await expect(haul.locator('[role="row"][data-kind="file"]').filter({ hasText: `kept-${STAMP}` })).toHaveCount(0);
  await worker!.eval(`chrome.downloads.search({ url: ${JSON.stringify(url)} }).then(([d]) => d && chrome.downloads.cancel(d.id))`);
  tab.close();
});
