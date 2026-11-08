import { expect, test } from 'vitest';
import { createMockBackend } from './backend-mock';
import { createSettingsState } from './settings-state.svelte';

test('loads_then_saves', async () => {
  const b = createMockBackend({ tickMs: 250, demo: false });
  const changed: string[] = [];
  const s = createSettingsState(b, { onChanged: (p) => changed.push(...Object.keys(p)) });
  await s.load();
  expect(s.settings?.maxParallel).toBe(3);
  expect(await s.save({ maxParallel: 5 })).toBe(true);
  expect(s.settings?.maxParallel).toBe(5);
  expect(changed).toEqual(['maxParallel']);
});

test('save_failure_reverts', async () => {
  const b = createMockBackend({ tickMs: 250, demo: false });
  const s = createSettingsState(b, {});
  await s.load();
  const before = s.settings?.downloadDir;
  expect(await s.save({ downloadDir: 'Z:\\Refused' })).toBe(false);
  expect(s.settings?.downloadDir).toBe(before);
  expect(s.error.downloadDir).toMatch(/^downloadDir: /);
  expect(await s.save({ maxParallel: 4 })).toBe(true);
  expect(s.error.downloadDir).toBeDefined();
  expect(await s.save({ downloadDir: 'C:\\Users\\you\\Videos' })).toBe(true);
  expect(s.error.downloadDir).toBeUndefined();
});

test('mirror_shows_change_from_status_bar', async () => {
  const b = createMockBackend({ tickMs: 250, demo: false });
  const s = createSettingsState(b, {});
  await s.load();
  s.mirror({ watchClipboard: true });
  expect(s.settings?.watchClipboard).toBe(true);
});
