// Pure helpers for the Rules view's editors.

import type { Rule } from './types';

const MB = 1_000_000;

/** "mp4, .MKV avi" → ['mp4', 'mkv', 'avi']: lowercase, no dots, no repeats. */
export function parseExts(s: string): string[] {
  const out: string[] = [];
  for (const raw of s.split(/[\s,]+/)) {
    const e = raw.trim().replace(/^\.+/, '').toLowerCase();
    if (e && !out.includes(e)) out.push(e);
  }
  return out;
}

/** A size typed in MB (1 MB = 1,000,000 bytes); blank or not a number gives null. */
export function mbToBytes(s: string): number | null {
  const t = s.trim();
  if (!t) return null;
  const n = Number(t);
  return Number.isFinite(n) && n >= 0 ? Math.round(n * MB) : null;
}

export function bytesToMb(n: number | null): string {
  return n === null ? '' : String(Math.round((n / MB) * 1000) / 1000);
}

export function blankRule(): Rule {
  return {
    id: 0,
    name: '',
    enabled: true,
    match: { host: null, name: null, exts: [], minSize: null, maxSize: null },
    action: { folder: null, rename: null },
  };
}

/** A copy with item `i` moved one place up (-1) or down (1); unchanged at the ends. */
export function moveItem<T>(xs: T[], i: number, by: -1 | 1): T[] {
  const out = [...xs];
  const j = i + by;
  if (i < 0 || i >= out.length || j < 0 || j >= out.length) return out;
  [out[i], out[j]] = [out[j], out[i]];
  return out;
}

function sizeText(lo: number | null, hi: number | null): string | null {
  if (lo !== null && hi !== null) return `${bytesToMb(lo)}–${bytesToMb(hi)} MB`;
  if (lo !== null) return `≥ ${bytesToMb(lo)} MB`;
  if (hi !== null) return `≤ ${bytesToMb(hi)} MB`;
  return null;
}

/** One line for the rules list, e.g. `*.iso · releases.ubuntu.com → Programs\ISO`. */
export function ruleSummary(r: Rule): string {
  const m = r.match;
  const parts = [
    m.name?.trim() || null,
    m.host?.trim() || null,
    m.exts.length ? m.exts.map((e) => `.${e}`).join(' ') : null,
    sizeText(m.minSize, m.maxSize),
  ].filter((p): p is string => p !== null);
  const what = parts.length ? parts.join(' · ') : 'Every file';
  const folder = r.action.folder?.trim() || 'stays';
  const rename = r.action.rename?.trim();
  return `${what} → ${folder}${rename ? ` as ${rename}` : ''}`;
}
