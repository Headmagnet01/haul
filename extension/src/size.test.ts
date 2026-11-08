import { expect, test } from 'vitest';
import { waitForSize } from './size';

const noSleep = async () => {};

test('returns_the_size_once_known', async () => {
  const seen = [-1, -1, 5_000_000];
  let reads = 0;
  const size = await waitForSize(async () => ({ totalBytes: seen[reads++] ?? 5_000_000, fileSize: -1 }), 15, noSleep);
  expect(size).toBe(5_000_000);
  expect(reads).toBe(3);
});

test('prefers_file_size', async () => {
  expect(await waitForSize(async () => ({ totalBytes: 10, fileSize: 20 }), 15, noSleep)).toBe(20);
});

test('gives_up_after_the_tries', async () => {
  let reads = 0;
  const size = await waitForSize(async () => (reads++, { totalBytes: -1, fileSize: -1 }), 15, noSleep);
  expect(size).toBeNull();
  expect(reads).toBe(15);
});

test('a_vanished_download_is_null', async () => {
  expect(await waitForSize(async () => undefined, 15, noSleep)).toBeNull();
});

test('read_errors_are_null', async () => {
  expect(await waitForSize(() => Promise.reject(new Error('gone')), 15, noSleep)).toBeNull();
});
