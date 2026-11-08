import { expect, test } from 'vitest';
import { decide, extOf } from './decide';
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
  expect(d(f({ finalUrl: '' }))).toBe('catch'); // falls back to url
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
