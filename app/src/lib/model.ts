import type { DownloadView, PackageView, Snapshot, Update } from './types';

export type ListItem =
  | { kind: 'package'; pkg: PackageView; children: DownloadView[]; open: boolean }
  | { kind: 'file'; d: DownloadView; child: boolean };

export interface ListOpts {
  collapsed: Set<number>;
  filter: string;
  hidden: Set<number>;
}

/**
 * Rows in queue order. A package row sits where its first child would be, followed by its children
 * (unless collapsed). The filter matches file names; a package stays only if a child matches.
 */
export function buildList(snap: Snapshot, opts: ListOpts): ListItem[] {
  const q = opts.filter.trim().toLowerCase();
  const pkgs = new Map(snap.packages.map((p) => [p.id, p]));
  const visible = snap.downloads.filter((d) => !opts.hidden.has(d.id) && (!q || d.name.toLowerCase().includes(q)));
  const kids = new Map<number, DownloadView[]>();
  for (const d of visible) {
    if (d.packageId !== null && pkgs.has(d.packageId)) {
      const list = kids.get(d.packageId) ?? [];
      list.push(d);
      kids.set(d.packageId, list);
    }
  }
  const out: ListItem[] = [];
  const placed = new Set<number>();
  for (const d of visible) {
    const pid = d.packageId;
    if (pid === null || !pkgs.has(pid)) {
      out.push({ kind: 'file', d, child: false });
      continue;
    }
    if (placed.has(pid)) continue;
    placed.add(pid);
    const children = kids.get(pid) ?? [];
    const open = !opts.collapsed.has(pid);
    out.push({ kind: 'package', pkg: pkgs.get(pid)!, children, open });
    if (open) for (const c of children) out.push({ kind: 'file', d: c, child: true });
  }
  return out;
}

/** Applies a batched update in place. Anything unknown means the UI is stale: reload a snapshot. */
export function applyUpdate(byId: Map<number, DownloadView>, speeds: Map<number, number>, u: Update): { resync: boolean } {
  let resync = u.resync;
  for (const p of u.progress) {
    const d = byId.get(p.id);
    if (!d) {
      resync = true;
      continue;
    }
    d.written = p.written;
    d.size = p.size;
    d.segments = p.segments;
    speeds.set(p.id, p.speedBps);
  }
  for (const s of u.states) {
    const d = byId.get(s.id);
    if (!d) {
      resync = true;
      continue;
    }
    d.state = s.state;
    d.error = s.error;
    if (s.state !== 'running') speeds.delete(s.id);
  }
  for (const s of u.steals) if (!byId.has(s.id)) resync = true;
  return { resync };
}

export function summary(ds: DownloadView[], speeds: Map<number, number>) {
  let downloading = 0;
  let waiting = 0;
  let attention = 0;
  for (const d of ds) {
    if (d.state === 'running') downloading++;
    else if (d.state === 'queued') waiting++;
    else if (d.state === 'failed' || d.state === 'link_expired' || d.state === 'offline') attention++;
    else if (d.state === 'paused' && d.error) attention++;
  }
  let totalBps = 0;
  for (const v of speeds.values()) totalBps += v;
  return { downloading, waiting, attention, totalBps };
}

/** Moves `id` one place up (-1) or down (1). Out-of-range moves leave the order unchanged. */
export function moveIds(order: number[], id: number, delta: -1 | 1): number[] {
  const i = order.indexOf(id);
  const j = i + delta;
  if (i < 0 || j < 0 || j >= order.length) return [...order];
  const out = [...order];
  [out[i], out[j]] = [out[j], out[i]];
  return out;
}
