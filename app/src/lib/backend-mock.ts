// A small simulated engine for the browser and the tests. Deterministic: sizes and speeds come from the URL.

import type { Backend } from './backend';
import type {
  DownloadView, PackageView, PairedClient, PriorityName, Seg, Settings, SettingsPatch, Snapshot, SpeedLimit, State, Update,
  UpdateStatus, WhenDoneStatus,
} from './types';
import { createMockCollector } from './backend-mock-collector';
import { createMockPostproc } from './backend-mock-postproc';

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

const SETTINGS_KEY = 'haul.mock.settings';

function save(key: string, v: unknown) {
  try {
    globalThis.sessionStorage?.setItem(key, JSON.stringify(v));
  } catch {
    /* storage unavailable */
  }
}

function readSettings(): Omit<Settings, 'watchClipboard' | 'speedLimit'> {
  const defaults = {
    downloadDir: 'C:\\Users\\you\\Downloads', maxParallel: MAX_PARALLEL, connections: 8, browserStart: 'now' as const,
    schedule: null, closeToTray: true, autostart: false, notifyFinished: true, trayHintShown: false, firstDownloadSeen: false,
    checkUpdates: true,
  };
  try {
    const v = globalThis.sessionStorage?.getItem(SETTINGS_KEY);
    if (v) return { ...defaults, ...(JSON.parse(v) as object) };
  } catch {
    /* storage unavailable */
  }
  return defaults;
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

export function createMockBackend(
  opts: { tickMs: number; demo: boolean; checkMs?: number; update?: string },
): Backend & {
  calls: string[]; pairRequest(browser: string): number; pairTimeout(): void; queueDrained(): void; showView(view: string): void;
  updateAvailable(version: string): void;
} {
  const calls: string[] = [];
  // Pairing: one waiting request at a time, like the app.
  let pairing: { n: number; browser: string } | null = null;
  let nextRequest = 1;
  let nextClient = 1;
  let clients: PairedClient[] = [];
  const pairListeners = new Set<(n: number, browser: string) => void>();
  const pairClosedListeners = new Set<(n: number) => void>();
  const viewListeners = new Set<(view: string) => void>();
  const updateListeners = new Set<(u: { version: string; url: string }) => void>();
  // The app is 0.1.0; `update` (`?update=0.2.0`) stands in for a newer release on GitHub.
  const tagUrl = (v: string) => `https://github.com/meraaki410/haul/releases/tag/v${v}`;
  let update: UpdateStatus = {
    current: '0.1.0', latest: opts.update ?? null, url: opts.update ? tagUrl(opts.update) : null, checkedAt: null, error: null,
  };
  const endPairing = () => {
    if (pairing === null) return;
    const n = pairing.n;
    pairing = null;
    for (const cb of pairClosedListeners) cb(n);
  };
  const packages: PackageView[] = [];
  let downloads: DownloadView[] = [];
  const sims = new Map<number, Sim>();
  const listeners = new Set<(u: Update) => void>();
  const empty = (): Update => ({ progress: [], states: [], steals: [], resync: false, collector: false, postproc: false });
  let pending: Update = empty();
  let nextId = 1;
  let nextPkg = 1;
  let limit = readLimit();
  // Like the engine: a running row is removed only after its transfer stops (one tick later).
  const removing = new Set<number>();
  // "When done": the countdown ticks once a second, like the app.
  let wd: WhenDoneStatus = { mode: 'nothing', countdown: null };
  let wdTimer: ReturnType<typeof setInterval> | undefined;
  const wdListeners = new Set<(s: WhenDoneStatus) => void>();
  const wdSet = (s: WhenDoneStatus) => {
    wd = s;
    if (s.countdown === null) clearInterval(wdTimer);
    wdListeners.forEach((cb) => cb({ ...wd }));
    return { ...wd };
  };
  // Finished files deleted outside Haul (the demo seeds one).
  const missing = new Set<number>();
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
      targetDir: 'C:\\Users\\you\\Downloads', targetPath: null, createdAt: nowIso(), finishedAt: null, segments: [], ...extra,
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
      if (running >= own.maxParallel) break;
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
        d.finishedAt = nowIso();
        setState(d, 'completed');
      }
    }
    postproc.tick();
    const u = pending;
    if (u.progress.length || u.states.length || u.steals.length || u.resync || u.collector || u.postproc) {
      pending = empty();
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
    // Finished: a package from this morning, a file from last week and one deleted outside Haul.
    const stamp = (t: Date) => t.toISOString().replace(/\.\d+Z$/, 'Z');
    const today = (h: number, m: number) => {
      const t = new Date();
      t.setHours(h, m, 0, 0);
      return stamp(t);
    };
    const daysAgo = (n: number) => stamp(new Date(Date.now() - n * 86_400_000));
    const done = (url: string, packageId: number | null, size: number, finishedAt: string) => {
      const name = nameFromUrl(url);
      return add(url, packageId, {
        name, size, connections: 8, state: 'completed', finishedAt, createdAt: finishedAt,
        segments: [[0, size, size]], targetPath: `C:\\Users\\you\\Downloads\\${name}`,
      });
    };
    const photos: PackageView = { id: nextPkg++, name: 'holiday-photos', createdAt: today(9, 10) };
    packages.push(photos);
    done('https://share.example.org/holiday-photos.part1.rar', photos.id, 700_000_000, today(9, 41));
    done('https://share.example.org/holiday-photos.part2.rar', photos.id, 412_000_000, today(9, 44));
    const pdf = done('https://docs.example.org/handbook-2026.pdf', null, 8_400_000, daysAgo(6));
    const gone = done('https://cdn.example.org/tools/old-setup.exe', null, 96_000_000, daysAgo(9));
    missing.add(gone.id);
    postproc.seedDone({ packageId: photos.id });
    postproc.seedDone({ downloadId: pdf.id });
    postproc.seedDone({ downloadId: gone.id });
    for (const d of downloads) d.written = d.segments.reduce((a, g) => a + (g[1] - g[0]), 0);
  }

  const sizeFor = (url: string) => 50_000_000 + (hash(url) % 450_000_000);
  const collector = createMockCollector({
    checkMs: opts.checkMs ?? 1200,
    demo: opts.demo,
    changed: () => (pending.collector = true),
    downloaded: (url) => {
      const d = downloads.find((x) => x.url === url);
      return d ? (d.state === 'completed' ? 'downloaded' : 'queued') : null;
    },
    nameFromUrl,
    sizeFor,
    pastedName: () => pastedName(),
    nowIso,
  });
  const queuePackages = new Map<string, number>();
  const postproc = createMockPostproc({
    changed: () => (pending.postproc = true),
    downloads: () => downloads,
    packages: () => packages,
  });
  // Like Tauri: a rejected command rejects with the message string.
  const call = async <T>(f: () => T): Promise<T> => f();

  // Settings other than the clipboard switch and speed limit, which live with their old mock state.
  type Own = Omit<Settings, 'watchClipboard' | 'speedLimit'>;
  let own: Own = readSettings();
  const fullSettings = (): Settings => ({ ...own, watchClipboard: collector.snapshot().watchClipboard, speedLimit: { ...limit } });
  const clamp = (n: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, Math.round(n)));

  if (opts.demo) {
    seedDemo();
    own = { ...own, firstDownloadSeen: true };
  }
  setInterval(tick, opts.tickMs);

  const rec = (name: string) => calls.push(name);
  const find = (id: number) => downloads.find((d) => d.id === id);

  return {
    calls,
    async settings() {
      rec('settings');
      return fullSettings();
    },
    async setSettings(p: SettingsPatch) {
      rec('setSettings');
      // A folder containing "Refused" stands in for one Haul can't write to.
      if (p.downloadDir !== undefined && p.downloadDir.includes('Refused')) {
        throw "downloadDir: Haul can't save files there (Access is denied.)";
      }
      if (p.schedule && p.schedule.weekdays.length === 0) throw 'schedule: choose at least one day';
      const next: SettingsPatch = { ...p };
      if (next.maxParallel !== undefined) next.maxParallel = clamp(next.maxParallel, 1, 20);
      if (next.connections !== undefined) next.connections = clamp(next.connections, 1, 16);
      if (next.speedLimit) {
        limit = { ...next.speedLimit, bps: next.speedLimit.enabled ? Math.max(1024, next.speedLimit.bps) : next.speedLimit.bps };
        save(LIMIT_KEY, limit);
      }
      if (next.watchClipboard !== undefined) collector.setWatch(next.watchClipboard);
      const { watchClipboard: _w, speedLimit: _l, ...rest } = next;
      own = { ...own, ...rest };
      save(SETTINGS_KEY, own);
      return fullSettings();
    },
    async pickFolder() {
      rec('pickFolder');
      const w = globalThis as unknown as { __haulMockNextFolder?: string };
      return w.__haulMockNextFolder ?? 'C:\\Users\\you\\Downloads\\Picked';
    },
    async openPath() {
      rec('openPath');
    },
    async snapshot(): Promise<Snapshot> {
      rec('snapshot');
      const used = new Set(downloads.map((d) => d.packageId));
      return structuredClone({ packages: packages.filter((p) => used.has(p.id)), downloads, speedLimit: limit });
    },
    async addLinks(text, source) {
      rec('addLinks');
      const links = mockExtract(text);
      if (links.length === 0) return { added: 0, skipped: 0 };
      if (!own.firstDownloadSeen) {
        own = { ...own, firstDownloadSeen: true };
        save(SETTINGS_KEY, own);
      }
      return collector.intake(links, source);
    },
    async collectorSnapshot() {
      rec('collectorSnapshot');
      return collector.snapshot();
    },
    async collectorStart(ids) {
      rec('collectorStart');
      let started = 0;
      for (const g of collector.take(ids)) {
        let packageId: number | null = null;
        if (g.key !== null) {
          packageId = queuePackages.get(g.key) ?? null;
          if (packageId === null) {
            const p = { id: nextPkg++, name: g.name ?? pastedName(), createdAt: nowIso() };
            packages.push(p);
            packageId = p.id;
            queuePackages.set(g.key, p.id);
          }
        }
        for (const l of g.links) {
          const d = add(l.url, packageId, { name: l.name, size: l.size ?? sizeFor(l.url), targetDir: own.downloadDir, connections: own.connections });
          pending.states.push({ id: d.id, state: 'queued', error: null });
          started++;
        }
      }
      return { started };
    },
    async collectorRemove(ids) {
      rec('collectorRemove');
      collector.remove(ids);
    },
    async collectorRecheck(ids) {
      rec('collectorRecheck');
      collector.recheck(ids);
    },
    async setCollectorFilters(f) {
      rec('setCollectorFilters');
      collector.setFilters(f);
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
    async openFolder() {
      rec('openFolder');
    },
    async openFile(id) {
      rec(`openFile:${id}`);
      if (missing.has(id)) throw 'That file is no longer on disk.';
    },
    async reveal(id) {
      rec(`reveal:${id}`);
    },
    async filesExist(ids) {
      rec('filesExist');
      return ids.filter((id) => !!find(id)?.targetPath && !missing.has(id));
    },
    async deleteFiles(ids) {
      rec('deleteFiles');
      for (const id of ids) {
        const d = find(id);
        if (!d) continue;
        // A name with "locked" stands in for a file another program has open.
        if (d.name.includes('locked') && !missing.has(id)) throw `${d.name} is in use`;
        downloads = downloads.filter((x) => x.id !== id);
        sims.delete(id);
      }
      pending.resync = true;
    },
    async downloadAgain(id) {
      rec('downloadAgain');
      const d = find(id);
      if (!d) throw 'No such download';
      const n = add(d.url, null, { name: d.name, size: d.size, targetDir: own.downloadDir, connections: own.connections });
      pending.states.push({ id: n.id, state: 'queued', error: null });
      return n.id;
    },
    async postprocRuns() {
      rec('postprocRuns');
      return postproc.runs();
    },
    postprocProcess(t) {
      rec('postprocProcess');
      return call(() => postproc.process(t));
    },
    postprocRetry(runId) {
      rec('postprocRetry');
      return call(() => postproc.retry(runId));
    },
    postprocPassword(runId, password, remember) {
      rec('postprocPassword');
      return call(() => postproc.password(runId, password, remember));
    },
    setExpectedHash(id, hash) {
      rec('setExpectedHash');
      return call(() => postproc.setHash(id, hash));
    },
    async rulesConfig() {
      rec('rulesConfig');
      return postproc.rulesConfig();
    },
    setPostprocSettings(s) {
      rec('setPostprocSettings');
      return call(() => postproc.setSettings(s));
    },
    setCategories(c) {
      rec('setCategories');
      return call(() => postproc.setCategories(c));
    },
    setRules(r) {
      rec('setRules');
      return call(() => postproc.setRules(r));
    },
    tryRules(f, d) {
      rec('tryRules');
      return call(() => postproc.tryRules(f, d));
    },
    addPassword(pw) {
      rec('addPassword');
      return call(() => postproc.addPassword(pw));
    },
    removePassword(id) {
      rec('removePassword');
      return call(() => postproc.removePassword(id));
    },
    orderPasswords(ids) {
      rec('orderPasswords');
      return call(() => postproc.orderPasswords(ids));
    },
    async whenDone() {
      rec('whenDone');
      return { ...wd };
    },
    async setWhenDone(mode) {
      rec('setWhenDone');
      return wdSet({ mode, countdown: null });
    },
    async whenDoneCancel() {
      rec('whenDoneCancel');
      return wdSet({ mode: 'nothing', countdown: null });
    },
    async whenDoneNow() {
      rec('whenDoneNow');
      wdSet({ mode: 'nothing', countdown: null });
    },
    onWhenDone(cb) {
      wdListeners.add(cb);
      return () => wdListeners.delete(cb);
    },
    /** e2e: the queue and post-processing went idle after work ran. */
    queueDrained() {
      if (wd.mode === 'nothing' || wd.countdown !== null) return;
      wdSet({ mode: wd.mode, countdown: 60 });
      wdTimer = setInterval(() => {
        const left = (wd.countdown ?? 1) - 1;
        if (left > 0) wdSet({ mode: wd.mode, countdown: left });
        else wdSet({ mode: 'nothing', countdown: null });
      }, 1000);
    },
    onUpdate(cb) {
      listeners.add(cb);
      return () => listeners.delete(cb);
    },
    pairRequest(browser) {
      if (pairing !== null) return pairing.n;
      pairing = { n: nextRequest++, browser };
      for (const cb of pairListeners) cb(pairing.n, browser);
      return pairing.n;
    },
    /** Like the app's 60 s timeout. */
    pairTimeout() {
      endPairing();
    },
    async pairAnswer(n, allow) {
      rec(`pairAnswer:${allow}`);
      if (pairing === null || pairing.n !== n) return;
      if (allow) clients.push({ id: nextClient++, browser: pairing.browser, createdAt: nowIso(), lastSeen: null });
      endPairing();
    },
    async pairedClients() {
      rec('pairedClients');
      return structuredClone(clients);
    },
    async removePairedClient(id) {
      rec('removePairedClient');
      clients = clients.filter((c) => c.id !== id);
    },
    onPairRequest(cb) {
      pairListeners.add(cb);
      return () => pairListeners.delete(cb);
    },
    onPairClosed(cb) {
      pairClosedListeners.add(cb);
      return () => pairClosedListeners.delete(cb);
    },
    async openExtensionFolder() {
      rec('openExtensionFolder');
    },
    onShowView(cb) {
      viewListeners.add(cb);
      return () => viewListeners.delete(cb);
    },
    /** Like a click on one of the app's notifications. */
    showView(view) {
      for (const cb of viewListeners) cb(view);
    },
    async updateStatus() {
      rec('updateStatus');
      return { ...update };
    },
    async checkUpdatesNow() {
      rec('checkUpdatesNow');
      await new Promise((r) => setTimeout(r, 50));
      update = { ...update, checkedAt: new Date().toISOString() };
      return { ...update };
    },
    async openRelease(url) {
      rec(`openRelease ${url}`);
    },
    onUpdateAvailable(cb) {
      updateListeners.add(cb);
      return () => updateListeners.delete(cb);
    },
    /** Like the daily check finding a newer release. */
    updateAvailable(version) {
      update = { ...update, latest: version, url: tagUrl(version) };
      for (const cb of updateListeners) cb({ version, url: tagUrl(version) });
    },
  };
}
