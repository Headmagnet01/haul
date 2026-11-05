import { expect, test } from 'vitest';
import { sparkPoints } from './sparkline';

test('sparkline points', () => {
  expect(sparkPoints([0, 10], 84, 20)).toBe('0.0,20.0 84.0,2.0');
  expect(sparkPoints([], 84, 20)).toBe('');
  expect(sparkPoints([0, 0], 84, 20)).toBe('0.0,20.0 84.0,20.0');
  expect(sparkPoints([5, 10, 5], 84, 20)).toBe('0.0,11.0 42.0,2.0 84.0,11.0');
});
