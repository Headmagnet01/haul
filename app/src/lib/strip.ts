import type { DownloadView, Seg, State } from './types';

export type Cell = { flex: number; fill: number };
export type StripKind = 'active' | 'done' | 'paused' | 'error' | 'queued';

const clamp01 = (x: number) => (Number.isFinite(x) ? Math.min(1, Math.max(0, x)) : 0);

/**
 * One cell per range, width proportional to the range, fill to its progress.
 * Adjacent fully written ranges merge. Unknown size gives one empty cell (never divide by it).
 */
export function stripCells(segments: Seg[], size: number | null, state: State): Cell[] {
  const done = state === 'completed';
  if (size === null || segments.length === 0 || done) return [{ flex: 1, fill: done ? 1 : 0 }];
  const cells: Cell[] = [];
  let prevFull = false;
  for (const [start, pos, end] of [...segments].sort((a, b) => a[0] - b[0])) {
    const len = Math.max(0, end - start);
    const full = pos >= end;
    if (full && prevFull) {
      cells[cells.length - 1].flex += len;
    } else if (len > 0 || full) {
      cells.push({ flex: len, fill: full ? 1 : clamp01((pos - start) / len) });
    }
    prevFull = full;
  }
  const out = cells.filter((c) => c.flex > 0);
  return out.length ? out : [{ flex: 1, fill: 0 }];
}

/** A package strip: one cell per file. */
export function packageCells(children: DownloadView[]): Cell[] {
  return children.map((c) => {
    const flex = c.size !== null && c.size > 0 ? c.size : 1;
    const fill = c.state === 'completed' ? 1 : c.size !== null && c.size > 0 ? clamp01(c.written / c.size) : 0;
    return { flex, fill };
  });
}

export function stripKind(state: State, error: string | null): StripKind {
  switch (state) {
    case 'running':
      return 'active';
    case 'completed':
      return 'done';
    case 'paused':
      return error ? 'error' : 'paused';
    case 'queued':
      return 'queued';
    default:
      return 'error';
  }
}
