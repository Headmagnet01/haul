import { expect, test } from 'vitest';
import { blankRule, bytesToMb, mbToBytes, moveItem, parseExts, ruleSummary } from './rules';

test('parse_exts', () => {
  expect(parseExts('mp4, .MKV  avi,,mp4')).toEqual(['mp4', 'mkv', 'avi']);
  expect(parseExts('   ')).toEqual([]);
});

test('mb_round_trip', () => {
  expect(mbToBytes('')).toBeNull();
  expect(mbToBytes('  ')).toBeNull();
  expect(mbToBytes('abc')).toBeNull();
  expect(mbToBytes('1.5')).toBe(1_500_000);
  expect(mbToBytes('700')).toBe(700_000_000);
  expect(bytesToMb(null)).toBe('');
  expect(bytesToMb(1_500_000)).toBe('1.5');
  expect(bytesToMb(mbToBytes('700'))).toBe('700');
});

test('move_item_edges', () => {
  expect(moveItem(['a', 'b', 'c'], 0, -1)).toEqual(['a', 'b', 'c']);
  expect(moveItem(['a', 'b', 'c'], 2, 1)).toEqual(['a', 'b', 'c']);
  expect(moveItem(['a', 'b', 'c'], 1, -1)).toEqual(['b', 'a', 'c']);
  expect(moveItem(['a', 'b', 'c'], 1, 1)).toEqual(['a', 'c', 'b']);
  const xs = ['a', 'b'];
  moveItem(xs, 0, 1);
  expect(xs).toEqual(['a', 'b']);
});

test('rule_summary', () => {
  const r = blankRule();
  expect(r.enabled).toBe(true);
  expect(ruleSummary(r)).toBe('Every file → stays');
  r.match.name = '*.iso';
  r.match.host = 'releases.ubuntu.com';
  r.action.folder = 'Programs\\ISO';
  expect(ruleSummary(r)).toBe('*.iso · releases.ubuntu.com → Programs\\ISO');
  r.match.exts = ['iso', 'img'];
  r.match.minSize = 10_000_000;
  r.action.rename = '{name}-{date}.{ext}';
  expect(ruleSummary(r)).toBe('*.iso · releases.ubuntu.com · .iso .img · ≥ 10 MB → Programs\\ISO as {name}-{date}.{ext}');
  r.match.maxSize = 20_000_000;
  expect(ruleSummary(r)).toContain('10–20 MB');
});
