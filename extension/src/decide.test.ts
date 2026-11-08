import { expect, test } from 'vitest';
import { decide, extOf, factsFromItem, needsSize } from './decide';
import { DEFAULT_OPTIONS, type DownloadFacts } from './types';

const MIB = 1048576;
const NOW = 1_000_000;

function f(over: Partial<DownloadFacts> = {}): DownloadFacts {
  return {
    url: 'https://a.org/file.bin',
    finalUrl: 'https://cdn.a.org/file.bin',
    filename: 'C:\\Users\\me\\Downloads\\file.bin',
    size: 2 * MIB,
    incognito: false,
    ...over,
  };
}

const d = (facts: DownloadFacts, alt: { url: string; at: number }[] = []) => decide(facts, DEFAULT_OPTIONS, alt, NOW);

test('catches_big_known_size', () => expect(d(f())).toBe('catch'));

test('skips_small_unlisted', () => expect(d(f({ size: 10 * 1024 }))).toBe('skip'));

test('catches_small_listed_type', () => {
  expect(d(f({ size: 10 * 1024, filename: 'C:\\dl\\tiny.zip', finalUrl: 'https://a.org/tiny.zip' }))).toBe('catch');
});

test('unknown_size_listed_catches', () => {
  expect(d(f({ size: null, filename: 'C:\\dl\\doc.PDF' }))).toBe('catch');
});

test('unknown_size_unlisted_skips', () => expect(d(f({ size: null }))).toBe('skip'));

test('skips_incognito', () => expect(d(f({ incognito: true }))).toBe('skip'));

test('skips_blob_and_data', () => {
  expect(d(f({ url: 'blob:https://a.org/123', finalUrl: 'blob:https://a.org/123' }))).toBe('skip');
  expect(d(f({ url: 'data:application/zip;base64,AAAA', finalUrl: 'data:application/zip;base64,AAAA' }))).toBe('skip');
});

test('skips_non_http', () => {
  expect(d(f({ url: 'file:///C:/x.bin', finalUrl: 'file:///C:/x.bin' }))).toBe('skip');
  expect(d(f({ url: 'ftp://a.org/x.bin', finalUrl: 'ftp://a.org/x.bin' }))).toBe('skip');
  // No final URL: the download wasn't a plain GET Haul can fetch again (spec 4.1).
  expect(d(f({ finalUrl: '' }))).toBe('skip');
  expect(d(f({ url: '', finalUrl: '' }))).toBe('skip');
});

test('skips_alt_clicked_within_2s', () => {
  expect(d(f(), [{ url: 'https://a.org/file.bin', at: NOW - 1999 }])).toBe('skip');
  expect(d(f(), [{ url: 'https://cdn.a.org/file.bin', at: NOW - 100 }])).toBe('skip');
  expect(d(f(), [{ url: 'https://a.org/file.bin', at: NOW - 2001 }])).toBe('catch');
  expect(d(f(), [{ url: 'https://a.org/other.bin', at: NOW - 100 }])).toBe('catch');
});

test('ext_from_finalurl_when_filename_empty', () => {
  expect(d(f({ size: 10, filename: '', finalUrl: 'https://a.org/get/setup.msi?token=1' }))).toBe('catch');
  expect(extOf('C:\\dl\\Archive.Part1.RAR')).toBe('rar');
  expect(extOf('https://a.org/a.tar.gz?x=1#y')).toBe('gz');
  expect(extOf('noext')).toBe('');
});

test('size_threshold_inclusive', () => {
  expect(d(f({ size: MIB }))).toBe('catch');
  expect(d(f({ size: MIB - 1 }))).toBe('skip');
});

test('firefox_item_without_final_url_uses_url', () => {
  const facts = factsFromItem({ url: 'https://a.org/x.zip', filename: '', fileSize: -1, totalBytes: 5_000_000, incognito: false });
  expect(facts.finalUrl).toBe('https://a.org/x.zip');
  expect(facts.size).toBe(5_000_000);
  expect(d(facts)).toBe('catch');
});

test('chrome_item_keeps_final_url', () => {
  const facts = factsFromItem({
    url: 'https://a.org/dl',
    finalUrl: 'https://cdn.a.org/x.zip',
    filename: 'C:\\dl\\x.zip',
    fileSize: 2 * MIB,
    totalBytes: 0,
    incognito: true,
  });
  expect(facts).toEqual({ url: 'https://a.org/dl', finalUrl: 'https://cdn.a.org/x.zip', filename: 'C:\\dl\\x.zip', size: 2 * MIB, incognito: true });
});

// Firefox fires onCreated before it knows the size (totalBytes -1), so the size is waited for
// exactly when it is the only thing deciding.
test('needs_size_when_only_size_decides', () => {
  const ns = (facts: DownloadFacts, alt: { url: string; at: number }[] = []) => needsSize(facts, DEFAULT_OPTIONS, alt, NOW);
  expect(ns(f({ size: null }))).toBe(true);
  expect(ns(f({ size: 2 * MIB }))).toBe(false);
  expect(ns(f({ size: null, filename: 'C:\\dl\\a.zip' }))).toBe(false); // listed type: caught anyway
  expect(ns(f({ size: null, incognito: true }))).toBe(false);
  expect(ns(f({ size: null, finalUrl: 'blob:https://a.org/x' }))).toBe(false);
  expect(ns(f({ size: null }), [{ url: 'https://a.org/file.bin', at: NOW - 100 }])).toBe(false); // Alt-click
});

test('item_without_any_size_is_null', () => {
  expect(factsFromItem({ url: 'https://a.org/x', filename: '', fileSize: 0, totalBytes: 0, incognito: false }).size).toBeNull();
});
