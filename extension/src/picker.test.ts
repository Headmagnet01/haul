import { expect, test } from 'vitest';
import { KINDS, kindOf, uniqueLinks } from './picker';

test('kind_of_each_category', () => {
  expect(kindOf('https://a.org/a.zip?x=1')).toBe('archives');
  expect(kindOf('https://a.org/set.part2.rar')).toBe('archives');
  expect(kindOf('https://a.org/s.7z.003')).toBe('archives');
  expect(kindOf('https://a.org/film.MKV')).toBe('video');
  expect(kindOf('https://a.org/song.flac#t=1')).toBe('audio');
  expect(kindOf('https://a.org/pic.webp')).toBe('images');
  expect(kindOf('https://a.org/paper.pdf')).toBe('documents');
  expect(kindOf('https://a.org/setup.msi')).toBe('programs');
  expect(kindOf('https://a.org/page')).toBe('other');
  expect(kindOf('https://a.org/')).toBe('other');
  expect(KINDS.map((k) => k.id)).toEqual(['archives', 'video', 'audio', 'images', 'documents', 'programs', 'other']);
});

test('unique_links_filters_and_dedupes', () => {
  expect(
    uniqueLinks([
      'https://a.org/1.zip',
      'https://a.org/1.zip#top',
      'javascript:void(0)',
      'mailto:x@a.org',
      'http://b.org/2.zip',
      'https://a.org/1.zip',
      '',
      'not a url',
    ]),
  ).toEqual(['https://a.org/1.zip', 'http://b.org/2.zip']);
});
