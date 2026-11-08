// Which downloads are Finished, and how the Finished view lists them.

import { buildList, type ListItem } from './model';
import type { DownloadView, Run, Snapshot } from './types';

const MONTHS = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];

/** Every file completed and post-processing ended (`done` or `held`) or never ran. */
export function isFinished(downloads: DownloadView[], run: Run | null): boolean {
  if (!downloads.length || downloads.some((d) => d.state !== 'completed')) return false;
  return run === null || run.state === 'done' || run.state === 'held';
}

/** Decided per package (its files and its run) and per lone download (its run by `downloadId`). */
export function finishedIds(snap: Snapshot, runs: Run[]): Set<number> {
  const out = new Set<number>();
  const byPkg = new Map<number, DownloadView[]>();
  const pkgs = new Set(snap.packages.map((p) => p.id));
  for (const d of snap.downloads) {
    if (d.packageId !== null && pkgs.has(d.packageId)) {
      const l = byPkg.get(d.packageId) ?? [];
      l.push(d);
      byPkg.set(d.packageId, l);
    } else if (isFinished([d], runs.find((r) => r.downloadId === d.id) ?? null)) out.add(d.id);
  }
  for (const [id, ds] of byPkg) {
    if (isFinished(ds, runs.find((r) => r.packageId === id) ?? null)) ds.forEach((d) => out.add(d.id));
  }
  return out;
}

/** `HH:MM` on the same local day as `now`, else `8 Nov 2026`; `''` for null. Computed at render time. */
export function finishedTime(iso: string | null, now: Date): string {
  if (!iso) return '';
  const t = new Date(iso);
  if (Number.isNaN(t.getTime())) return '';
  const p = (n: number) => String(n).padStart(2, '0');
  const sameDay = t.getFullYear() === now.getFullYear() && t.getMonth() === now.getMonth() && t.getDate() === now.getDate();
  return sameDay ? `${p(t.getHours())}:${p(t.getMinutes())}` : `${t.getDate()} ${MONTHS[t.getMonth()]} ${t.getFullYear()}`;
}

/** The folder the file was saved in. */
export function locationOf(d: DownloadView): string {
  const p = d.targetPath;
  if (!p) return d.targetDir;
  const i = Math.max(p.lastIndexOf('\\'), p.lastIndexOf('/'));
  return i > 0 ? p.slice(0, i) : d.targetDir;
}

/** Older rows have no `finishedAt` and sort by `createdAt`. */
const when = (d: DownloadView) => d.finishedAt ?? d.createdAt;

/** Finished rows newest first, grouped like Queue; packages stay collapsed unless in `expanded`. */
export function finishedList(
  snap: Snapshot,
  finished: Set<number>,
  opts: { expanded: Set<number>; filter: string; hidden: Set<number> },
): ListItem[] {
  const downloads = snap.downloads.filter((d) => finished.has(d.id)).sort((a, b) => when(b).localeCompare(when(a)) || b.id - a.id);
  const collapsed = new Set(snap.packages.map((p) => p.id).filter((id) => !opts.expanded.has(id)));
  return buildList({ ...snap, downloads }, { collapsed, filter: opts.filter, hidden: opts.hidden });
}
