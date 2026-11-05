import { describe, expect, test } from 'vitest';
import { formatBytes, formatEta, formatProgress, formatSpeed } from './format';

describe('format', () => {
  test('bytes', () => {
    expect(formatBytes(999)).toBe('999 B');
    expect(formatBytes(15_400)).toBe('15 KB');
    expect(formatBytes(4_560_000)).toBe('4.6 MB');
    expect(formatBytes(412_000_000)).toBe('412 MB');
    expect(formatBytes(6_100_000_000)).toBe('6.10 GB');
    expect(formatBytes(null)).toBe('—');
  });

  test('speed', () => {
    expect(formatSpeed(0)).toBe('—');
    expect(formatSpeed(512_000)).toBe('512 KB/s');
    expect(formatSpeed(41_230_000)).toBe('41.2 MB/s');
  });

  test('eta', () => {
    expect(formatEta(null)).toBe('—');
    expect(formatEta(42)).toBe('42s');
    expect(formatEta(125)).toBe('2m 5s');
    expect(formatEta(7380)).toBe('2h 3m');
  });

  test('format_handles_unknown', () => {
    expect(formatProgress(3_000_000, null)).toBe('3.0 MB');
    expect(formatProgress(0, 0)).toBe('100.0%');
    expect(formatProgress(38, 100)).toBe('38.0%');
    expect(formatEta(Infinity)).toBe('—');
    expect(formatEta(NaN)).toBe('—');
    expect(formatSpeed(NaN)).toBe('—');
    expect(formatBytes(NaN)).toBe('—');
  });
});
