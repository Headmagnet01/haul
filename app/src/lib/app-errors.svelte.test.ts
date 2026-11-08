import { afterEach, beforeEach, expect, test, vi } from 'vitest';
import { createAppState } from './app-state.svelte';
import { createMockBackend } from './backend-mock';

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

/** An app on the mock's demo rows; `toasts` is the toast showing now. */
async function setup() {
  const b = createMockBackend({ tickMs: 60_000, demo: true });
  const app = createAppState(b);
  await app.load();
  const toasts = () => (app.toast ? [app.toast.text] : []);
  return { b, app, toasts };
}

test('fail_next_rejects_once_with_the_message', async () => {
  const b = createMockBackend({ tickMs: 60_000, demo: false });
  b.failNext('settings', 'disk busy');
  await expect(b.settings()).rejects.toBe('disk busy');
  await expect(b.settings()).resolves.toBeTruthy();
});

test('failed_remove_restores_rows', async () => {
  const { b, app, toasts } = await setup();
  const first = app.snapshot.downloads[0];
  if (!first) return expect.unreachable('no download seeded');
  b.failNext('remove', 'disk busy');
  app.removeWithUndo([first.id]);
  expect(app.hidden.has(first.id)).toBe(true);
  await vi.advanceTimersByTimeAsync(6_500);
  expect(app.hidden.has(first.id)).toBe(false);
  expect(toasts()).toEqual(["Couldn't remove: disk busy"]);
});

test('failed_speed_limit_rolls_back', async () => {
  const { b, app, toasts } = await setup();
  const before = { ...app.snapshot.speedLimit };
  b.failNext('setSettings', 'x');
  await app.setSpeedLimit({ enabled: !before.enabled, bps: before.bps });
  expect(app.snapshot.speedLimit).toEqual(before);
  expect(toasts()).toEqual(["Couldn't change the speed limit: x"]);
});

test('failed_add_toasts', async () => {
  const { b, app, toasts } = await setup();
  b.failNext('addLinks', 'x');
  const r = await app.addText('https://a.org/three.bin', 'paste');
  expect(r).toEqual({ added: 0, skipped: 0 });
  expect(toasts()).toEqual(["Couldn't add links: x"]);
});

test('failed_reorder_reloads', async () => {
  const { b, app, toasts } = await setup();
  // The last lone queue row always has a block above it to move into.
  const lone = app.snapshot.downloads.filter((d) => d.packageId === null && d.state !== 'completed');
  const last = lone.at(-1);
  if (!last || lone.length < 2) return expect.unreachable('the demo has fewer than two lone rows');
  app.select(last.id);
  b.failNext('reorder', 'x');
  const loadsBefore = b.calls.filter((c) => c === 'snapshot').length;
  await app.move(-1);
  expect(toasts()).toEqual(["Couldn't move: x"]);
  expect(b.calls.filter((c) => c === 'snapshot').length).toBeGreaterThan(loadsBefore);
});
