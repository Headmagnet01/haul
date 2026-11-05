// The mock backend's Collector: intake, simulated link checks, simple archive grouping, start and filters.
// Grouping covers `.partN.rar` and `.7z.NNN` only, and the rest of a batch is always "Pasted list · …";
// the real rules live in the engine (crates/engine/src/grabber).

import type {
  ArchiveKind, CollectedLink, CollectorFilters, CollectorPackage, CollectorSnapshot, DupKind, IntakeResult, LinkStatus,
} from './types';

const FILTERS_KEY = 'haul.mock.collectorFilters';
const WATCH_KEY = 'haul.mock.watchClipboard';
const DEFAULT_FILTERS: CollectorFilters = { html: true, images: true, small: false, duplicates: false, exts: [] };

export interface StartGroup {
  /** The package key, or null for a lone link. */
  key: string | null;
  name: string | null;
  links: CollectedLink[];
}

export interface MockCollectorOpts {
  checkMs: number;
  demo: boolean;
  /** The Collector changed; the backend sends `collector: true` on its next tick. */
  changed: () => void;
  /** Whether a URL is already a download. */
  downloaded: (url: string) => DupKind | null;
  nameFromUrl: (url: string) => string;
  sizeFor: (url: string) => number;
  pastedName: () => string;
  nowIso: () => string;
}

function load<T>(key: string, fallback: T): T {
  try {
    const v = globalThis.sessionStorage?.getItem(key);
    if (v) return JSON.parse(v) as T;
  } catch {
    /* storage unavailable */
  }
  return fallback;
}

function save(key: string, v: unknown) {
  try {
    globalThis.sessionStorage?.setItem(key, JSON.stringify(v));
  } catch {
    /* storage unavailable */
  }
}

function archivePart(name: string): { base: string; kind: ArchiveKind; part: number } | null {
  const rar = /^(.+)\.part(\d+)\.rar$/i.exec(name);
  if (rar) return { base: rar[1], kind: 'rar', part: Number(rar[2]) };
  const sz = /^(.+)\.7z\.(\d{3,})$/i.exec(name);
  if (sz) return { base: sz[1], kind: '7z', part: Number(sz[2]) };
  return null;
}

function contentTypeFor(name: string): string {
  const e = name.split('.').pop()?.toLowerCase() ?? '';
  if (['html', 'htm', 'php'].includes(e)) return 'text/html';
  if (['png', 'jpg', 'jpeg', 'gif', 'webp'].includes(e)) return `image/${e === 'jpg' ? 'jpeg' : e}`;
  return 'application/octet-stream';
}

function checkResult(url: string): { status: LinkStatus; message: string | null } {
  if (url.includes('404')) return { status: 'offline', message: '404' };
  if (url.includes('expired')) return { status: 'expired', message: '403' };
  if (url.includes('unreachable')) return { status: 'error', message: 'timed out' };
  return { status: 'online', message: null };
}

export function createMockCollector(o: MockCollectorOpts) {
  let links: (CollectedLink & { dup?: DupKind })[] = [];
  let packages: CollectorPackage[] = [];
  let filters: CollectorFilters = load(FILTERS_KEY, DEFAULT_FILTERS);
  let watch: boolean = load(WATCH_KEY, false);
  let nextLink = 1;
  let nextBatch = 1;

  function check(l: CollectedLink) {
    setTimeout(() => {
      const live = links.find((x) => x.id === l.id);
      if (!live || live.status !== 'checking') return;
      const r = checkResult(live.url);
      live.status = r.status;
      live.message = r.message;
      live.size = r.status === 'online' ? o.sizeFor(live.url) : null;
      live.contentType = r.status === 'online' ? contentTypeFor(live.name) : null;
      o.changed();
    }, o.checkMs);
  }

  /** Adds one batch, grouped the simple way. Each link may carry overrides (used by the demo seed). */
  function addBatch(urls: string[], source: string, label: string, over: Partial<CollectedLink & { dup: DupKind }>[] = [],
    names: (string | null)[] = []) {
    const batchId = nextBatch++;
    const made = urls.map((url, i) => ({
      id: nextLink++, batchId, url, name: o.nameFromUrl(url), size: null, contentType: null,
      status: 'checking' as LinkStatus, message: null, source, duplicate: null, packageKey: null as string | null,
      ...over[i],
    }));
    const sets = new Map<string, { kind: ArchiveKind; base: string; items: { l: (typeof made)[0]; part: number }[] }>();
    const rest: typeof made = [];
    for (const l of made) {
      const p = archivePart(l.name);
      if (!p) {
        rest.push(l);
        continue;
      }
      const key = `${batchId}:${p.kind}:${p.base.toLowerCase()}`;
      if (!sets.has(key)) sets.set(key, { kind: p.kind, base: p.base, items: [] });
      sets.get(key)!.items.push({ l, part: p.part });
    }
    const ordered: typeof made = [];
    for (const [key, set] of sets) {
      packages.push({ key, name: set.base, archive: set.kind, source, createdAt: o.nowIso() });
      set.items.sort((a, b) => a.part - b.part).forEach(({ l }) => {
        l.packageKey = key;
        ordered.push(l);
      });
    }
    if (rest.length >= 2) {
      const key = `${batchId}:rest`;
      packages.push({ key, name: names[0] ?? label, archive: null, source, createdAt: o.nowIso() });
      rest.forEach((l) => (l.packageKey = key));
    }
    ordered.push(...rest);
    links.push(...ordered);
    ordered.filter((l) => l.status === 'checking').forEach(check);
  }

  function prune() {
    const used = new Set(links.map((l) => l.packageKey));
    packages = packages.filter((p) => used.has(p.key));
    o.changed();
  }

  if (o.demo) {
    const cam = (n: number) => `https://download.blender.org/demo/movies/caminandes-3-4k.7z.00${n}`;
    const on = (size: number) => ({ status: 'online' as LinkStatus, size, contentType: 'application/octet-stream' });
    addBatch([1, 2, 3, 4].map(cam), 'clipboard', o.pastedName(),
      [on(2_000_000_000), on(2_000_000_000), on(2_000_000_000), { size: 1_800_000_000 }]);
    addBatch(
      ['https://storage.googleapis.com/openimages/v7/openimages-v7-sample.csv',
        'https://commonvoice.mozilla.org/datasets/mozilla-common-voice-en.tar.gz',
        'https://files.example.org/docs/readme-old.pdf'],
      'paste', 'Pasted list · 5 Nov 18:40',
      [on(188_000_000), { ...on(214_000_000), dup: 'downloaded' }, { status: 'offline', message: '404' }]);
    addBatch(
      ['https://downloads.tuxfamily.org/godotengine/4.4/Godot_v4.4-stable_win64.exe.zip',
        'https://downloads.tuxfamily.org/godotengine/4.4/Godot_v4.4-stable_export_templates.tpz'],
      'paste', o.pastedName(), [on(58_000_000), on(88_000_000)], ['godot-4.4-stable']);
  }

  return {
    snapshot(): CollectorSnapshot {
      const seen = new Set<string>();
      const out = links.map(({ dup, ...l }) => {
        const duplicate = dup ?? o.downloaded(l.url) ?? (seen.has(l.url) ? 'collector' : null);
        seen.add(l.url);
        return { ...l, duplicate };
      });
      return structuredClone({ packages, links: out, filters, watchClipboard: watch });
    },
    intake(urls: string[], source: string): IntakeResult {
      const waiting = new Set(links.map((l) => l.url));
      const fresh = urls.filter((u) => !waiting.has(u));
      if (fresh.length) {
        addBatch(fresh, source, o.pastedName());
        o.changed();
      }
      return { added: fresh.length, skipped: urls.length - fresh.length };
    },
    /** Takes startable links out of the Collector, grouped by package in display order. */
    take(ids: number[]): StartGroup[] {
      const groups: StartGroup[] = [];
      const taken = links.filter((l) => ids.includes(l.id) && l.status !== 'offline' && l.status !== 'expired');
      for (const l of taken) {
        let g = groups.find((x) => x.key === l.packageKey && l.packageKey !== null);
        if (!g) {
          g = { key: l.packageKey, name: packages.find((p) => p.key === l.packageKey)?.name ?? null, links: [] };
          groups.push(g);
        }
        g.links.push(l);
      }
      links = links.filter((l) => !taken.includes(l));
      if (taken.length) prune();
      return groups;
    },
    remove(ids: number[]) {
      links = links.filter((l) => !ids.includes(l.id));
      prune();
    },
    recheck(ids: number[]) {
      for (const l of links.filter((x) => ids.includes(x.id))) {
        Object.assign(l, { status: 'checking', message: null });
        check(l);
      }
      o.changed();
    },
    setFilters(f: CollectorFilters) {
      filters = structuredClone(f);
      save(FILTERS_KEY, filters);
    },
    setWatch(on: boolean) {
      watch = on;
      save(WATCH_KEY, watch);
    },
  };
}
