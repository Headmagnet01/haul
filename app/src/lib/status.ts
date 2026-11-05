import type { IconName, State } from './types';

export type Tone = 'signal' | 'ok' | 'idle' | 'err' | 'none';
export interface Status {
  icon: IconName;
  word: string;
  tone: Tone;
}

const cap = (s: string) => s.charAt(0).toUpperCase() + s.slice(1);

/** Icon + word for a row. The word is never coloured except for errors; the icon carries the tone. */
export function statusOf(d: { state: State; error: string | null }): Status {
  switch (d.state) {
    case 'queued':
      return { icon: 'wait', word: 'Waiting', tone: 'none' };
    case 'running':
      return { icon: 'dl', word: 'Downloading', tone: 'signal' };
    case 'paused':
      return d.error ? { icon: 'pause', word: cap(d.error), tone: 'err' } : { icon: 'pause', word: 'Paused', tone: 'idle' };
    case 'completed':
      return { icon: 'ok', word: 'Complete', tone: 'ok' };
    case 'failed':
      return { icon: 'err', word: 'Failed', tone: 'err' };
    case 'link_expired':
      return { icon: 'err', word: 'Link expired', tone: 'err' };
    case 'offline':
      return { icon: 'err', word: 'Offline', tone: 'err' };
  }
}
