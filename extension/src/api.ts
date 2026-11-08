import { findPort } from './port';
import { TimeoutError, base, withTimeout, type Fetcher, type Reply } from './net';
import type { CaptureBody } from './types';

export type { Fetcher } from './net';

/** If Haul doesn't answer a capture this fast, the browser keeps the download. */
export const CAPTURE_TIMEOUT_MS = 1500;
/** Long enough for the person to answer Haul's Allow/Deny question (Haul gives up at 60 s). */
const PAIR_TIMEOUT_MS = 65_000;
const LINKS_TIMEOUT_MS = 10_000;

export interface Saved {
  port: number | null;
  token: string | null;
}

export interface SavedStore {
  get(): Promise<Saved>;
  set(v: Partial<Saved>): Promise<void>;
}

export type Outcome = 'ok' | 'unpaired' | 'failed';

export interface Status {
  running: boolean;
  paired: boolean;
  active: number;
}

/** Talks to Haul: remembers its port and key, rescans once when Haul moved. */
export function makeClient(fetch: Fetcher, saved: SavedStore) {
  /** One request; 'network' means nothing answered on that port. */
  async function attempt(port: number, path: string, init: RequestInit, ms: number): Promise<Reply | 'network' | 'timeout'> {
    try {
      return await withTimeout(ms, (signal) => fetch(`${base(port)}${path}`, { ...init, signal }));
    } catch (e) {
      return e instanceof TimeoutError ? 'timeout' : 'network';
    }
  }

  /** Sends to the remembered port; if nothing is there, finds Haul again and retries once. */
  async function send(path: string, init: RequestInit, ms: number): Promise<Reply | null> {
    const s = await saved.get();
    let port = s.port ?? (await findPort(fetch, null));
    if (port === null) return null;
    if (port !== s.port) await saved.set({ port });
    let r = await attempt(port, path, init, ms);
    if (r === 'network') {
      const found = await findPort(fetch, null);
      if (found === null) return null;
      if (found !== port) await saved.set({ port: found });
      port = found;
      r = await attempt(port, path, init, ms);
    }
    return typeof r === 'string' ? null : r;
  }

  async function authed(path: string, body: unknown, ms: number): Promise<{ outcome: Outcome; reply: Reply | null }> {
    const { token } = await saved.get();
    if (!token) return { outcome: 'unpaired', reply: null };
    const r = await send(
      path,
      {
        method: 'POST',
        headers: { Authorization: `Bearer ${token}`, 'Content-Type': 'application/json' },
        body: JSON.stringify(body),
      },
      ms,
    );
    if (r === null) return { outcome: 'failed', reply: null };
    if (r.status === 401) {
      await saved.set({ token: null });
      return { outcome: 'unpaired', reply: r };
    }
    return { outcome: r.status === 200 ? 'ok' : 'failed', reply: r };
  }

  return {
    async capture(body: CaptureBody): Promise<Outcome> {
      return (await authed('/v1/capture', body, CAPTURE_TIMEOUT_MS)).outcome;
    },

    async links(links: string[], pageTitle: string): Promise<{ outcome: Outcome; added: number }> {
      const { outcome, reply } = await authed('/v1/links', { links, page_title: pageTitle }, LINKS_TIMEOUT_MS);
      const added = outcome === 'ok' && reply ? Number(((await reply.json()) as { added?: number }).added ?? 0) : 0;
      return { outcome, added };
    },

    /** Asks Haul to pair. Resolves after the person answers in Haul's window. */
    async pair(browser: string): Promise<'paired' | 'denied' | 'failed'> {
      const r = await send(
        '/v1/pair',
        { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ browser }) },
        PAIR_TIMEOUT_MS,
      );
      if (r === null) return 'failed';
      if (r.status !== 200) return r.status === 403 || r.status === 429 ? 'denied' : 'failed';
      const token = ((await r.json()) as { token?: unknown }).token;
      if (typeof token !== 'string' || !token) return 'failed';
      await saved.set({ token });
      return 'paired';
    },

    async status(): Promise<Status> {
      const { token } = await saved.get();
      const headers: Record<string, string> = token ? { Authorization: `Bearer ${token}` } : {};
      // A POST: Chrome sends no Origin on an extension's GET, and Haul requires one.
      const r = await send('/v1/status', { method: 'POST', headers }, CAPTURE_TIMEOUT_MS);
      if (r === null || r.status !== 200) return { running: false, paired: false, active: 0 };
      const v = (await r.json()) as { paired?: boolean; active?: number };
      return { running: true, paired: !!v.paired, active: Number(v.active ?? 0) };
    },
  };
}

export type HaulApi = ReturnType<typeof makeClient>;
