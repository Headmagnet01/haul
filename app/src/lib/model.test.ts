import { describe, expect, test } from 'vitest';
import { applyUpdate, buildList, moveIds, planDrop, planMove, summary } from './model';
import type { DownloadView, Snapshot, Update } from './types';

function dl(id: number, name: string, packageId: number | null = null, extra: Partial<DownloadView> = {}): DownloadView {
  return {
    id, packageId, name, url: `https://a.org/${name}`, host: 'a.org', size: 100, written: 0, state: 'queued',
    error: null, priority: 'normal', position: id, connections: 8, resumable: null, targetDir: 'C:\\dl',
    targetPath: null, createdAt: '2026-11-05T10:00:00Z', segments: [], ...extra,
  };
}

const snap = (): Snapshot => ({
  packages: [{ id: 7, name: 'sprite-fright', createdAt: '2026-11-05T10:00:00Z' }],
  downloads: [dl(1, 'ubuntu.iso'), dl(2, 'sprite.7z.001', 7), dl(3, 'notes.pdf'), dl(4, 'sprite.7z.002', 7)],
  speedLimit: { enabled: false, bps: 10_000_000 },
});
const opts = (o: Partial<{ collapsed: Set<number>; filter: string; hidden: Set<number> }> = {}) => ({
  collapsed: new Set<number>(), filter: '', hidden: new Set<number>(), ...o,
});
const shape = (items: ReturnType<typeof buildList>) =>
  items.map((i) => (i.kind === 'package' ? `P${i.pkg.id}` : `${i.child ? 'c' : 'f'}${i.d.id}`));

describe('buildList', () => {
  test('package sits at its first child, children follow', () => {
    const items = buildList(snap(), opts());
    expect(shape(items)).toEqual(['f1', 'P7', 'c2', 'c4', 'f3']);
    const p = items[1];
    expect(p.kind === 'package' && p.children.map((c) => c.id)).toEqual([2, 4]);
    expect(p.kind === 'package' && p.open).toBe(true);
  });
  test('collapsed package has no child items', () => {
    const items = buildList(snap(), opts({ collapsed: new Set([7]) }));
    expect(shape(items)).toEqual(['f1', 'P7', 'f3']);
    expect(items[1].kind === 'package' && items[1].open).toBe(false);
  });
  test('filter is case-insensitive by name and keeps a package only with a matching child', () => {
    expect(shape(buildList(snap(), opts({ filter: 'ISO' })))).toEqual(['f1']);
    expect(shape(buildList(snap(), opts({ filter: 'sprite' })))).toEqual(['P7', 'c2', 'c4']);
    expect(shape(buildList(snap(), opts({ filter: '002' })))).toEqual(['P7', 'c4']);
  });
  test('hidden ids are left out', () => {
    expect(shape(buildList(snap(), opts({ hidden: new Set([1, 2]) })))).toEqual(['f3', 'P7', 'c4']);
    expect(shape(buildList(snap(), opts({ hidden: new Set([2, 4]) })))).toEqual(['f1', 'f3']);
  });
});

describe('applyUpdate', () => {
  const upd = (u: Partial<Update>): Update => ({ progress: [], states: [], steals: [], resync: false, ...u });
  test('progress and states', () => {
    const byId = new Map(snap().downloads.map((d) => [d.id, d]));
    const speeds = new Map<number, number>();
    let r = applyUpdate(byId, speeds, upd({
      progress: [{ id: 1, written: 40, size: 100, speedBps: 5000, segments: [[0, 40, 100]] }],
      states: [{ id: 1, state: 'running', error: null }],
    }));
    expect(r).toEqual({ resync: false });
    expect(byId.get(1)!.written).toBe(40);
    expect(byId.get(1)!.segments).toEqual([[0, 40, 100]]);
    expect(byId.get(1)!.state).toBe('running');
    expect(speeds.get(1)).toBe(5000);
    r = applyUpdate(byId, speeds, upd({ states: [{ id: 1, state: 'paused', error: 'disk full' }] }));
    expect(byId.get(1)!.error).toBe('disk full');
    expect(speeds.has(1)).toBe(false);
  });
  test('unknown_id_requests_resync', () => {
    const byId = new Map(snap().downloads.map((d) => [d.id, d]));
    const speeds = new Map<number, number>();
    expect(applyUpdate(byId, speeds, upd({ states: [{ id: 99, state: 'queued', error: null }] }))).toEqual({ resync: true });
    expect(applyUpdate(byId, speeds, upd({ progress: [{ id: 98, written: 1, size: null, speedBps: 1, segments: [] }] }))).toEqual({ resync: true });
    expect(applyUpdate(byId, speeds, upd({ resync: true }))).toEqual({ resync: true });
  });
});

test('summary', () => {
  const ds = [
    dl(1, 'a', null, { state: 'running' }), dl(2, 'b', null, { state: 'running' }), dl(3, 'c'),
    dl(4, 'd', null, { state: 'failed' }), dl(5, 'e', null, { state: 'link_expired' }), dl(6, 'f', null, { state: 'offline' }),
    dl(7, 'g', null, { state: 'paused', error: 'disk full' }), dl(8, 'h', null, { state: 'paused' }),
    dl(9, 'i', null, { state: 'completed' }),
  ];
  expect(summary(ds, new Map([[1, 100], [2, 50]]))).toEqual({ downloading: 2, waiting: 1, attention: 4, totalBps: 150 });
});

test('moveIds', () => {
  expect(moveIds([1, 2, 3], 3, -1)).toEqual([1, 3, 2]);
  expect(moveIds([1, 2, 3], 1, -1)).toEqual([1, 2, 3]);
  expect(moveIds([1, 2, 3], 1, 1)).toEqual([2, 1, 3]);
  expect(moveIds([1, 2, 3], 3, 1)).toEqual([1, 2, 3]);
});

describe('planMove / planDrop', () => {
  // queue order: 1 (loose), [2, 4] package 7, 3 (loose), 5 low priority
  const ds = () => [
    dl(1, 'a'), dl(2, 'p1', 7), dl(3, 'b'), dl(4, 'p2', 7), dl(5, 'c', null, { priority: 'low' }),
  ];
  test('a loose file swaps with the neighbouring block', () => {
    expect(planMove(ds(), { kind: 'file', id: 3 }, -1)).toEqual({ order: [1, 3, 2, 4, 5], moved: [3], priority: 'normal' });
    expect(planMove(ds(), { kind: 'file', id: 1 }, -1)).toBeNull();
  });
  test('a package moves as a block', () => {
    expect(planMove(ds(), { kind: 'package', id: 7 }, -1)).toEqual({ order: [2, 4, 1, 3, 5], moved: [2, 4], priority: 'normal' });
  });
  test('a child moves among its siblings', () => {
    expect(planMove(ds(), { kind: 'file', id: 4 }, -1)).toEqual({ order: [1, 4, 2, 3, 5], moved: [4], priority: 'normal' });
    expect(planMove(ds(), { kind: 'file', id: 4 }, 1)).toBeNull();
  });
  test('moving across a priority boundary takes the neighbour priority', () => {
    expect(planMove(ds(), { kind: 'file', id: 3 }, 1)).toEqual({ order: [1, 2, 4, 5, 3], moved: [3], priority: 'low' });
  });
  test('drop inserts before the target block and takes its priority', () => {
    expect(planDrop(ds(), { kind: 'file', id: 5 }, { kind: 'file', id: 1 })).toEqual({ order: [5, 1, 2, 4, 3], moved: [5], priority: 'normal' });
    expect(planDrop(ds(), { kind: 'file', id: 1 }, { kind: 'file', id: 4 })).toEqual({ order: [2, 4, 1, 3, 5], moved: [1], priority: 'normal' });
    expect(planDrop(ds(), { kind: 'file', id: 4 }, { kind: 'file', id: 2 })).toEqual({ order: [1, 4, 2, 3, 5], moved: [4], priority: 'normal' });
    expect(planDrop(ds(), { kind: 'package', id: 7 }, { kind: 'file', id: 2 })).toBeNull();
  });
});
