import { SvelteMap, SvelteSet } from 'svelte/reactivity';
import type { Backend } from './backend';
import { applyUpdate, buildList, summary, type ListItem } from './model';
import type { AddResult, DownloadView, Snapshot, Update } from './types';

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
    if (selected !== null && !byId.has(selected)) selected = null;
    if (selectedPkg !== null && !snapshot.packages.some((p) => p.id === selectedPkg)) selectedPkg = null;
    loaded = true;
  }

  const listeners = new Set<(u: Update) => void>();
  backend.onUpdate((u) => {
    const r = applyUpdate(byId, speeds, u);
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
    setCollapsed(pkgId: number, closed: boolean) {
      if (closed) collapsed.add(pkgId);
      else collapsed.delete(pkgId);
    },
  };
}
