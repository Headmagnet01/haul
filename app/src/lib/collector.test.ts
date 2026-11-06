import { expect, test } from 'vitest';
import {
  buildCollectorList, dupLabel, isHidden, linkStatusWord, onlineIds, packageStatus, packageTag, sourceLabel,
  startableIds, subLine,
} from './collector';
import type { CollectedLink, CollectorFilters, CollectorPackage, CollectorSnapshot } from './types';

const NONE: CollectorFilters = { html: false, images: false, small: false, duplicates: false, exts: [] };
let nextId = 1;
const link = (p: Partial<CollectedLink> = {}): CollectedLink => ({
  id: nextId++, batchId: 1, url: `https://a.org/${nextId}.bin`, name: `${nextId}.bin`, size: 5_000_000,
  contentType: null, status: 'online', message: null, source: 'paste', duplicate: null, packageKey: null, ...p,
});
const pkg = (p: Partial<CollectorPackage> = {}): CollectorPackage => ({
  key: '1:rest', name: 'Pasted list · 5 Nov 18:40', archive: null, source: 'paste', createdAt: '2026-11-05T18:40:00Z', ...p,
});
const snap = (packages: CollectorPackage[], links: CollectedLink[], filters = NONE): CollectorSnapshot =>
  ({ packages, links, filters, watchClipboard: false });

test('hides_by_chip', () => {
  const on = (k: keyof Omit<CollectorFilters, 'exts'>) => ({ ...NONE, [k]: true });
  expect(isHidden(link({ contentType: 'text/html; charset=utf-8', name: 'x' }), on('html'))).toBe(true);
  expect(isHidden(link({ name: 'index.php' }), on('html'))).toBe(true);
  expect(isHidden(link({ name: 'index.php' }), NONE)).toBe(false);
  expect(isHidden(link({ contentType: 'image/png', name: 'x' }), on('images'))).toBe(true);
  expect(isHidden(link({ name: 'PHOTO.JPG' }), on('images'))).toBe(true);
  expect(isHidden(link({ name: 'movie.mkv' }), on('images'))).toBe(false);
  expect(isHidden(link({ size: 999_999 }), on('small'))).toBe(true);
  expect(isHidden(link({ size: 1_000_000 }), on('small'))).toBe(false);
  expect(isHidden(link({ size: null }), on('small'))).toBe(false);
  expect(isHidden(link({ duplicate: 'queued' }), on('duplicates'))).toBe(true);
  expect(isHidden(link({ duplicate: null }), on('duplicates'))).toBe(false);
});

test('ext_filter_matches_lowercase', () => {
  expect(isHidden(link({ name: 'Linux.ISO' }), { ...NONE, exts: ['iso'] })).toBe(true);
  expect(isHidden(link({ name: 'iso' }), { ...NONE, exts: ['iso'] })).toBe(false);
});

test('build_list_packages_and_lone_rows', () => {
  const p = pkg();
  const a = link({ packageKey: p.key });
  const b = link({ packageKey: p.key });
  const lone = link();
  const s = snap([p], [a, b, lone]);
  const items = buildCollectorList(s, new Set(), new Set());
  expect(items.map((i) => (i.kind === 'link' ? `link:${i.child}` : 'package'))).toEqual(
    ['package', 'link:true', 'link:true', 'link:false']);
  expect(items[0].kind === 'package' && items[0].links.length).toBe(2);
  const collapsed = buildCollectorList(s, new Set([p.key]), new Set());
  expect(collapsed.map((i) => i.kind)).toEqual(['package', 'link']);
  const removed = buildCollectorList(s, new Set(), new Set([a.id, lone.id]));
  expect(removed.map((i) => i.kind)).toEqual(['package', 'link']);
});

test('package_hidden_when_all_links_hidden', () => {
  const p = pkg();
  const s = snap([p], [link({ packageKey: p.key, name: 'a.png' }), link({ packageKey: p.key, name: 'b.gif' })],
    { ...NONE, images: true });
  expect(buildCollectorList(s, new Set(), new Set())).toEqual([]);
});

test('package_status_words', () => {
  const on = () => link();
  expect(packageStatus([on(), on(), on(), on()])).toEqual({ word: '4 / 4 online', tone: 'ok' });
  expect(packageStatus([on(), on(), on(), link({ status: 'offline' })])).toEqual({ word: '1 offline', tone: 'err' });
  expect(packageStatus([on(), link({ status: 'checking' }), link({ status: 'offline' })]))
    .toEqual({ word: 'Checking…', tone: 'checking' });
  expect(packageStatus([on(), link({ status: 'error' }), link({ status: 'error' })]))
    .toEqual({ word: "2 couldn't check", tone: 'err' });
});

test('package_tag', () => {
  const n = (k: number) => Array.from({ length: k }, () => link());
  expect(packageTag(pkg({ archive: '7z' }), n(4))).toBe('7z · 4 parts');
  expect(packageTag(pkg({ archive: 'rar' }), n(1))).toBe('rar · 1 part');
  expect(packageTag(pkg(), n(8))).toBe('8 files');
  expect(packageTag(pkg(), n(1))).toBe('1 file');
});

test('sub_line', () => {
  const ps = [pkg({ key: 'a' }), pkg({ key: 'b' }), pkg({ key: 'c' })];
  const links = [
    ...Array.from({ length: 5 }, () => link({ packageKey: 'a' })),
    ...Array.from({ length: 5 }, () => link({ packageKey: 'b' })),
    ...Array.from({ length: 2 }, () => link({ packageKey: 'c' })),
    link({ packageKey: 'c', name: 'x.png' }),
    link({ packageKey: 'c', name: 'y.png' }),
  ];
  expect(subLine(snap(ps, links, { ...NONE, images: true }), new Set()))
    .toBe('Links waiting for review · 14 links in 3 packages · 2 hidden');
  expect(subLine(snap([], [link()]), new Set())).toBe('Links waiting for review · 1 link in 0 packages');
  expect(subLine(snap([], []), new Set())).toBe('Nothing waiting');
  const one = link();
  expect(subLine(snap([], [one]), new Set([one.id]))).toBe('Nothing waiting');
});

test('startable_skip_hidden_and_offline', () => {
  const ok = link();
  const err = link({ status: 'error' });
  const checking = link({ status: 'checking' });
  const off = link({ status: 'offline' });
  const exp = link({ status: 'expired' });
  const img = link({ name: 'a.png' });
  expect(startableIds([ok, err, checking, off, exp, img], { ...NONE, images: true }))
    .toEqual([ok.id, err.id, checking.id]);
});

test('online_ids_only_visible_online', () => {
  const ok = link();
  const gone = link();
  const err = link({ status: 'error' });
  const img = link({ name: 'a.png' });
  expect(onlineIds(snap([], [ok, gone, err, img], { ...NONE, images: true }), new Set([gone.id]))).toEqual([ok.id]);
});

test('link_status_words', () => {
  expect(linkStatusWord(link({ status: 'checking' }))).toBe('Checking…');
  expect(linkStatusWord(link({ status: 'online' }))).toBe('Online');
  expect(linkStatusWord(link({ status: 'offline', message: '404' }))).toBe('Offline (404)');
  expect(linkStatusWord(link({ status: 'expired', message: '410' }))).toBe('Link expired (410)');
  expect(linkStatusWord(link({ status: 'error', message: 'timed out' }))).toBe("Couldn't check");
});

test('labels', () => {
  expect(dupLabel('downloaded')).toBe('Already downloaded');
  expect(dupLabel('queued')).toBe('Already queued');
  expect(dupLabel('collector')).toBe('Also in Collector');
  expect(sourceLabel('clipboard')).toBe('Clipboard');
  expect(sourceLabel('paste')).toBe('Paste');
  expect(sourceLabel('drop')).toBe('Drop');
  expect(sourceLabel('Edge · page')).toBe('Edge · page');
});
