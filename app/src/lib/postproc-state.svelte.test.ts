import { afterEach, beforeEach, expect, test, vi } from 'vitest';
import { createPostprocState } from './postproc-state.svelte';
import { createMockBackend } from './backend-mock';
import type { Backend } from './backend';
import type { Run, Update } from './types';

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

const upd = (u: Partial<Update>): Update =>
  ({ progress: [], states: [], steals: [], resync: false, collector: false, postproc: false, ...u });

function setup(b: Backend = createMockBackend({ tickMs: 250, demo: false, checkMs: 50 })) {
  const toasts: string[] = [];
  let changed = 0;
  const p = createPostprocState(b, { showToast: (t: string) => void toasts.push(t), onChanged: () => void changed++ });
  return { b, p, toasts, changed: () => changed };
}

const fakeRun = (id: number, state: Run['state']): Run => ({
  id, packageId: id, downloadId: null, state, updatedAt: '', steps: [],
});

test('load_on_postproc_update', async () => {
  const b = createMockBackend({ tickMs: 250, demo: false });
  const { p, changed } = setup(b);
  p.onUpdate(upd({ collector: true }));
  await vi.advanceTimersByTimeAsync(0);
  expect(b.calls.filter((c) => c === 'postprocRuns')).toHaveLength(0);
  p.onUpdate(upd({ postproc: true }));
  await vi.advanceTimersByTimeAsync(0);
  expect(b.calls.filter((c) => c === 'postprocRuns')).toHaveLength(1);
  expect(changed()).toBe(1);
});

test('newest_load_wins', async () => {
  const resolvers: ((r: Run[]) => void)[] = [];
  const b = { postprocRuns: () => new Promise<Run[]>((res) => resolvers.push(res)) } as unknown as Backend;
  const { p } = setup(b);
  const first = p.load();
  const second = p.load(); // requested while the first is in flight: runs once afterwards
  resolvers[0]([fakeRun(1, 'running')]);
  await first;
  await vi.advanceTimersByTimeAsync(0);
  expect(resolvers).toHaveLength(2);
  resolvers[1]([fakeRun(1, 'done')]);
  await second;
  await vi.advanceTimersByTimeAsync(0);
  expect(p.byPackage(1)?.state).toBe('done');
  expect(p.byPackage(2)).toBeNull();
});

test('password_wrong_toast_free', async () => {
  let gave = 0;
  const b = {
    postprocRuns: async () => [fakeRun(1, 'needs_password')],
    postprocPassword: async () => void gave++,
  } as unknown as Backend;
  const { p, toasts } = setup(b);
  await p.password(1, 'nope', true);
  expect(gave).toBe(1);
  expect(toasts).toEqual([]);
});

test('set_hash_toasts', async () => {
  const b = {
    postprocRuns: async () => [],
    setExpectedHash: async (_id: number, h: string | null) => {
      if (h === 'bad') throw 'Not a checksum: use 8, 32, 40 or 64 hex characters';
    },
  } as unknown as Backend;
  const { p, toasts } = setup(b);
  expect(await p.setHash(1, 'd41d8cd98f00b204e9800998ecf8427e')).toBe(true);
  expect(await p.setHash(1, 'bad')).toBe(false);
  expect(toasts).toEqual(['Checksum saved', 'Not a checksum: use 8, 32, 40 or 64 hex characters']);
});
