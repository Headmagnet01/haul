import { describe, expect, it } from 'vitest';
import { finishedIds, finishedList, finishedTime, isFinished, locationOf } from './finished';
import type { DownloadView, Run, RunState, Snapshot } from './types';

function dl(id: number, extra: Partial<DownloadView> = {}): DownloadView {
  return {
    id, packageId: null, name: `f${id}.bin`, url: `https://a.org/f${id}.bin`, host: 'a.org', size: 10, written: 10,
    state: 'completed', error: null, priority: 'normal', position: id, connections: 4, resumable: true,
    targetDir: 'C:\\dl', targetPath: `C:\\dl\\f${id}.bin`, createdAt: '2026-11-01T10:00:00Z', finishedAt: null,
    segments: [], ...extra,
  };
}

function run(state: RunState, t: { packageId?: number; downloadId?: number }): Run {
  return { id: 1, packageId: t.packageId ?? null, downloadId: t.downloadId ?? null, state, steps: [], updatedAt: '' };
}

describe('isFinished', () => {
  it('all completed and no run', () => {
    expect(isFinished([dl(1), dl(2)], null)).toBe(true);
  });
  it('one still running', () => {
    expect(isFinished([dl(1), dl(2, { state: 'running' })], null)).toBe(false);
  });
  it('a failed download stays in Queue', () => {
    expect(isFinished([dl(1, { state: 'failed' })], null)).toBe(false);
  });
  it('nothing to finish', () => {
    expect(isFinished([], null)).toBe(false);
  });
  for (const s of ['queued', 'running', 'waiting', 'needs_password', 'failed'] as RunState[]) {
    it(`run ${s} keeps it in Queue`, () => {
      expect(isFinished([dl(1)], run(s, { downloadId: 1 }))).toBe(false);
    });
  }
  for (const s of ['held', 'done'] as RunState[]) {
    it(`run ${s} is finished`, () => {
      expect(isFinished([dl(1)], run(s, { downloadId: 1 }))).toBe(true);
    });
  }
});

describe('finished_time_today_vs_older', () => {
  const now = new Date(2026, 10, 8, 18, 0);
  it('same day shows the time', () => {
    expect(finishedTime(new Date(2026, 10, 8, 14, 32).toISOString(), now)).toBe('14:32');
  });
  it('yesterday shows a date', () => {
    expect(finishedTime(new Date(2026, 10, 7, 23, 59).toISOString(), now)).toBe('7 Nov 2026');
  });
  it('null is blank', () => {
    expect(finishedTime(null, now)).toBe('');
  });
});

describe('finishedIds', () => {
  const snap = (downloads: DownloadView[]): Snapshot => ({
    packages: [{ id: 7, name: 'pkg', createdAt: '' }], downloads, speedLimit: { enabled: false, bps: 1 },
  });
  it('a package moves as a whole once its run ends', () => {
    const ds = [dl(1, { packageId: 7 }), dl(2, { packageId: 7 }), dl(3)];
    expect([...finishedIds(snap(ds), [run('running', { packageId: 7 })])]).toEqual([3]);
    expect([...finishedIds(snap(ds), [run('done', { packageId: 7 })])].sort()).toEqual([1, 2, 3]);
  });
  it('a package with one file left stays in Queue', () => {
    const ds = [dl(1, { packageId: 7 }), dl(2, { packageId: 7, state: 'paused' })];
    expect(finishedIds(snap(ds), []).size).toBe(0);
  });
  it('a lone file waits for its own run', () => {
    expect(finishedIds(snap([dl(3)]), [run('queued', { downloadId: 3 })]).size).toBe(0);
  });
});

describe('finishedList', () => {
  it('newest first, packages collapsed unless expanded', () => {
    const ds = [
      dl(1, { packageId: 7, finishedAt: '2026-11-08T09:00:00Z' }),
      dl(2, { finishedAt: '2026-11-08T12:00:00Z' }),
      dl(3, { finishedAt: null, createdAt: '2026-11-01T00:00:00Z' }),
    ];
    const snap: Snapshot = { packages: [{ id: 7, name: 'pkg', createdAt: '' }], downloads: ds, speedLimit: { enabled: false, bps: 1 } };
    const ids = new Set([1, 2, 3]);
    const keys = (xs: ReturnType<typeof finishedList>) => xs.map((x) => (x.kind === 'package' ? `p${x.pkg.id}` : `f${x.d.id}`));
    expect(keys(finishedList(snap, ids, { expanded: new Set(), filter: '', hidden: new Set() }))).toEqual(['f2', 'p7', 'f3']);
    expect(keys(finishedList(snap, ids, { expanded: new Set([7]), filter: '', hidden: new Set() }))).toEqual(['f2', 'p7', 'f1', 'f3']);
    expect(keys(finishedList(snap, ids, { expanded: new Set(), filter: 'f3', hidden: new Set() }))).toEqual(['f3']);
  });
});

describe('locationOf', () => {
  it('is the folder of the saved file', () => {
    expect(locationOf(dl(1, { targetPath: 'C:\\Users\\you\\Downloads\\a.zip' }))).toBe('C:\\Users\\you\\Downloads');
    expect(locationOf(dl(1, { targetPath: null, targetDir: 'D:\\x' }))).toBe('D:\\x');
  });
});
