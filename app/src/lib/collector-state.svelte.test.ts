import { afterEach, beforeEach, expect, test, vi } from 'vitest';
import { createCollectorState } from './collector-state.svelte';
import { createMockBackend } from './backend-mock';
import type { Update } from './types';

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

const upd = (u: Partial<Update>): Update =>
  ({ progress: [], states: [], steals: [], resync: false, collector: false, ...u });

function setup() {
  const b = createMockBackend({ tickMs: 250, demo: false, checkMs: 50 });
  const toasts: string[] = [];
  let started = 0;
  const c = createCollectorState(b, {
    showToast: (text: string) => void toasts.push(text),
    onStarted: () => void started++,
  });
  return { b, c, toasts, started: () => started };
}

test('remove_hides_then_commits', async () => {
  const { b, c } = setup();
  await b.addLinks('https://a.org/1.bin https://a.org/2.bin', 'paste');
  await c.load();
  const id = c.snapshot.links[0].id;
  c.removeWithUndo([id]);
  expect(c.items.some((i) => i.kind === 'link' && i.link.id === id)).toBe(false);
  expect(b.calls).not.toContain('collectorRemove');
  await vi.advanceTimersByTimeAsync(6000);
  expect(b.calls.filter((x) => x === 'collectorRemove')).toHaveLength(1);
  expect(c.snapshot.links.map((l) => l.id)).not.toContain(id);
});

test('undo_restores', async () => {
  const { b, c, toasts } = setup();
  await b.addLinks('https://a.org/1.bin', 'paste');
  await c.load();
  const id = c.snapshot.links[0].id;
  c.removeWithUndo([id]);
  expect(toasts.at(-1)).toBe('Removed 1.bin');
  c.undo();
  expect(c.items.some((i) => i.kind === 'link' && i.link.id === id)).toBe(true);
  await vi.advanceTimersByTimeAsync(6000);
  expect(b.calls).not.toContain('collectorRemove');
});

test('update_flag_reloads', async () => {
  const { b, c } = setup();
  const before = b.calls.filter((x) => x === 'collectorSnapshot').length;
  c.onUpdate(upd({ collector: true }));
  await vi.advanceTimersByTimeAsync(0);
  expect(b.calls.filter((x) => x === 'collectorSnapshot').length).toBe(before + 1);
  c.onUpdate(upd({ progress: [] }));
  await vi.advanceTimersByTimeAsync(0);
  expect(b.calls.filter((x) => x === 'collectorSnapshot').length).toBe(before + 1);
});

test('start_package_toasts_and_notifies', async () => {
  const { b, c, toasts, started } = setup();
  await b.addLinks('https://a.org/1.bin https://a.org/2.bin', 'paste');
  await vi.advanceTimersByTimeAsync(60);
  await c.load();
  await c.startPackage(c.snapshot.packages[0].key);
  expect(toasts.at(-1)).toBe('Started 2 downloads');
  expect(started()).toBe(1);
  expect(c.snapshot.links).toHaveLength(0);
});

test('chip_toggle_rolls_back_on_error', async () => {
  const { b, c } = setup();
  await c.load();
  expect(c.snapshot.filters.small).toBe(false);
  b.setCollectorFilters = () => Promise.reject(new Error('disk'));
  await c.toggleChip('small');
  expect(c.snapshot.filters.small).toBe(false);
});

test('filter_change_beats_snapshot_in_flight', async () => {
  const { b, c } = setup();
  await c.load();
  let release: () => void = () => {};
  const real = b.collectorSnapshot.bind(b);
  b.collectorSnapshot = async () => {
    const s = await real(); // read before the filter change
    await new Promise<void>((r) => (release = r));
    return s;
  };
  const stale = c.load();
  await vi.advanceTimersByTimeAsync(0);
  let releaseNext: () => void = () => {};
  b.collectorSnapshot = async () => {
    const s = await real();
    await new Promise<void>((r) => (releaseNext = r)); // hold the follow-up load open
    return s;
  };
  await c.toggleChip('small');
  release();
  await stale;
  expect(c.snapshot.filters.small).toBe(true); // the stale snapshot never shows, not even briefly
  releaseNext();
  await vi.advanceTimersByTimeAsync(0);
  expect(c.snapshot.filters.small).toBe(true);
});

test('chip_toggle_saves_through_the_backend', async () => {
  const { b, c } = setup();
  await c.load();
  await c.addExt('csv');
  await c.toggleChip('small');
  expect(c.snapshot.filters.small).toBe(true);
  expect((await b.collectorSnapshot()).filters).toEqual(
    { html: true, images: true, small: true, duplicates: false, exts: ['csv'] });
});

test('double_start_calls_backend_once', async () => {
  const { b, c, toasts } = setup();
  await b.addLinks('https://a.org/1.bin https://a.org/2.bin', 'paste');
  await vi.advanceTimersByTimeAsync(60);
  await c.load();
  const key = c.snapshot.packages[0].key;
  await Promise.all([c.startPackage(key), c.startPackage(key), c.startSelected()]);
  expect(b.calls.filter((x) => x === 'collectorStart')).toHaveLength(1);
  expect(toasts.filter((t) => t.startsWith('Started'))).toEqual(['Started 2 downloads']);
});
