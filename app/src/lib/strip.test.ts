import { expect, test } from 'vitest';
import { packageCells, stripCells, stripKind } from './strip';
import type { DownloadView } from './types';

test('cells follow ranges', () => {
  expect(stripCells([[0, 50, 100], [100, 100, 200]], 200, 'running')).toEqual([
    { flex: 100, fill: 0.5 },
    { flex: 100, fill: 0 },
  ]);
  // done ranges merge
  expect(stripCells([[0, 100, 100], [100, 200, 200], [200, 250, 300]], 300, 'running')).toEqual([
    { flex: 200, fill: 1 },
    { flex: 100, fill: 0.5 },
  ]);
  expect(stripCells([], 500, 'queued')).toEqual([{ flex: 1, fill: 0 }]);
  expect(stripCells([], 500, 'completed')).toEqual([{ flex: 1, fill: 1 }]);
});

test('strip_unknown_size', () => {
  expect(stripCells([[0, 300, Number.MAX_SAFE_INTEGER]], null, 'running')).toEqual([{ flex: 1, fill: 0 }]);
  for (const c of stripCells([[0, 300, 2 ** 64]], null, 'running')) {
    expect(Number.isFinite(c.flex) && Number.isFinite(c.fill)).toBe(true);
  }
});

test('strip kind', () => {
  expect(stripKind('paused', 'disk full')).toBe('error');
  expect(stripKind('completed', null)).toBe('done');
  expect(stripKind('running', null)).toBe('active');
  expect(stripKind('paused', null)).toBe('paused');
  expect(stripKind('queued', null)).toBe('queued');
  expect(stripKind('link_expired', null)).toBe('error');
});

test('package cells: one per file', () => {
  const d = (id: number, size: number | null, written: number, state: DownloadView['state']) =>
    ({ id, size, written, state }) as DownloadView;
  expect(packageCells([d(1, 100, 100, 'completed'), d(2, 300, 150, 'running'), d(3, null, 5, 'queued')])).toEqual([
    { flex: 100, fill: 1 },
    { flex: 300, fill: 0.5 },
    { flex: 1, fill: 0 },
  ]);
});
