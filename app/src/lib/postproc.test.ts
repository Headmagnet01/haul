import { expect, test } from 'vitest';
import { STEP_TITLES, canProcess, packageWord, runStatus, stepIcon } from './postproc';
import type { DownloadView, Run, RunState, StepKind, StepState } from './types';

const KINDS: StepKind[] = ['verify', 'extract', 'sort', 'scan', 'notify'];

function run(state: RunState, steps: Partial<Record<StepKind, StepState>> = {}): Run {
  return {
    id: 1, packageId: 1, downloadId: null, state, updatedAt: '2026-11-06T10:00:00Z',
    steps: KINDS.map((kind) => ({ kind, state: steps[kind] ?? 'pending', detail: null })),
  };
}

test('step_titles', () => {
  expect(KINDS.map((k) => STEP_TITLES[k])).toEqual(['Verify checksum', 'Extract', 'Sort', 'Virus scan', 'Notify']);
});

test('package_word_for_every_state', () => {
  expect(packageWord(null)).toBeNull();
  expect(packageWord(run('held'))).toBe('Not processed');
  expect(packageWord(run('queued'))).toBe('Verifying');
  expect(packageWord(run('running', { verify: 'running' }))).toBe('Verifying');
  expect(packageWord(run('running', { verify: 'skipped', extract: 'running' }))).toBe('Extracting');
  expect(packageWord(run('running', { verify: 'done', extract: 'done', sort: 'running' }))).toBe('Sorting');
  expect(packageWord(run('running', { verify: 'done', extract: 'done', sort: 'done', scan: 'running' }))).toBe('Scanning');
  expect(packageWord(run('waiting'))).toBe('Re-downloading');
  expect(packageWord(run('needs_password'))).toBe('Needs password');
  expect(packageWord(run('failed'))).toBe('Needs attention');
  expect(packageWord(run('done'))).toBeNull();
});

test('run_status_tones', () => {
  expect(runStatus(run('running', { verify: 'running' }))).toEqual({ icon: 'ext', word: 'Verifying', tone: 'signal' });
  expect(runStatus(run('failed'))).toEqual({ icon: 'err', word: 'Needs attention', tone: 'err' });
  expect(runStatus(run('needs_password'))).toEqual({ icon: 'err', word: 'Needs password', tone: 'err' });
  expect(runStatus(run('held'))).toEqual({ icon: 'wait', word: 'Not processed', tone: 'none' });
  expect(runStatus(run('done'))).toBeNull();
  expect(runStatus(null)).toBeNull();
});

test('step_icons', () => {
  expect(stepIcon('done')).toBe('ok');
  expect(stepIcon('skipped')).toBe('ok');
  expect(stepIcon('failed')).toBe('err');
  expect(stepIcon('needs_password')).toBe('err');
  expect(stepIcon('running')).toBe('ext');
  expect(stepIcon('pending')).toBe('wait');
});

test('can_process', () => {
  const d = (state: DownloadView['state']) => ({ state }) as DownloadView;
  expect(canProcess([d('completed'), d('completed')], null)).toBe(true);
  expect(canProcess([d('completed'), d('running')], null)).toBe(false);
  expect(canProcess([], null)).toBe(false);
  expect(canProcess([d('completed')], run('held'))).toBe(true);
  expect(canProcess([d('completed')], run('done'))).toBe(false);
  expect(canProcess([d('completed')], run('failed'))).toBe(false);
});
