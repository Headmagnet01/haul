import { extOf } from './decide';

export type Kind = 'archives' | 'video' | 'audio' | 'images' | 'documents' | 'programs' | 'other';

/** The picker's filter chips, in order, with the extensions each covers (as Haul's categories). */
export const KINDS: { id: Kind; label: string; exts: string[] }[] = [
  { id: 'archives', label: 'Archives', exts: ['zip', 'rar', '7z', 'tar', 'gz', 'tgz', 'bz2', 'xz', 'zst', 'cab'] },
  { id: 'video', label: 'Video', exts: ['mp4', 'mkv', 'avi', 'mov', 'wmv', 'flv', 'webm', 'm4v', 'mpg', 'mpeg', 'ts', '3gp'] },
  { id: 'audio', label: 'Audio', exts: ['mp3', 'flac', 'wav', 'aac', 'ogg', 'opus', 'm4a', 'wma', 'aiff'] },
  { id: 'images', label: 'Images', exts: ['jpg', 'jpeg', 'png', 'gif', 'webp', 'bmp', 'svg', 'avif'] },
  {
    id: 'documents',
    label: 'Documents',
    exts: ['pdf', 'doc', 'docx', 'xls', 'xlsx', 'ppt', 'pptx', 'odt', 'ods', 'odp', 'txt', 'rtf', 'csv', 'epub', 'md'],
  },
  { id: 'programs', label: 'Programs', exts: ['exe', 'msi', 'msix', 'appx', 'bat', 'cmd', 'ps1', 'iso', 'img', 'apk'] },
  { id: 'other', label: 'Other', exts: [] },
];

/** Split-archive volumes: `.r00`, `.001`. */
const VOLUME = /^(r\d{2}|\d{3})$/;

export function kindOf(url: string): Kind {
  let path = url;
  try {
    path = new URL(url).pathname;
  } catch {
    // not a URL: use it as a path
  }
  const ext = extOf(path);
  if (VOLUME.test(ext)) return 'archives';
  return KINDS.find((k) => k.exts.includes(ext))?.id ?? 'other';
}

/** http(s) links only, without #fragments, each once, in first-seen order. */
export function uniqueLinks(hrefs: string[]): string[] {
  const seen = new Set<string>();
  const out: string[] = [];
  for (const h of hrefs) {
    let u: URL;
    try {
      u = new URL(h);
    } catch {
      continue;
    }
    if (u.protocol !== 'http:' && u.protocol !== 'https:') continue;
    u.hash = '';
    const s = u.toString();
    if (!seen.has(s)) {
      seen.add(s);
      out.push(s);
    }
  }
  return out;
}
