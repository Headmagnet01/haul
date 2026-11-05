import { expect, test } from 'vitest';
import { statusOf } from './status';
import type { State } from './types';

test('status table', () => {
  const s = (state: State, error: string | null = null) => statusOf({ state, error });
  expect(s('queued')).toEqual({ icon: 'wait', word: 'Waiting', tone: 'none' });
  expect(s('running')).toEqual({ icon: 'dl', word: 'Downloading', tone: 'signal' });
  expect(s('paused')).toEqual({ icon: 'pause', word: 'Paused', tone: 'idle' });
  expect(s('paused', 'disk full')).toEqual({ icon: 'pause', word: 'Disk full', tone: 'err' });
  expect(s('completed')).toEqual({ icon: 'ok', word: 'Complete', tone: 'ok' });
  expect(s('failed', 'http status 500')).toEqual({ icon: 'err', word: 'Failed', tone: 'err' });
  expect(s('link_expired')).toEqual({ icon: 'err', word: 'Link expired', tone: 'err' });
  expect(s('offline')).toEqual({ icon: 'err', word: 'Offline', tone: 'err' });
});
