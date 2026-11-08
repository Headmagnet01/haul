import { expect, test } from 'vitest';
import { createMockBackend } from './backend-mock';
import { createWhenDoneState } from './when-done-state.svelte';

const flush = () => new Promise((r) => setTimeout(r, 0));

test('when_done_state_follows_backend', async () => {
  const b = createMockBackend({ tickMs: 60_000, demo: false });
  const toasts: string[] = [];
  const wd = createWhenDoneState(b, { showToast: (t) => toasts.push(t) });
  await flush();
  expect(wd.status).toEqual({ mode: 'nothing', countdown: null });
  await wd.set('shutdown');
  expect(wd.status.mode).toBe('shutdown');
  b.queueDrained();
  expect(wd.status).toEqual({ mode: 'shutdown', countdown: 60 });
  await wd.cancel();
  expect(wd.status).toEqual({ mode: 'nothing', countdown: null });
  await wd.set('sleep');
  b.queueDrained();
  await wd.now();
  expect(wd.status).toEqual({ mode: 'nothing', countdown: null });
  expect(b.calls).toContain('whenDoneNow');
  expect(toasts).toEqual([]);
});
