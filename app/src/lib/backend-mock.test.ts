import { afterEach, beforeEach, expect, test, vi } from 'vitest';
import { createMockBackend } from './backend-mock';
import type { Backend } from './backend';
import type { Update } from './types';

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

const CHECK_MS = 100;

/** Like the app: links go to the Collector, are checked, then started. */
async function queue(b: Backend, text: string) {
  await b.addLinks(text, 'paste');
  await vi.advanceTimersByTimeAsync(CHECK_MS);
  const s = await b.collectorSnapshot();
  await b.collectorStart(s.links.map((l) => l.id));
}

test('mock_collector_flow', async () => {
  const b = createMockBackend({ tickMs: 250, demo: false, checkMs: CHECK_MS });
  const got: Update[] = [];
  b.onUpdate((u) => got.push(u));
  const r = await b.addLinks('https://x.org/a.bin https://x.org/404.bin', 'paste');
  expect(r).toEqual({ added: 2, skipped: 0 });
  let c = await b.collectorSnapshot();
  expect(c.links.map((l) => l.status)).toEqual(['checking', 'checking']);
  expect(c.packages).toHaveLength(1);
  expect(c.links.every((l) => l.packageKey === c.packages[0].key)).toBe(true);
  await vi.advanceTimersByTimeAsync(CHECK_MS + 250);
  expect(got.some((u) => u.collector)).toBe(true);
  c = await b.collectorSnapshot();
  expect(c.links.map((l) => [l.status, l.message])).toEqual([['online', null], ['offline', '404']]);
  const online = c.links.find((l) => l.status === 'online')!;
  expect(await b.collectorStart([online.id])).toEqual({ started: 1 });
  expect((await b.snapshot()).downloads.map((d) => d.name)).toEqual(['a.bin']);
  expect((await b.collectorSnapshot()).links).toHaveLength(1);
  expect(await b.addLinks('https://x.org/404.bin', 'paste')).toEqual({ added: 0, skipped: 1 });
  expect(await b.addLinks('no links', 'paste')).toEqual({ added: 0, skipped: 0 });
  expect(b.calls).toContain('addLinks');
});

test('mock_groups_archive_parts', async () => {
  const b = createMockBackend({ tickMs: 250, demo: false, checkMs: CHECK_MS });
  await b.addLinks('https://x.org/m.7z.002 https://x.org/m.7z.001 https://x.org/lone.bin', 'paste');
  const c = await b.collectorSnapshot();
  expect(c.packages.map((p) => [p.name, p.archive])).toEqual([['m', '7z']]);
  expect(c.links.map((l) => [l.name, l.packageKey !== null])).toEqual(
    [['m.7z.001', true], ['m.7z.002', true], ['lone.bin', false]]);
});

test('starting a package makes a package of downloads', async () => {
  const b = createMockBackend({ tickMs: 250, demo: false, checkMs: CHECK_MS });
  await queue(b, 'https://a.org/f.bin https://a.org/g.bin');
  const s = await b.snapshot();
  expect(s.packages).toHaveLength(1);
  expect(s.downloads.filter((d) => d.packageId === s.packages[0].id)).toHaveLength(2);
});

test('ticks emit progress, and pause shows up in the next update', async () => {
  const b = createMockBackend({ tickMs: 250, demo: false, checkMs: CHECK_MS });
  const got: Update[] = [];
  const off = b.onUpdate((u) => got.push(u));
  await queue(b, 'https://a.org/big.bin');
  vi.advanceTimersByTime(250 * 30);
  expect(got.some((u) => u.progress.length >= 1)).toBe(true);
  await queue(b, 'https://a.org/second.bin');
  vi.advanceTimersByTime(500);
  const second = (await b.snapshot()).downloads.find((d) => d.name === 'second.bin')!;
  expect(second.state).toBe('running');
  const id = second.id;
  got.length = 0;
  await b.pause([id]);
  vi.advanceTimersByTime(250);
  expect(got[0].states).toContainEqual({ id, state: 'paused', error: null });
  off();
});

test('a download finishes in about 6 s', async () => {
  const b = createMockBackend({ tickMs: 250, demo: false, checkMs: CHECK_MS });
  await queue(b, 'https://a.org/x.bin');
  vi.advanceTimersByTime(4000);
  expect((await b.snapshot()).downloads[0].state).toBe('running');
  vi.advanceTimersByTime(6000);
  expect((await b.snapshot()).downloads[0].state).toBe('completed');
});

test('demo seed', async () => {
  const b = createMockBackend({ tickMs: 250, demo: true });
  const s = await b.snapshot();
  expect(s.packages).toHaveLength(1);
  expect(s.downloads.length).toBeGreaterThanOrEqual(4);
  expect(s.downloads.some((d) => d.state === 'running' && d.name.endsWith('.iso'))).toBe(true);
  expect(s.downloads.some((d) => d.state === 'link_expired')).toBe(true);
  expect(s.downloads.some((d) => d.state === 'paused')).toBe(true);
  const c = await b.collectorSnapshot();
  expect(c.packages.map((p) => p.name)).toEqual(['caminandes-3-4k', 'Pasted list · 5 Nov 18:40', 'godot-4.4-stable']);
  expect(c.links).toHaveLength(9);
  expect(c.links.find((l) => l.name === 'mozilla-common-voice-en.tar.gz')!.duplicate).toBe('downloaded');
  expect(c.links.find((l) => l.name === 'readme-old.pdf')!.status).toBe('offline');
  expect(c.links.find((l) => l.name === 'caminandes-3-4k.7z.004')!.status).toBe('checking');
});

test('mock_package_runs_steps_in_order', async () => {
  const b = createMockBackend({ tickMs: 250, demo: false, checkMs: CHECK_MS });
  await queue(b, 'https://x.org/film.7z.001 https://x.org/film.7z.002');
  const seen: string[] = [];
  for (let i = 0; i < 200; i++) {
    await vi.advanceTimersByTimeAsync(250);
    const r = (await b.postprocRuns())[0];
    const running = r?.steps.find((s) => s.state === 'running')?.kind;
    if (running && seen.at(-1) !== running) seen.push(running);
    if (r?.state === 'done') break;
  }
  const r = (await b.postprocRuns())[0];
  expect(r.state).toBe('done');
  expect(r.packageId).not.toBeNull();
  expect(r.steps.map((s) => s.state)).toEqual(['skipped', 'done', 'done', 'skipped', 'done']);
  expect(r.steps[2].detail).toBe('Compressed → C:\\Users\\you\\Downloads\\Compressed');
  expect(seen).toEqual(['verify', 'extract', 'sort', 'scan', 'notify']);
});

test('mock_secret_needs_password', async () => {
  const b = createMockBackend({ tickMs: 250, demo: false, checkMs: CHECK_MS });
  await queue(b, 'https://x.org/secret.7z');
  let r = (await b.postprocRuns())[0];
  for (let i = 0; i < 200 && r?.state !== 'needs_password'; i++) {
    await vi.advanceTimersByTimeAsync(250);
    r = (await b.postprocRuns())[0];
  }
  expect(r.steps[1]).toEqual({ kind: 'extract', state: 'needs_password', detail: 'Needs a password' });
  await b.postprocPassword(r.id, 'nope', true);
  for (let i = 0; i < 10; i++) await vi.advanceTimersByTimeAsync(250);
  r = (await b.postprocRuns())[0];
  expect(r.steps[1].detail).toBe('Wrong password');
  expect((await b.rulesConfig()).passwords).toHaveLength(0);
  await b.postprocPassword(r.id, 'haul', true);
  for (let i = 0; i < 20; i++) await vi.advanceTimersByTimeAsync(250);
  expect((await b.postprocRuns())[0].state).toBe('done');
  expect((await b.rulesConfig()).passwords).toHaveLength(1);
});

test('pair_request_then_allow_adds_client', async () => {
  const b = createMockBackend({ tickMs: 250, demo: false });
  const asked: string[] = [];
  b.onPairRequest((_n, browser) => asked.push(browser));
  const n = b.pairRequest('Microsoft Edge');
  expect(asked).toEqual(['Microsoft Edge']);
  await b.pairAnswer(n, true);
  const list = await b.pairedClients();
  expect(list).toHaveLength(1);
  expect(list[0].browser).toBe('Microsoft Edge');
  expect(list[0].lastSeen).toBeNull();
});

test('deny_adds_nothing', async () => {
  const b = createMockBackend({ tickMs: 250, demo: false });
  const n = b.pairRequest('Google Chrome');
  await b.pairAnswer(n, false);
  expect(await b.pairedClients()).toEqual([]);
});

test('remove_paired_client_forgets', async () => {
  const b = createMockBackend({ tickMs: 250, demo: false });
  const n = b.pairRequest('Google Chrome');
  await b.pairAnswer(n, true);
  const [c] = await b.pairedClients();
  await b.removePairedClient(c.id);
  expect(await b.pairedClients()).toEqual([]);
});

test('answer_for_another_request_does_nothing', async () => {
  const b = createMockBackend({ tickMs: 250, demo: false });
  const n = b.pairRequest('Google Chrome');
  await b.pairAnswer(n + 1, true);
  expect(await b.pairedClients()).toEqual([]);
});

test('ended_request_is_closed', async () => {
  const b = createMockBackend({ tickMs: 250, demo: false });
  const closed: number[] = [];
  b.onPairClosed((n) => closed.push(n));
  const n = b.pairRequest('Google Chrome');
  b.pairTimeout();
  expect(closed).toEqual([n]);
  await b.pairAnswer(n, true);
  expect(await b.pairedClients()).toEqual([]); // a late Allow pairs nothing
});
