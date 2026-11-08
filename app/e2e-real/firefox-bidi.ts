// A small WebDriver BiDi client for driving the installed Firefox in real-browser tests.
// Playwright's Firefox is a patched build that can't load add-ons, so the real one is
// started here with a throwaway profile and spoken to over its BiDi WebSocket.
import { spawn, type ChildProcess } from 'node:child_process';
import { cpSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

export const FIREFOX = 'C:\\Program Files\\Mozilla Firefox\\firefox.exe';
export const GECKO_ID = 'haul@meraaki410.github.io';
/** Fixed through the `extensions.webextensions.uuids` pref, so tests know the add-on's origin. */
export const EXT_UUID = '6c1d9f2e-1b7a-4e3c-9a55-0d2f7e8b4c31';
export const EXT_ORIGIN = `moz-extension://${EXT_UUID}`;

type Pref = string | number | boolean;

interface BidiReply {
  id?: number;
  type?: string;
  result?: Record<string, unknown>;
  error?: string;
  message?: string;
}

class Bidi {
  private next = 1;
  private waiting = new Map<number, (m: BidiReply) => void>();
  private ws: WebSocket;
  constructor(ws: WebSocket) {
    this.ws = ws;
    ws.addEventListener('message', (e) => {
      const m = JSON.parse(String(e.data)) as BidiReply;
      if (m.id !== undefined) this.waiting.get(m.id)?.(m);
    });
  }
  async send(method: string, params: object = {}): Promise<Record<string, unknown>> {
    const id = this.next++;
    const reply = new Promise<BidiReply>((ok) => this.waiting.set(id, ok));
    this.ws.send(JSON.stringify({ id, method, params }));
    const m = await reply;
    this.waiting.delete(id);
    if (m.type === 'error' || m.error) throw new Error(`${method}: ${m.error} ${m.message ?? ''}`);
    return m.result ?? {};
  }
  close() {
    this.ws.close();
  }
}

interface RemoteValue {
  type: string;
  value?: unknown;
}

/** BiDi's serialised value back to plain JSON. */
function plain(v: RemoteValue | undefined): unknown {
  if (!v) return undefined;
  switch (v.type) {
    case 'undefined':
    case 'null':
      return v.type === 'null' ? null : undefined;
    case 'array':
      return (v.value as RemoteValue[]).map(plain);
    case 'object':
      return Object.fromEntries((v.value as [string | RemoteValue, RemoteValue][]).map(([k, x]) => [typeof k === 'string' ? k : plain(k), plain(x)]));
    default:
      return v.value;
  }
}

export class Tab {
  private bidi: Bidi;
  readonly context: string;
  constructor(bidi: Bidi, context: string) {
    this.bidi = bidi;
    this.context = context;
  }
  async navigate(url: string): Promise<void> {
    await this.bidi.send('browsingContext.navigate', { context: this.context, url, wait: 'complete' });
  }
  /**
   * Opens one of the test copy's add-on pages. BiDi won't navigate to moz-extension: itself, so the
   * current (http) page does it; the page must be web-accessible (see `Firefox.testCopy`).
   */
  async openAddonPage(url: string): Promise<void> {
    await this.eval(`void (location.href = ${JSON.stringify(url)})`);
    const end = Date.now() + 10_000;
    for (;;) {
      try {
        if (await this.eval<boolean>(`location.href === ${JSON.stringify(url)} && document.readyState === 'complete'`)) return;
      } catch {
        // Between documents.
      }
      if (Date.now() > end) throw new Error(`add-on page didn't open: ${url}`);
      await new Promise((ok) => setTimeout(ok, 200));
    }
  }
  /** Runs `expr` in the page (promises awaited) and returns its value as JSON. */
  async eval<T>(expr: string): Promise<T> {
    const r = (await this.bidi.send('script.evaluate', {
      expression: expr,
      target: { context: this.context },
      awaitPromise: true,
      resultOwnership: 'none',
    })) as { type: string; result?: RemoteValue; exceptionDetails?: { text: string } };
    if (r.type === 'exception') throw new Error(r.exceptionDetails?.text ?? 'script exception');
    return plain(r.result) as T;
  }
}

export class Firefox {
  private proc: ChildProcess;
  private bidi: Bidi;
  private constructor(proc: ChildProcess, bidi: Bidi) {
    this.proc = proc;
    this.bidi = bidi;
  }

  static async launch(opts: { profileDir: string; port: number; prefs?: Record<string, Pref> }): Promise<Firefox> {
    const dl = join(opts.profileDir, 'dl');
    mkdirSync(dl, { recursive: true });
    const prefs: Record<string, Pref> = {
      'extensions.webextensions.uuids': JSON.stringify({ [GECKO_ID]: EXT_UUID }),
      'browser.download.useDownloadDir': true,
      'browser.download.folderList': 2,
      'browser.download.dir': dl,
      'browser.download.always_ask_before_handling_new_types': false,
      'browser.shell.checkDefaultBrowser': false,
      'datareporting.policy.dataSubmissionEnabled': false,
      'browser.aboutwelcome.enabled': false,
      ...opts.prefs,
    };
    writeFileSync(
      join(opts.profileDir, 'user.js'),
      Object.entries(prefs)
        .map(([k, v]) => `user_pref(${JSON.stringify(k)}, ${JSON.stringify(v)});`)
        .join('\n') + '\n',
    );
    const proc = spawn(FIREFOX, ['-profile', opts.profileDir, '-no-remote', '--remote-debugging-port', String(opts.port), '--remote-allow-system-access'], {
      stdio: 'ignore',
      env: { ...process.env, MOZ_REMOTE_ALLOW_SYSTEM_ACCESS: '1' },
    });
    const ws = await connect(`ws://127.0.0.1:${opts.port}/session`, 30_000);
    const bidi = new Bidi(ws);
    await bidi.send('session.new', { capabilities: {} });
    return new Firefox(proc, bidi);
  }

  /**
   * A copy of the add-on in `out` whose pages tests may open: BiDi refuses to navigate to an
   * add-on page that isn't web-accessible. Only the test copy gets this; the shipped one never.
   */
  static testCopy(dir: string, out: string): string {
    rmSync(out, { recursive: true, force: true });
    cpSync(dir, out, { recursive: true });
    const file = join(out, 'manifest.json');
    const m = JSON.parse(readFileSync(file, 'utf8')) as Record<string, unknown>;
    m.web_accessible_resources = [{ resources: ['popup.html', 'options.html', 'picker.html'], matches: ['<all_urls>'] }];
    writeFileSync(file, JSON.stringify(m, null, 2));
    return out;
  }

  /** Installs an unpacked add-on as a temporary one (no signature needed). */
  async installTemporary(dir: string): Promise<void> {
    await this.bidi.send('webExtension.install', { extensionData: { type: 'path', path: dir } });
  }

  /** The raw BiDi command, for debugging a test. */
  send(method: string, params: object = {}): Promise<Record<string, unknown>> {
    return this.bidi.send(method, params);
  }

  async openTab(url: string): Promise<Tab> {
    const r = (await this.bidi.send('browsingContext.create', { type: 'tab' })) as { context: string };
    const tab = new Tab(this.bidi, r.context);
    await tab.navigate(url);
    return tab;
  }

  async close(): Promise<void> {
    try {
      await Promise.race([this.bidi.send('browser.close'), new Promise((ok) => setTimeout(ok, 3000))]);
    } catch {
      // Already gone.
    }
    this.bidi.close();
    if (this.proc.exitCode === null) this.proc.kill();
  }
}

async function connect(url: string, ms: number): Promise<WebSocket> {
  const end = Date.now() + ms;
  for (;;) {
    try {
      return await new Promise<WebSocket>((ok, bad) => {
        const ws = new WebSocket(url);
        ws.addEventListener('open', () => ok(ws));
        ws.addEventListener('error', () => bad(new Error('no BiDi yet')));
      });
    } catch (e) {
      if (Date.now() > end) throw e;
      await new Promise((ok) => setTimeout(ok, 300));
    }
  }
}
