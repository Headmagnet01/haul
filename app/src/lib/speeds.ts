import type { Seg } from './types';

interface Sample {
  pos: number;
  t: number;
  bps: number;
  done: boolean;
}

/**
 * Per-range speeds measured from successive segment lists, keyed by range start.
 * The engine reports ranges, not connections, so a fresh range (a steal) reads 0 until its second sample.
 */
export class RangeSpeeds {
  private byId = new Map<number, Map<number, Sample>>();

  update(id: number, segs: Seg[], nowMs: number): void {
    const prev = this.byId.get(id) ?? new Map<number, Sample>();
    const next = new Map<number, Sample>();
    for (const [start, pos, end] of segs) {
      const p = prev.get(start);
      const done = pos >= end;
      let bps = 0;
      if (p && !done) {
        const dt = (nowMs - p.t) / 1000;
        bps = dt > 0 ? Math.max(0, (pos - p.pos) / dt) : p.bps;
      }
      next.set(start, { pos, t: nowMs, bps, done });
    }
    this.byId.set(id, next);
  }

  get(id: number): { start: number; bps: number; done: boolean }[] {
    const m = this.byId.get(id);
    if (!m) return [];
    return [...m.entries()]
      .sort((a, b) => a[0] - b[0])
      .map(([start, s]) => ({ start, bps: s.done ? 0 : s.bps, done: s.done }));
  }

  forget(id: number): void {
    this.byId.delete(id);
  }
}
