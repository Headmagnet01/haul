// Pure Collector logic: filters, list building, summaries and labels.

import type { CollectedLink, CollectorFilters, CollectorPackage, CollectorSnapshot, DupKind } from './types';

const HTML_EXTS = new Set(['html', 'htm', 'php', 'asp', 'aspx', 'jsp', 'shtml']);
const IMAGE_EXTS = new Set(['jpg', 'jpeg', 'png', 'gif', 'webp', 'bmp', 'svg', 'ico', 'avif', 'tif', 'tiff', 'heic']);
export const SMALL_BYTES = 1_000_000;

/** Lowercase extension after the last dot, or '' when there is none. */
function ext(name: string): string {
  const i = name.lastIndexOf('.');
  return i > 0 ? name.slice(i + 1).toLowerCase() : '';
}

export function isHidden(l: CollectedLink, f: CollectorFilters): boolean {
  const e = ext(l.name);
  const type = (l.contentType ?? '').toLowerCase();
  if (f.html && (type.startsWith('text/html') || HTML_EXTS.has(e))) return true;
  if (f.images && (type.startsWith('image/') || IMAGE_EXTS.has(e))) return true;
  if (f.small && l.size !== null && l.size < SMALL_BYTES) return true;
  if (f.duplicates && l.duplicate !== null) return true;
  return e !== '' && f.exts.includes(e);
}

export type CItem =
  | { kind: 'package'; pkg: CollectorPackage; links: CollectedLink[]; visible: CollectedLink[] }
  | { kind: 'link'; link: CollectedLink; child: boolean };

/** Rows to draw: removed and filtered links are left out, and so is a package with nothing left to show. */
export function buildCollectorList(s: CollectorSnapshot, collapsed: Set<string>, removed: Set<number>): CItem[] {
  const pkgs = new Map(s.packages.map((p) => [p.key, p]));
  const live = s.links.filter((l) => !removed.has(l.id));
  const out: CItem[] = [];
  const done = new Set<string>();
  for (const l of live) {
    const key = l.packageKey;
    const pkg = key === null ? undefined : pkgs.get(key);
    if (key === null || !pkg) {
      if (!isHidden(l, s.filters)) out.push({ kind: 'link', link: l, child: false });
      continue;
    }
    if (done.has(key)) continue;
    done.add(key);
    const links = live.filter((x) => x.packageKey === key);
    const visible = links.filter((x) => !isHidden(x, s.filters));
    if (visible.length === 0) continue;
    out.push({ kind: 'package', pkg, links, visible });
    if (!collapsed.has(key)) visible.forEach((v) => out.push({ kind: 'link', link: v, child: true }));
  }
  return out;
}

export function linkStatusWord(l: CollectedLink): string {
  switch (l.status) {
    case 'checking':
      return 'Checking…';
    case 'online':
      return 'Online';
    case 'offline':
      return `Offline (${l.message ?? '404'})`;
    case 'expired':
      return `Link expired (${l.message ?? '403'})`;
    default:
      return "Couldn't check";
  }
}

export function packageStatus(links: CollectedLink[]): { word: string; tone: 'ok' | 'err' | 'checking' } {
  const count = (pred: (l: CollectedLink) => boolean) => links.filter(pred).length;
  if (count((l) => l.status === 'checking') > 0) return { word: 'Checking…', tone: 'checking' };
  const bad = count((l) => l.status === 'offline' || l.status === 'expired');
  if (bad > 0) return { word: `${bad} offline`, tone: 'err' };
  const unknown = count((l) => l.status === 'error');
  if (unknown > 0) return { word: `${unknown} couldn't check`, tone: 'err' };
  return { word: `${links.length} / ${links.length} online`, tone: 'ok' };
}

const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? '' : 's'}`;

export function packageTag(pkg: CollectorPackage, links: CollectedLink[]): string {
  return pkg.archive ? `${pkg.archive} · ${plural(links.length, 'part')}` : plural(links.length, 'file');
}

export function subLine(s: CollectorSnapshot, removed: Set<number>): string {
  const live = s.links.filter((l) => !removed.has(l.id));
  if (live.length === 0) return 'Nothing waiting';
  const pkgs = new Set(live.map((l) => l.packageKey).filter((k) => k !== null));
  const hidden = live.filter((l) => isHidden(l, s.filters)).length;
  const base = `Links waiting for review · ${plural(live.length, 'link')} in ${plural(pkgs.size, 'package')}`;
  return hidden > 0 ? `${base} · ${hidden} hidden` : base;
}

/** What "Start package" or a lone row's "Start" starts: visible links that are not offline or expired. */
export function startableIds(links: CollectedLink[], f: CollectorFilters): number[] {
  return links
    .filter((l) => !isHidden(l, f) && l.status !== 'offline' && l.status !== 'expired')
    .map((l) => l.id);
}

/** What "Start all online" starts. */
export function onlineIds(s: CollectorSnapshot, removed: Set<number>): number[] {
  return s.links
    .filter((l) => !removed.has(l.id) && l.status === 'online' && !isHidden(l, s.filters))
    .map((l) => l.id);
}

export function dupLabel(k: DupKind): string {
  return { downloaded: 'Already downloaded', queued: 'Already queued', collector: 'Also in Collector' }[k];
}

export function sourceLabel(s: string): string {
  return ({ clipboard: 'Clipboard', paste: 'Paste', drop: 'Drop' } as Record<string, string>)[s] ?? s;
}
