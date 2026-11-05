import { SvelteMap, SvelteSet } from 'svelte/reactivity';
import type { Backend } from './backend';
import { applyUpdate, buildList, planDrop, planMove, summary, type ListItem, type MovePlan, type Sel } from './model';
import { RangeSpeeds } from './speeds';
import type { AddResult, DownloadView, Snapshot, SpeedLimit, Update } from './types';

export const HISTORY = 240; // 60 s at 4 updates per second

export interface ToastMsg {
  id: number;
  text: string;
  action?: { label: string; run: () => void };
}

export type AppState = ReturnType<typeof createAppState>;

/** All Queue state in one rune-based object. DOM updates come only from `haul:update` batches. */
export function createAppState(backend: Backend) {
  let snapshot = $state<Snapshot>({ packages: [], downloads: [], speedLimit: { enabled: false, bps: 10_000_000 } });
  let byId = new Map<number, DownloadView>();
  const speeds = new SvelteMap<number, number>();
  const collapsed = new SvelteSet<number>();
  const hidden = new SvelteSet<number>();
  let selected = $state<number | null>(null);
  let selectedPkg = $state<number | null>(null);
  let filter = $state('');
  let toast = $state<ToastMsg | null>(null);
  let loaded = $state(false);
  let speedHistory = $state<number[]>([]);
  const lastSteal = new SvelteMap<number, number>();
  const rangeSpeeds = new RangeSpeeds();
  const ranges = new SvelteMap<number, { start: number; bps: number; done: boolean }[]>();
  let toastSeq = 0;
  let toastTimer: ReturnType<typeof setTimeout> | undefined;

  const list = $derived<ListItem[]>(buildList(snapshot, { collapsed, filter, hidden }));
  const visible = $derived(snapshot.downloads.filter((d) => !hidden.has(d.id)));
  const totals = $derived(summary(visible, speeds));

  async function load() {
    const snap = await backend.snapshot();
    snapshot = snap;
    byId = new Map(snapshot.downloads.map((d) => [d.id, d]));
    for (const id of [...speeds.keys()]) if (!byId.has(id)) speeds.delete(id);
    for (const id of [...ranges.keys()]) if (!byId.has(id)) { ranges.delete(id); rangeSpeeds.forget(id); }
    if (selected !== null && !byId.has(selected)) selected = null;
    if (selectedPkg !== null && !snapshot.packages.some((p) => p.id === selectedPkg)) selectedPkg = null;
    loaded = true;
  }

  const listeners = new Set<(u: Update) => void>();
  backend.onUpdate((u) => {
    const r = applyUpdate(byId, speeds, u);
    const now = performance.now();
    for (const p of u.progress) {
      rangeSpeeds.update(p.id, p.segments, now);
      ranges.set(p.id, rangeSpeeds.get(p.id));
    }
    for (const s of u.steals) lastSteal.set(s.id, s.bytes);
    let total = 0;
    for (const v of speeds.values()) total += v;
    speedHistory = [...speedHistory, total].slice(-HISTORY);
    listeners.forEach((cb) => cb(u));
    if (r.resync) void load();
  });

  function showToast(text: string, action?: ToastMsg['action'], ms = 4000) {
    clearTimeout(toastTimer);
    const id = ++toastSeq;
    toast = { id, text, action };
    toastTimer = setTimeout(() => {
      if (toast?.id === id) toast = null;
    }, ms);
  }

  async function addText(text: string): Promise<AddResult> {
    const r = await backend.addLinks(text);
    showToast(r.added === 0 ? 'No links found' : `Added ${r.added} download${r.added === 1 ? '' : 's'}`);
    if (r.added > 0) await load();
    return r;
  }

  function childrenOf(pkgId: number): DownloadView[] {
    return snapshot.downloads.filter((d) => d.packageId === pkgId && !hidden.has(d.id));
  }

  // Remove waits 6 s so it can be undone; only then does the backend delete anything.
  const UNDO_MS = 6000;
  let pendingRemoval: { ids: number[]; timer: ReturnType<typeof setTimeout> } | null = null;

  async function commitRemoval() {
    const p = pendingRemoval;
    if (!p) return;
    pendingRemoval = null;
    clearTimeout(p.timer);
    await backend.remove(p.ids);
    await load();
    p.ids.forEach((id) => hidden.delete(id));
  }

  function undo() {
    const p = pendingRemoval;
    if (!p) return;
    pendingRemoval = null;
    clearTimeout(p.timer);
    p.ids.forEach((id) => hidden.delete(id));
  }

  function removeWithUndo(ids: number[]) {
    if (!ids.length) return;
    if (pendingRemoval) void commitRemoval();
    ids.forEach((id) => hidden.add(id));
    const timer = setTimeout(() => void commitRemoval(), UNDO_MS);
    pendingRemoval = { ids, timer };
    const name = ids.length === 1 ? (byId.get(ids[0])?.name ?? '1 download') : `${ids.length} downloads`;
    showToast(`Removed ${name}`, { label: 'Undo', run: undo }, UNDO_MS);
  }

  async function applyPlan(p: MovePlan | null) {
    if (!p) return;
    for (const id of p.moved) {
      const d = byId.get(id);
      if (d && d.priority !== p.priority) await backend.setPriority(id, p.priority);
    }
    await backend.reorder(p.order);
    await load();
  }

  const currentSel = (): Sel | null =>
    selectedPkg !== null ? { kind: 'package', id: selectedPkg } : selected !== null ? { kind: 'file', id: selected } : null;

  /** Pauses what's moving, otherwise resumes. A package acts on all its files. */
  async function toggleIds(ds: DownloadView[]) {
    const moving = ds.filter((d) => d.state === 'running' || d.state === 'queued');
    if (moving.length) await backend.pause(moving.map((d) => d.id));
    else {
      const ids = ds.filter((d) => d.state !== 'completed').map((d) => d.id);
      if (ids.length) await backend.resume(ids);
    }
  }

  return {
    backend,
    get snapshot() { return snapshot; },
    get byId() { return byId; },
    speeds,
    collapsed,
    hidden,
    get selected() { return selected; },
    get selectedPkg() { return selectedPkg; },
    get filter() { return filter; },
    set filter(v: string) { filter = v; },
    get toast() { return toast; },
    set toast(v: ToastMsg | null) { toast = v; },
    get loaded() { return loaded; },
    get speedHistory() { return speedHistory; },
    lastSteal,
    ranges,
    async setSpeedLimit(l: SpeedLimit) {
      snapshot.speedLimit = { ...l };
      await backend.setSpeedLimit(l);
    },
    get list() { return list; },
    get totals() { return totals; },
    onUpdate(cb: (u: Update) => void) {
      listeners.add(cb);
      return () => listeners.delete(cb);
    },
    load,
    showToast,
    addText,
    childrenOf,
    select(id: number | null) {
      selected = id;
      selectedPkg = null;
    },
    selectPackage(id: number | null) {
      selectedPkg = id;
      selected = null;
    },
    /** Space on the selection. */
    async toggle(id?: number) {
      if (id !== undefined) {
        const d = byId.get(id);
        if (d) await toggleIds([d]);
      } else if (selectedPkg !== null) await toggleIds(childrenOf(selectedPkg));
      else if (selected !== null) {
        const d = byId.get(selected);
        if (d) await toggleIds([d]);
      }
    },
    toggleIds,
    removeWithUndo,
    undo,
    /** Delete on the selection: a package removes all its files. */
    removeSelected() {
      if (selectedPkg !== null) removeWithUndo(childrenOf(selectedPkg).map((d) => d.id));
      else if (selected !== null) removeWithUndo([selected]);
    },
    /** Alt+Up/Down on the selection. */
    async move(delta: -1 | 1) {
      const sel = currentSel();
      if (sel) await applyPlan(planMove(snapshot.downloads, sel, delta));
    },
    async drop(dragged: Sel, target: Sel) {
      await applyPlan(planDrop(snapshot.downloads, dragged, target));
    },
    setCollapsed(pkgId: number, closed: boolean) {
      if (closed) collapsed.add(pkgId);
      else collapsed.delete(pkgId);
    },
  };
}
