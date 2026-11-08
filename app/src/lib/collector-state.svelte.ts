import { SvelteSet } from 'svelte/reactivity';
import type { Backend } from './backend';
import { buildCollectorList, onlineIds, startableIds, subLine, type CItem } from './collector';
import type { CollectorFilters, CollectorSnapshot, Update } from './types';
import { errText } from './postproc';

export type CSel = { kind: 'package'; key: string } | { kind: 'link'; id: number };
export type Chip = 'html' | 'images' | 'small' | 'duplicates';
export type CollectorState = ReturnType<typeof createCollectorState>;

const UNDO_MS = 6000;
const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? '' : 's'}`;

export function createCollectorState(
  backend: Backend,
  ui: { showToast: (text: string, action?: { label: string; run: () => void }, ms?: number) => void; onStarted: () => void },
) {
  let snapshot = $state<CollectorSnapshot>({
    packages: [], links: [], filters: { html: true, images: true, small: false, duplicates: false, exts: [] },
    watchClipboard: false,
  });
  let loaded = $state(false);
  let selected = $state<CSel | null>(null);
  const collapsed = new SvelteSet<string>();
  const removed = new SvelteSet<number>();

  const items = $derived<CItem[]>(buildCollectorList(snapshot, collapsed, removed));
  const sub = $derived(subLine(snapshot, removed));
  const count = $derived(snapshot.links.filter((l) => !removed.has(l.id)).length);

  // Newest load wins; a load requested while one is in flight runs once afterwards.
  let gen = 0;
  let inflight = false;
  let again = false;

  async function load() {
    if (inflight) {
      again = true;
      return;
    }
    inflight = true;
    const my = ++gen;
    try {
      const s = await backend.collectorSnapshot();
      if (my !== gen) return;
      snapshot = s;
      const ids = new Set(s.links.map((l) => l.id));
      const undoable = new Set(pending?.ids ?? []);
      for (const id of [...removed]) if (!ids.has(id) && !undoable.has(id)) removed.delete(id);
      if (selected?.kind === 'link' && !ids.has(selected.id)) selected = null;
      if (selected?.kind === 'package' && !s.packages.some((p) => p.key === (selected as { key: string }).key)) selected = null;
      loaded = true;
    } finally {
      inflight = false;
      if (again) {
        again = false;
        void load();
      }
    }
  }

  let pending: { ids: number[]; timer: ReturnType<typeof setTimeout> } | null = null;

  async function commit() {
    const p = pending;
    if (!p) return;
    pending = null;
    clearTimeout(p.timer);
    try {
      await backend.collectorRemove(p.ids);
    } catch (e) {
      p.ids.forEach((id) => removed.delete(id));
      ui.showToast(`Couldn't remove: ${errText(e)}`);
    }
    await load();
  }

  function undo() {
    const p = pending;
    if (!p) return;
    pending = null;
    clearTimeout(p.timer);
    p.ids.forEach((id) => removed.delete(id));
  }

  function hideWithUndo(ids: number[], text: string) {
    if (!ids.length) return;
    if (pending) void commit();
    ids.forEach((id) => removed.add(id));
    pending = { ids, timer: setTimeout(() => void commit(), UNDO_MS) };
    ui.showToast(text, { label: 'Undo', run: undo }, UNDO_MS);
  }

  function removeWithUndo(ids: number[]) {
    const name = ids.length === 1 ? (snapshot.links.find((l) => l.id === ids[0])?.name ?? '1 link') : `${ids.length} links`;
    hideWithUndo(ids, `Removed ${name}`);
  }

  // One start at a time: a double click or a second Enter while the first is in flight does nothing.
  let starting = false;
  async function start(ids: number[]) {
    if (!ids.length || starting) return;
    starting = true;
    try {
      const r = await backend.collectorStart(ids);
      ui.showToast(`Started ${plural(r.started, 'download')}`);
      await load();
      ui.onStarted();
    } finally {
      starting = false;
    }
  }

  const linksOf = (key: string) => snapshot.links.filter((l) => l.packageKey === key && !removed.has(l.id));

  /** A local change wins over any snapshot already in flight, which was read before it. */
  async function saveLocal(apply: () => () => void, save: () => Promise<void>) {
    gen++;
    const rollback = apply();
    try {
      await save();
    } catch {
      rollback();
    }
    void load();
  }

  /** The filters as a plain object, not the reactive proxy, so any backend can copy it. */
  const plainFilters = (): CollectorFilters => $state.snapshot(snapshot.filters);

  function setFilters(f: CollectorFilters) {
    return saveLocal(() => {
      const before = snapshot.filters;
      snapshot.filters = f;
      return () => (snapshot.filters = before);
    }, () => backend.setCollectorFilters(f));
  }

  return {
    get snapshot() { return snapshot; },
    get loaded() { return loaded; },
    get items() { return items; },
    get sub() { return sub; },
    /** Links waiting, for the rail badge. */
    get count() { return count; },
    get selected() { return selected; },
    collapsed,
    removed,
    load,
    onUpdate(u: Update) {
      if (u.collector) void load();
    },
    select(s: CSel | null) {
      selected = s;
    },
    setCollapsed(key: string, closed: boolean) {
      if (closed) collapsed.add(key);
      else collapsed.delete(key);
    },
    linksOf,
    startPackage: (key: string) => start(startableIds(linksOf(key), snapshot.filters)),
    startLink: (id: number) => start(startableIds(snapshot.links.filter((l) => l.id === id), snapshot.filters)),
    startAllOnline: () => start(onlineIds(snapshot, removed)),
    /** Enter on the selection. */
    async startSelected() {
      if (selected?.kind === 'package') await start(startableIds(linksOf(selected.key), snapshot.filters));
      else if (selected?.kind === 'link') {
        const id = selected.id;
        await start(startableIds(snapshot.links.filter((l) => l.id === id), snapshot.filters));
      }
    },
    removeWithUndo,
    undo,
    /** Delete on the selection: a package removes all its links. */
    removeSelected() {
      if (selected?.kind === 'package') {
        // The next package takes the selection (the one before it at the end), as in the Queue.
        const keys = items.flatMap((i) => (i.kind === 'package' ? [i.pkg.key] : []));
        const at = keys.indexOf(selected.key);
        const next = keys[at + 1] ?? keys[at - 1];
        removeWithUndo(linksOf(selected.key).map((l) => l.id));
        selected = next === undefined ? null : { kind: 'package', key: next };
      } else if (selected?.kind === 'link') removeWithUndo([selected.id]);
    },
    clearOffline() {
      const ids = snapshot.links
        .filter((l) => !removed.has(l.id) && (l.status === 'offline' || l.status === 'expired'))
        .map((l) => l.id);
      hideWithUndo(ids, `Cleared ${plural(ids.length, 'offline link')}`);
    },
    async recheck(id: number) {
      await backend.collectorRecheck([id]);
      await load();
    },
    toggleChip: (chip: Chip) => setFilters({ ...plainFilters(), [chip]: !snapshot.filters[chip] }),
    addExt(raw: string) {
      const e = raw.trim().replace(/^\.+/, '').toLowerCase();
      if (!e || snapshot.filters.exts.includes(e)) return Promise.resolve();
      return setFilters({ ...plainFilters(), exts: [...snapshot.filters.exts, e] });
    },
    removeExt: (e: string) => setFilters({ ...plainFilters(), exts: snapshot.filters.exts.filter((x) => x !== e) }),
    setWatchClipboard(on: boolean) {
      return saveLocal(() => {
        const before = snapshot.watchClipboard;
        snapshot.watchClipboard = on;
        return () => (snapshot.watchClipboard = before);
      }, async () => void (await backend.setSettings({ watchClipboard: on })));
    },
  };
}
