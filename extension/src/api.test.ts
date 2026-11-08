import { afterEach, beforeEach, expect, test, vi } from 'vitest';
import { makeClient, type Fetcher, type Saved } from './api';

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

function storage(init: Saved) {
  let v = { ...init };
  return {
    get: async () => ({ ...v }),
    set: async (p: Partial<Saved>) => void (v = { ...v, ...p }),
    now: () => v,
  };
}

type Call = { url: string; init: RequestInit };

/** Haul listens on `live` (null = nowhere); `capture` is the status /v1/capture answers with. */
function haul(live: number | null, capture = 200) {
  const calls: Call[] = [];
  const fetch: Fetcher = async (url, init) => {
    calls.push({ url, init });
    const u = new URL(url);
    if (Number(u.port) !== live) throw new TypeError('Failed to fetch');
    if (u.pathname === '/v1/status') {
      if (init.method !== 'POST') throw new Error('status must be a POST');
      return { status: 200, json: async () => ({ app: 'haul' }) };
    }
    return { status: capture, json: async () => ({ id: 1 }) };
  };
  return { fetch, calls };
}

const BODY = { url: 'https://a.org/f.zip', cookies: 'sid=1' };

test('sends_bearer_and_json', async () => {
  const h = haul(17654);
  const c = makeClient(h.fetch, storage({ port: 17654, token: 'tok' }));
  expect(await c.capture(BODY)).toBe('ok');
  const call = h.calls.find((x) => x.url.endsWith('/v1/capture'))!;
  expect(call.url).toBe('http://127.0.0.1:17654/v1/capture');
  expect(call.init.method).toBe('POST');
  const headers = call.init.headers as Record<string, string>;
  expect(headers.Authorization).toBe('Bearer tok');
  expect(headers['Content-Type']).toBe('application/json');
  expect(JSON.parse(call.init.body as string)).toEqual(BODY);
});

test('no_token_is_unpaired_without_a_request', async () => {
  const h = haul(17654);
  expect(await makeClient(h.fetch, storage({ port: 17654, token: null })).capture(BODY)).toBe('unpaired');
  expect(h.calls).toEqual([]);
});

test('timeout_after_1500ms_is_failed', async () => {
  const never: Fetcher = () => new Promise(() => {});
  const c = makeClient(never, storage({ port: 17654, token: 'tok' }));
  const r = c.capture(BODY);
  await vi.advanceTimersByTimeAsync(1499);
  let settled = false;
  void r.then(() => (settled = true));
  await Promise.resolve();
  expect(settled).toBe(false);
  await vi.advanceTimersByTimeAsync(2);
  expect(await r).toBe('failed');
});

test('401_clears_token_and_is_unpaired', async () => {
  const s = storage({ port: 17654, token: 'tok' });
  expect(await makeClient(haul(17654, 401).fetch, s).capture(BODY)).toBe('unpaired');
  expect(s.now().token).toBeNull();
});

test('server_error_is_failed', async () => {
  expect(await makeClient(haul(17654, 500).fetch, storage({ port: 17654, token: 'tok' })).capture(BODY)).toBe('failed');
});

test('port_rescans_after_failure', async () => {
  const s = storage({ port: 17654, token: 'tok' });
  const h = haul(17659);
  expect(await makeClient(h.fetch, s).capture(BODY)).toBe('ok');
  expect(s.now().port).toBe(17659);
  expect(h.calls.filter((x) => x.url.endsWith('/v1/capture')).map((x) => new URL(x.url).port)).toEqual(['17654', '17659']);
});

test('nothing_listening_is_failed', async () => {
  expect(await makeClient(haul(null).fetch, storage({ port: 17654, token: 'tok' })).capture(BODY)).toBe('failed');
});
