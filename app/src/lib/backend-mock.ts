// A small simulated engine for the browser and the tests. Deterministic: sizes and speeds come from the URL.

import type { Backend } from './backend';
import type { AddResult, DownloadView, PackageView, PriorityName, Seg, Snapshot, SpeedLimit, State, Update } from './types';

const MAX_PARALLEL = 3;
const LIMIT_KEY = 'haul.mock.speedLimit';
const MONTHS = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];
const PRIO: Record<PriorityName, number> = { high: 0, normal: 1, low: 2 };

interface Sim {
  /** Bytes per second for each range. */
  rates: number[];
  base: number;
}

function hash(s: string): number {
  let h = 2166136261;
  for (let i = 0; i < s.length; i++) h = Math.imul(h ^ s.charCodeAt(i), 16777619);
  return h >>> 0;
}

function jitter(seed: string, i: number): number {
  return 0.8 + (hash(`${seed}#${i}`) % 1000) / 2500; // 0.8 .. 1.2
}

function nowIso(): string {
  return new Date().toISOString().replace(/\.\d+Z$/, 'Z');
}

function pastedName(d = new Date()): string {
  const p = (n: number) => String(n).padStart(2, '0');
  return `Pasted list · ${d.getDate()} ${MONTHS[d.getMonth()]} ${p(d.getHours())}:${p(d.getMinutes())}`;
}

function trimLink(s: string): string {
  for (;;) {
    const last = s.at(-1);
    const count = (c: string) => s.split(c).length - 1;
    if (last && '.,;:!?'.includes(last)) s = s.slice(0, -1);
    else if (last === ')' && count('(') < count(')')) s = s.slice(0, -1);
    else if (last === ']' && count('[') < count(']')) s = s.slice(0, -1);
    else return s;
  }
}

export function mockExtract(text: string): string[] {
  const out: string[] = [];
  for (const m of text.match(/https?:\/\/[^\s"'<>`]+/gi) ?? []) {
    try {
      const u = new URL(trimLink(m));
      if (u.hostname && !out.includes(u.href)) out.push(u.href);
    } catch {
      /* not a URL */
    }
  }
  return out;
}

function nameFromUrl(url: string): string {
  try {
    const seg = new URL(url).pathname.split('/').filter(Boolean).at(-1);
    return seg ? decodeURIComponent(seg) : 'download';
  } catch {
    return 'download';
  }
}

function readLimit(): SpeedLimit {
  try {
    const v = globalThis.sessionStorage?.getItem(LIMIT_KEY);
    if (v) return JSON.parse(v) as SpeedLimit;
  } catch {
    /* storage unavailable */
  }
  return { enabled: false, bps: 10_000_000 };
}

function splitEven(size: number, n: number): Seg[] {
  const step = Math.floor(size / n);
  return Array.from({ length: n }, (_, i) => {
    const start = i * step;
    const end = i === n - 1 ? size : start + step;
    return [start, start, end] as Seg;
  });
}

export function createMockBackend(opts: { tickMs: number; demo: boolean }): Backend & { calls: string[] } {
  const calls: string[] = [];
  const packages: PackageView[] = [];
  let downloads: DownloadView[] = [];
  const sims = new Map<number, Sim>();
  const listeners = new Set<(u: Update) => void>();
  let pending: Update = { progress: [], states: [], steals: [], resync: false };
  let nextId = 1;
  let nextPkg = 1;
  let limit = readLimit();
  // Like the engine: a running row is removed only after its transfer stops (one tick later).
  const removing = new Set<number>();
  const tickS = opts.tickMs / 1000;

  const sort = () => {
    downloads.sort((a, b) => PRIO[a.priority] - PRIO[b.priority] || a.position - b.position || a.id - b.id);
  };
  const setState = (d: DownloadView, state: State, error: string | null = null) => {
    d.state = state;
    d.error = error;
    pending.states.push({ id: d.id, state, error });
  };

  function add(url: string, packageId: number | null, extra: Partial<DownloadView> = {}, sim?: Sim): DownloadView {
    const h = hash(url);
    const size = extra.size !== undefined ? extra.size : 50_000_000 + (h % 450_000_000);
    const name = extra.name ?? nameFromUrl(url);
    const d: DownloadView = {
      id: nextId++, packageId, name, url, host: (() => { try { return new URL(url).host; } catch { return ''; } })(),
      size, written: 0, state: 'queued', error: null, priority: 'normal',
      position: downloads.reduce((m, x) => Math.max(m, x.position), 0) + 1, connections: 4, resumable: true,
      targetDir: 'C:\\Users\\you\\Downloads', targetPath: null, createdAt: nowIso(), segments: [], ...extra,
    };
    downloads.push(d);
    if (sim) sims.set(d.id, sim);
    sort();
    return d;
  }

  function simFor(d: DownloadView): Sim {
    let s = sims.get(d.id);
    if (!s) {
      const size = d.size ?? 100_000_000;
      const base = size / d.connections / 6; // a range takes about 6 s
      s = { base, rates: Array.from({ length: d.connections }, (_, i) => base * jitter(d.url, i)) };
      sims.set(d.id, s);
    }
    if (d.segments.length === 0 && d.size) d.segments = splitEven(d.size, d.connections);
    while (s.rates.length < d.segments.length) s.rates.push(s.base * jitter(d.url, s.rates.length));
    return s;
  }

  function tick() {
    if (removing.size) {
      for (const id of removing) {
        const d = downloads.find((x) => x.id === id);
        if (d) setState(d, 'paused');
        sims.delete(id);
      }
      downloads = downloads.filter((d) => !removing.has(d.id));
      removing.clear();
      pending.resync = true; // the bridge turns the engine's Removed event into a resync
    }
    // Start queued rows up to the parallel limit.
    let running = downloads.filter((d) => d.state === 'running').length;
    for (const d of downloads) {
      if (running >= MAX_PARALLEL) break;
      if (d.state !== 'queued') continue;
      if (d.url.includes('404')) setState(d, 'offline');
      else if (d.url.includes('expired')) setState(d, 'link_expired');
      else {
        setState(d, 'running');
        running++;
      }
    }
    const active = downloads.filter((d) => d.state === 'running' && !removing.has(d.id));
    let total = 0;
    for (const d of active) {
      const s = simFor(d);
      d.segments.forEach((g, i) => { if (g[1] < g[2]) total += s.rates[i]; });
    }
    const factor = limit.enabled && total > limit.bps ? limit.bps / total : 1;
    for (const d of active) {
      const s = simFor(d);
      let moved = 0;
      d.segments.forEach((g, i) => {
        const adv = Math.min(Math.round(s.rates[i] * factor * tickS), g[2] - g[1]);
        g[1] += adv;
        moved += adv;
      });
      // Split and steal: a free connection takes half of the largest remaining range.
      const open = d.segments.filter((g) => g[1] < g[2]).length;
      if (open < d.connections && d.size) {
        let big = -1;
        let left = 0;
        d.segments.forEach((g, i) => { if (g[2] - g[1] > left) { left = g[2] - g[1]; big = i; } });
        if (big >= 0 && left > Math.max(2_000_000, d.size / 200)) {
          const g = d.segments[big];
          const mid = g[1] + Math.floor(left / 2);
          const fresh: Seg = [mid, mid, g[2]];
          g[2] = mid;
          d.segments.splice(big + 1, 0, fresh);
          s.rates.splice(big + 1, 0, s.base * jitter(d.url, s.rates.length + 7));
          pending.steals.push({ id: d.id, bytes: fresh[2] - fresh[0] });
        }
      }
      d.written = d.segments.reduce((a, g) => a + (g[1] - g[0]), 0);
      pending.progress.push({
        id: d.id, written: d.written, size: d.size, speedBps: Math.round(moved / tickS),
        segments: d.segments.map((g) => [...g] as Seg),
      });
      if (d.segments.every((g) => g[1] >= g[2])) {
        d.targetPath = `${d.targetDir}\\${d.name}`;
        setState(d, 'completed');
      }
    }
    const u = pending;
    if (u.progress.length || u.states.length || u.steals.length || u.resync) {
      pending = { progress: [], states: [], steals: [], resync: false };
      listeners.forEach((cb) => cb(u));
    }
  }

  function seedDemo() {
    const at = (size: number, n: number, p: number, seed: string): Seg[] =>
      splitEven(size, n).map((g, i) => {
        const frac = i === 2 ? 0.985 : Math.min(1, Math.max(0, p + (jitter(seed, i) - 1)));
        return [g[0], g[0] + Math.floor((g[2] - g[0]) * frac), g[2]] as Seg;
      });
    const sim = (bps: number, n: number, seed: string): Sim => ({
      base: bps / n, rates: Array.from({ length: n }, (_, i) => (bps / n) * jitter(seed, i)),
    });
    const iso = 'https://releases.ubuntu.com/24.04.2/ubuntu-24.04.2-desktop-amd64.iso';
    const isoSegs = at(6_100_000_000, 8, 0.38, iso);
    add(iso, null, { size: 6_100_000_000, connections: 8, state: 'running', segments: isoSegs }, sim(41_200_000, 8, iso));
    const pkg: PackageView = { id: nextPkg++, name: 'sprite-fright-4k', createdAt: nowIso() };
    packages.push(pkg);
    const part = (n: number) => `https://download.blender.org/demo/movies/sprite-fright-4k.7z.00${n}`;
    for (const n of [1, 2]) {
      const segs = splitEven(2_000_000_000, 8).map((g) => [g[0], g[2], g[2]] as Seg);
      add(part(n), pkg.id, { size: 2_000_000_000, connections: 8, state: 'completed', segments: segs, targetPath: `C:\\Users\\you\\Downloads\\sprite-fright-4k.7z.00${n}` });
    }
    add(part(3), pkg.id, { size: 2_000_000_000, connections: 8, state: 'running', segments: at(2_000_000_000, 8, 0.62, part(3)) }, sim(18_200_000, 8, part(3)));
    add(part(4), pkg.id, { size: 2_000_000_000, connections: 8 }, sim(15_000_000, 8, part(4)));
    add(part(5), pkg.id, { size: 900_000_000, connections: 4 }, sim(12_000_000, 4, part(5)));
    const msi = 'https://download.blender.org/release/Blender4.5/blender-4.5.1-windows-x64.msi';
    add(msi, null, { size: 360_000_000, connections: 6, state: 'running', segments: at(360_000_000, 6, 0.81, msi) }, sim(12_000_000, 6, msi));
    const zip = 'https://files.example.org/share/quarterly-report-scans.zip?sig=expired';
    add(zip, null, { name: 'quarterly-report-scans.zip', size: 1_240_000_000, connections: 8, state: 'link_expired', segments: at(1_240_000_000, 8, 0.44, zip) });
    const mp4 = 'https://lectures.example.edu/cs101/lecture-07-recording.mp4';
    add(mp4, null, { size: 920_000_000, connections: 8, state: 'paused', resumable: true, segments: at(920_000_000, 8, 0.27, mp4) });
    for (const d of downloads) d.written = d.segments.reduce((a, g) => a + (g[1] - g[0]), 0);
  }

  if (opts.demo) seedDemo();
  setInterval(tick, opts.tickMs);

  const rec = (name: string) => calls.push(name);
  const find = (id: number) => downloads.find((d) => d.id === id);

  return {
    calls,
    async snapshot(): Promise<Snapshot> {
      rec('snapshot');
      const used = new Set(downloads.map((d) => d.packageId));
      return structuredClone({ packages: packages.filter((p) => used.has(p.id)), downloads, speedLimit: limit });
    },
    async addLinks(text): Promise<AddResult> {
      rec('addLinks');
      const links = mockExtract(text);
      if (links.length === 0) return { added: 0, packageId: null };
      let packageId: number | null = null;
      if (links.length >= 2) {
        const p = { id: nextPkg++, name: pastedName(), createdAt: nowIso() };
        packages.push(p);
        packageId = p.id;
      }
      for (const url of links) {
        const d = add(url, packageId);
        if (url.includes('404')) d.size = null;
        pending.states.push({ id: d.id, state: 'queued', error: null });
        if (url.includes('404')) setState(d, 'offline');
        else if (url.includes('expired')) setState(d, 'link_expired');
      }
      return { added: links.length, packageId };
    },
    async pause(ids) {
      rec('pause');
      for (const id of ids) {
        const d = find(id);
        if (d && (d.state === 'running' || d.state === 'queued')) setState(d, 'paused');
      }
    },
    async resume(ids) {
      rec('resume');
      for (const id of ids) {
        const d = find(id);
        if (d && d.state !== 'running' && d.state !== 'queued' && d.state !== 'completed') setState(d, 'queued');
      }
    },
    async remove(ids) {
      rec('remove');
      for (const id of ids) if (find(id)?.state === 'running') removing.add(id);
      downloads = downloads.filter((d) => !ids.includes(d.id) || removing.has(d.id));
      ids.filter((id) => !removing.has(id)).forEach((id) => sims.delete(id));
    },
    async pauseAll() {
      rec('pauseAll');
      for (const d of downloads) if (d.state === 'running' || d.state === 'queued') setState(d, 'paused');
    },
    async setPriority(id, p) {
      rec('setPriority');
      const d = find(id);
      if (d) d.priority = p;
      sort();
    },
    async reorder(ids) {
      rec('reorder');
      const rest = downloads.filter((d) => !ids.includes(d.id));
      let pos = 0;
      for (const id of ids) {
        const d = find(id);
        if (d) d.position = ++pos;
      }
      for (const d of rest) d.position = ++pos;
      sort();
    },
    async setSpeedLimit(l) {
      rec('setSpeedLimit');
      limit = { ...l };
      try {
        globalThis.sessionStorage?.setItem(LIMIT_KEY, JSON.stringify(limit));
      } catch {
        /* storage unavailable */
      }
    },
    async openFolder() {
      rec('openFolder');
    },
    onUpdate(cb) {
      listeners.add(cb);
      return () => listeners.delete(cb);
    },
  };
}
