import { expect, test } from 'vitest';
import { whenLabel } from './browsers';

const now = new Date('2026-11-08T15:00:00Z');

test('when_label_days', () => {
  expect(whenLabel('2026-11-08T09:00:00Z', now)).toBe('today');
  expect(whenLabel('2026-11-07T09:00:00Z', now)).toBe('yesterday');
  expect(whenLabel('2026-11-04T09:00:00Z', now)).toBe('4 days ago');
  expect(whenLabel('2026-10-01T09:00:00Z', now)).toBe('1 Oct 2026');
  expect(whenLabel(null, now)).toBe('never');
});
