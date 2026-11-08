import { expect, test } from 'vitest';
import { PAIR_WAIT_MS, pairStateNow } from './pairstate';

test('waiting_expires_after_the_pair_window', () => {
  const t0 = 1_000_000;
  expect(pairStateNow('waiting', t0, t0 + 1000)).toBe('waiting');
  expect(pairStateNow('waiting', t0, t0 + PAIR_WAIT_MS + 1)).toBe('idle');
  expect(pairStateNow('waiting', undefined, t0)).toBe('idle');
  expect(pairStateNow('paired', t0, t0 + 10 * PAIR_WAIT_MS)).toBe('paired');
  expect(PAIR_WAIT_MS).toBe(70_000);
});
