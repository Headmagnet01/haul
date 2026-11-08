import { expect, test } from 'vitest';
import { handOver, type DownloadsApi, type HaulClient } from './handover';

const BODY = { url: 'https://a.org/f.zip' };

function downloads(opts: { pauseFails?: boolean } = {}) {
  const log: string[] = [];
  const d: DownloadsApi = {
    pause: async (id) => {
      log.push(`pause ${id}`);
      if (opts.pauseFails) throw new Error('Download must be in progress');
    },
    resume: async (id) => void log.push(`resume ${id}`),
    cancel: async (id) => void log.push(`cancel ${id}`),
    erase: async (id) => void log.push(`erase ${id}`),
  };
  return { d, log };
}

const haul = (log: string[], r: 'ok' | 'unpaired' | 'failed'): HaulClient => ({
  capture: async () => {
    log.push('capture');
    return r;
  },
});

test('sent_cancels_and_erases_after_ok', async () => {
  const { d, log } = downloads();
  expect(await handOver(7, BODY, d, haul(log, 'ok'))).toBe('sent');
  expect(log).toEqual(['pause 7', 'capture', 'cancel 7', 'erase 7']);
});

test('failed_resumes', async () => {
  const { d, log } = downloads();
  expect(await handOver(7, BODY, d, haul(log, 'failed'))).toBe('kept');
  expect(log).toEqual(['pause 7', 'capture', 'resume 7']);
});

test('unpaired_resumes', async () => {
  const { d, log } = downloads();
  expect(await handOver(7, BODY, d, haul(log, 'unpaired'))).toBe('kept');
  expect(log).toEqual(['pause 7', 'capture', 'resume 7']);
});

test('capture_throwing_resumes', async () => {
  const { d, log } = downloads();
  const bad: HaulClient = { capture: async () => Promise.reject(new Error('boom')) };
  expect(await handOver(7, BODY, d, bad)).toBe('kept');
  expect(log).toEqual(['pause 7', 'resume 7']);
});

test('cancel_never_before_ok', async () => {
  const { d, log } = downloads();
  let release!: (r: 'ok') => void;
  const slow: HaulClient = { capture: () => new Promise((res) => (release = res)) };
  const done = handOver(7, BODY, d, slow);
  await Promise.resolve();
  await Promise.resolve();
  expect(log).toEqual(['pause 7']);
  release('ok');
  expect(await done).toBe('sent');
  expect(log).toEqual(['pause 7', 'cancel 7', 'erase 7']);
});

test('handover_leaves_finished_download_alone', async () => {
  const { d, log } = downloads({ pauseFails: true });
  expect(await handOver(7, BODY, d, haul(log, 'ok'))).toBe('gone');
  expect(log).toEqual(['pause 7']);
});
