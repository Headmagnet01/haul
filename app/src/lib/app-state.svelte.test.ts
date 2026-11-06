import { expect, test } from 'vitest';
import { createAppState } from './app-state.svelte';
import type { Backend } from './backend';
import type { DownloadView, Snapshot, State, Update } from './types';

function dl(id: number, state: State): DownloadView {
  return {
    id, packageId: null, name: `f${id}.bin`, url: `https://a.org/f${id}.bin`, host: 'a.org', size: 100, written: 0,
    state, error: null, priority: 'normal', position: id, connections: 8, resumable: true, targetDir: 'C:\\dl',
    targetPath: null, createdAt: '2026-11-05T10:00:00Z', segments: [],
  };
}
const snap = (...ds: DownloadView[]): Snapshot => ({ packages: [], downloads: ds, speedLimit: { enabled: false, bps: 1 } });
const upd = (u: Partial<Update>): Update => ({ progress: [], states: [], steals: [], resync: false, collector: false, postproc: false, ...u });

/** A backend whose snapshots resolve only when the test says so. */
function fake() {
  const waiting: ((s: Snapshot) => void)[] = [];
  let cb: (u: Update) => void = () => {};
  const b = {
    snapshot: () => new Promise<Snapshot>((r) => waiting.push(r)),
    onUpdate: (f: (u: Update) => void) => ((cb = f), () => {}),
  } as unknown as Backend;
  return { b, waiting, emit: (u: Update) => cb(u) };
}
const flush = () => new Promise((r) => setTimeout(r, 0));

test('an update that lands while a snapshot loads is not lost', async () => {
  const f = fake();
  const app = createAppState(f.b);
  const first = app.load();
  f.waiting.shift()!(snap(dl(1, 'running')));
  await first;
  const second = app.load(); // engine reads the DB: row 1 still running
  f.emit(upd({ states: [{ id: 1, state: 'paused', error: null }] })); // the user's pause lands now
  f.waiting.shift()!(snap(dl(1, 'running')));
  await second;
  await flush();
  expect(app.snapshot.downloads[0].state).toBe('paused');
});

test('overlapping loads: the newest snapshot wins', async () => {
  const f = fake();
  const app = createAppState(f.b);
  const older = app.load();
  const newer = app.load();
  // Resolve the newest first, then the stale one.
  f.waiting[1](snap(dl(1, 'completed')));
  f.waiting[0](snap(dl(1, 'running')));
  await Promise.all([older, newer]);
  await flush();
  expect(app.snapshot.downloads[0].state).toBe('completed');
});
