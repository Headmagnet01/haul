import { expect, test } from 'vitest';
import { parseSpeed, parseWhole } from './settings-num';

test('whole_numbers_in_range', () => {
  expect(parseWhole('5', 1, 20)).toBe(5);
  expect(parseWhole(' 5 ', 1, 20)).toBe(5);
  expect(parseWhole('20', 1, 20)).toBe(20);
  expect(parseWhole('1', 1, 20)).toBe(1);
});

test('everything_else_is_refused', () => {
  for (const t of ['', '  ', '2.5', '1e1', '-3', '0', '21', 'abc', '+5', '0x5']) {
    expect(parseWhole(t, 1, 20), JSON.stringify(t)).toBeNull();
  }
});

test('speed_in_mb_per_s', () => {
  expect(parseSpeed('2.5')).toBe(2.5);
  expect(parseSpeed(' 0.01 ')).toBe(0.01);
  for (const t of ['', '0', '0.001', '-1', 'abc', 'Infinity']) {
    expect(parseSpeed(t), JSON.stringify(t)).toBeNull();
  }
});
