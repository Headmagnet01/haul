import type { AltClick, DownloadFacts, Options } from './types';

/** An Alt+click this recent means "let the browser have it". */
export const ALT_WINDOW_MS = 2000;

/** The extension of a path or URL: lowercase, no dot, '' if none. */
export function extOf(name: string): string {
  const clean = name.split(/[?#]/)[0];
  const base = clean.split(/[\\/]/).pop() ?? '';
  const dot = base.lastIndexOf('.');
  return dot > 0 ? base.slice(dot + 1).toLowerCase() : '';
}

/** The parts of a browser `DownloadItem` the catch rule reads. Firefox has no `finalUrl`. */
export interface ItemLike {
  url: string;
  finalUrl?: string;
  filename: string;
  fileSize: number;
  totalBytes: number;
  incognito: boolean;
}

/** Firefox reports one `url`, already after redirects, so it stands in for the final URL. */
export function factsFromItem(item: ItemLike): DownloadFacts {
  const size = item.fileSize > 0 ? item.fileSize : item.totalBytes > 0 ? item.totalBytes : null;
  return { url: item.url, finalUrl: item.finalUrl || item.url, filename: item.filename, size, incognito: item.incognito };
}

function isHttp(u: string): boolean {
  return /^https?:\/\//i.test(u);
}

/** Whether Haul should take this download from the browser. */
export function decide(f: DownloadFacts, opts: Options, altClicks: AltClick[], now: number): 'catch' | 'skip' {
  // No final URL means no plain GET (a form post); blob:, data:, file: and ftp: links are also things
  // Haul can't fetch again by itself.
  const url = f.finalUrl;
  if (f.incognito || !isHttp(url)) return 'skip';
  if (altClicks.some((a) => now - a.at <= ALT_WINDOW_MS && (a.url === f.url || a.url === f.finalUrl))) return 'skip';
  const ext = extOf(f.filename) || extOf(url);
  if (ext && opts.types.includes(ext)) return 'catch';
  return f.size !== null && f.size >= opts.minSize ? 'catch' : 'skip';
}
