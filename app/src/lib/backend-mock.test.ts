import { afterEach, beforeEach, expect, test, vi } from 'vitest';
import { createMockBackend } from './backend-mock';
import type { Update } from './types';

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

test('addLinks makes a package of two downloads', async () => {
  const b = createMockBackend({ tickMs: 250, demo: false });
  const r = await b.addLinks('https://a.org/f.bin https://a.org/404.bin');
  expect(r.added).toBe(2);
  expect(r.packageId).not.toBeNull();
  const s = await b.snapshot();
  expect(s.packages).toHaveLength(1);
  expect(s.downloads.filter((d) => d.packageId === r.packageId)).toHaveLength(2);
  expect(s.downloads.find((d) => d.url.includes('404'))!.state).toBe('offline');
  expect(b.calls).toContain('addLinks');
  expect((await b.addLinks('no links')).added).toBe(0);
});

test('ticks emit progress, and pause shows up in the next update', async () => {
  const b = createMockBackend({ tickMs: 250, demo: false });
  const got: Update[] = [];
  const off = b.onUpdate((u) => got.push(u));
  await b.addLinks('https://a.org/big.bin');
  vi.advanceTimersByTime(250 * 30);
  expect(got.some((u) => u.progress.length >= 1)).toBe(true);
  await b.addLinks('https://a.org/second.bin');
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
  const b = createMockBackend({ tickMs: 250, demo: false });
  await b.addLinks('https://a.org/x.bin');
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
});
