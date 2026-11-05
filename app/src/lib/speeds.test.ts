import { expect, test } from 'vitest';
import { RangeSpeeds } from './speeds';

test('speed per range from successive samples', () => {
  const r = new RangeSpeeds();
  r.update(1, [[0, 0, 100]], 0);
  r.update(1, [[0, 40, 100]], 1000);
  expect(r.get(1)).toEqual([{ start: 0, bps: 40, done: false }]);
});

test('finished range is done with 0 bps; a steal starts at 0 until its second sample', () => {
  const r = new RangeSpeeds();
  r.update(1, [[0, 0, 100], [100, 100, 200]], 0);
  r.update(1, [[0, 100, 100], [100, 120, 150], [150, 150, 200]], 1000);
  expect(r.get(1)).toEqual([
    { start: 0, bps: 0, done: true },
    { start: 100, bps: 20, done: false },
    { start: 150, bps: 0, done: false },
  ]);
  r.update(1, [[0, 100, 100], [100, 130, 150], [150, 175, 200]], 2000);
  expect(r.get(1)[2]).toEqual({ start: 150, bps: 25, done: false });
});

test('forget and unknown ids', () => {
  const r = new RangeSpeeds();
  expect(r.get(5)).toEqual([]);
  r.update(5, [[0, 1, 2]], 0);
  r.forget(5);
  expect(r.get(5)).toEqual([]);
});
