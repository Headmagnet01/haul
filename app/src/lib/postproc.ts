// Pure helpers for the after-download steps.

import type { Status } from './status';
import type { DownloadView, IconName, Run, StepKind, StepState } from './types';

export const STEP_TITLES: Record<StepKind, string> = {
  verify: 'Verify checksum',
  extract: 'Extract',
  sort: 'Sort',
  scan: 'Virus scan',
  notify: 'Notify',
};

const WORKING: Record<StepKind, string | null> = {
  verify: 'Verifying',
  extract: 'Extracting',
  sort: 'Sorting',
  scan: 'Scanning',
  notify: null,
};

const finished = (s: StepState) => s === 'done' || s === 'skipped';

/** What a package row says while its run is not done; null when there is nothing to say. */
export function packageWord(run: Run | null): string | null {
  if (!run) return null;
  switch (run.state) {
    case 'held':
      return 'Waiting to process';
    case 'waiting':
      return 'Re-downloading';
    case 'needs_password':
      return 'Needs password';
    case 'failed':
      return 'Needs attention';
    case 'done':
      return null;
    case 'queued':
    case 'running': {
      const step = run.steps.find((s) => s.state === 'running') ?? run.steps.find((s) => !finished(s.state));
      return step ? WORKING[step.kind] : 'Verifying';
    }
  }
}

/** Icon, word and tone for a row whose run is not done. */
export function runStatus(run: Run | null): Status | null {
  const word = packageWord(run);
  if (!run || word === null) return null;
  if (run.state === 'failed' || run.state === 'needs_password') return { icon: 'err', word, tone: 'err' };
  if (run.state === 'held') return { icon: 'wait', word, tone: 'none' };
  return { icon: 'ext', word, tone: 'signal' };
}

export function stepIcon(s: StepState): IconName {
  switch (s) {
    case 'done':
    case 'skipped':
      return 'ok';
    case 'failed':
    case 'needs_password':
      return 'err';
    case 'running':
      return 'ext';
    case 'pending':
      return 'wait';
  }
}

/** "Process" is offered when every file has finished and nothing has run yet (or auto mode held it). */
export function canProcess(downloads: Pick<DownloadView, 'state'>[], run: Run | null): boolean {
  return downloads.length > 0 && downloads.every((d) => d.state === 'completed') && (run === null || run.state === 'held');
}

/** The text of a rejected backend call (Tauri rejects with a string). */
export function errText(e: unknown): string {
  if (typeof e === 'string') return e;
  if (e instanceof Error) return e.message;
  return String(e);
}
