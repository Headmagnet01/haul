import { expect, test } from 'vitest';
import { createMockBackend } from './backend-mock';
import { createUpdateState } from './update-state.svelte';

const flush = () => new Promise((r) => setTimeout(r, 0));
const TAG = 'https://github.com/meraaki410/haul/releases/tag/v0.2.0';

test('up_to_date_message', async () => {
  const b = createMockBackend({ tickMs: 60_000, demo: false });
  const u = createUpdateState(b);
  await u.load();
  expect(u.message()).toBe('');
  const p = u.checkNow();
  expect(u.checking).toBe(true);
  await p;
  expect(u.checking).toBe(false);
  expect(u.message()).toBe('You have the latest version (0.1.0).');
  expect(b.calls).toContain('checkUpdatesNow');
});

test('newer_message', async () => {
  const b = createMockBackend({ tickMs: 60_000, demo: false, update: '0.2.0' });
  const u = createUpdateState(b);
  await u.load();
  expect(u.status?.latest).toBe('0.2.0');
  expect(u.status?.url).toBe(TAG);
  expect(u.message()).toBe('Version 0.2.0 is available.');
});

test('error_message', async () => {
  const b = createMockBackend({ tickMs: 60_000, demo: false });
  const failing = {
    ...b,
    checkUpdatesNow: async () => ({ current: '0.1.0', latest: null, url: null, checkedAt: 't', error: 'GitHub answered 403' }),
  };
  const u = createUpdateState(failing);
  await u.checkNow();
  expect(u.message()).toBe("Couldn't check: GitHub answered 403.");
});

test('rejected_check_shows_reason', async () => {
  const b = createMockBackend({ tickMs: 60_000, demo: false });
  const u = createUpdateState({ ...b, checkUpdatesNow: () => Promise.reject('offline') });
  await u.checkNow();
  expect(u.checking).toBe(false);
  expect(u.message()).toBe("Couldn't check: offline.");
});

test('update_event_sets_latest', async () => {
  const b = createMockBackend({ tickMs: 60_000, demo: false });
  const u = createUpdateState(b);
  await u.load();
  expect(u.status?.latest).toBeNull();
  b.updateAvailable('0.3.0');
  await flush();
  expect(u.status?.latest).toBe('0.3.0');
  expect(u.status?.url).toBe('https://github.com/meraaki410/haul/releases/tag/v0.3.0');
  expect(u.message()).toBe('Version 0.3.0 is available.');
});

test('open_goes_to_backend', async () => {
  const b = createMockBackend({ tickMs: 60_000, demo: false, update: '0.2.0' });
  const u = createUpdateState(b);
  await u.load();
  await u.open();
  expect(b.calls).toContain(`openRelease ${TAG}`);
});
