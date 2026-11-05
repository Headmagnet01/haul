import type { DownloadView, PackageView, PriorityName, Snapshot, Update } from './types';

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

/** Applies a batched update in place. Anything unknown, or a finished download, means: reload a snapshot. */
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
    // The saved file's final path (and a late-known size) only arrive with a snapshot.
    if (s.state === 'completed') resync = true;
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

export type Sel = { kind: 'file' | 'package'; id: number };
export interface MovePlan {
  /** Every download id in its new queue order. */
  order: number[];
  moved: number[];
  /** The priority the moved rows take from their new neighbour. */
  priority: PriorityName;
}

interface Block {
  pkg: number | null;
  ids: number[];
}

/** Queue order as blocks: a loose file, or a whole package placed at its first child. */
function blocksOf(ds: DownloadView[]): Block[] {
  const out: Block[] = [];
  const byPkg = new Map<number, Block>();
  for (const d of ds) {
    if (d.packageId === null) {
      out.push({ pkg: null, ids: [d.id] });
      continue;
    }
    let b = byPkg.get(d.packageId);
    if (!b) {
      b = { pkg: d.packageId, ids: [] };
      byPkg.set(d.packageId, b);
      out.push(b);
    }
    b.ids.push(d.id);
  }
  return out;
}

function locate(blocks: Block[], ds: DownloadView[], sel: Sel): { bi: number; child: number } | null {
  if (sel.kind === 'package') {
    const bi = blocks.findIndex((b) => b.pkg === sel.id);
    return bi < 0 ? null : { bi, child: -1 };
  }
  const d = ds.find((x) => x.id === sel.id);
  if (!d) return null;
  const bi = blocks.findIndex((b) => b.ids.includes(sel.id));
  if (bi < 0) return null;
  return { bi, child: d.packageId === null ? -1 : blocks[bi].ids.indexOf(sel.id) };
}

function plan(ds: DownloadView[], blocks: Block[], moved: number[], neighbour: number): MovePlan {
  const prio = ds.find((d) => d.id === neighbour)?.priority ?? 'normal';
  return { order: blocks.flatMap((b) => b.ids), moved, priority: prio };
}

/** Alt+Up/Down: a file or package moves one place; a file inside a package moves among its siblings. */
export function planMove(ds: DownloadView[], sel: Sel, delta: -1 | 1): MovePlan | null {
  const blocks = blocksOf(ds);
  const at = locate(blocks, ds, sel);
  if (!at) return null;
  if (at.child >= 0) {
    const ids = blocks[at.bi].ids;
    const j = at.child + delta;
    if (j < 0 || j >= ids.length) return null;
    const neighbour = ids[j];
    [ids[at.child], ids[j]] = [ids[j], ids[at.child]];
    return plan(ds, blocks, [sel.id], neighbour);
  }
  const j = at.bi + delta;
  if (j < 0 || j >= blocks.length) return null;
  const neighbour = blocks[j].ids[0];
  const moved = [...blocks[at.bi].ids];
  [blocks[at.bi], blocks[j]] = [blocks[j], blocks[at.bi]];
  return plan(ds, blocks, moved, neighbour);
}

/** Drag and drop: the dragged file or package takes the target's place (after it when moving down, before it when moving up). */
export function planDrop(ds: DownloadView[], dragged: Sel, target: Sel): MovePlan | null {
  const blocks = blocksOf(ds);
  const from = locate(blocks, ds, dragged);
  const to = locate(blocks, ds, target);
  if (!from || !to) return null;
  if (from.child >= 0 && to.child >= 0 && from.bi === to.bi) {
    if (from.child === to.child) return null;
    const ids = blocks[from.bi].ids;
    const neighbour = ids[to.child];
    const down = from.child < to.child;
    ids.splice(from.child, 1);
    ids.splice(ids.indexOf(neighbour) + (down ? 1 : 0), 0, dragged.id);
    return plan(ds, blocks, [dragged.id], neighbour);
  }
  if (from.bi === to.bi) return null;
  const neighbour = blocks[to.bi].ids[0];
  const down = from.bi < to.bi;
  let moving: Block;
  if (from.child >= 0) {
    // A file dragged out of its package keeps its package; only its place in the queue changes.
    blocks[from.bi].ids.splice(from.child, 1);
    moving = { pkg: null, ids: [dragged.id] };
  } else {
    moving = blocks.splice(from.bi, 1)[0];
  }
  const ti = blocks.findIndex((b) => b.ids.includes(neighbour));
  blocks.splice(ti + (down ? 1 : 0), 0, moving);
  return plan(ds, blocks, moving.ids, neighbour);
}
