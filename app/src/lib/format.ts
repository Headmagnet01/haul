// Decimal units (1 MB = 1,000,000 bytes), as in the mockup.

const DASH = '—';
const ok = (n: number | null): n is number => n !== null && Number.isFinite(n) && n >= 0;

export function formatBytes(n: number | null): string {
  if (!ok(n)) return DASH;
  if (n < 1000) return `${Math.round(n)} B`;
  if (n < 999_500) return `${Math.round(n / 1e3)} KB`;
  if (n < 99_950_000) return `${(n / 1e6).toFixed(1)} MB`;
  if (n < 999_500_000) return `${Math.round(n / 1e6)} MB`;
  return `${(n / 1e9).toFixed(2)} GB`;
}

export function formatSpeed(bps: number): string {
  if (!ok(bps) || bps === 0) return DASH;
  if (bps < 1000) return `${Math.round(bps)} B/s`;
  if (bps < 999_500) return `${Math.round(bps / 1e3)} KB/s`;
  if (bps < 999_950_000) return `${(bps / 1e6).toFixed(1)} MB/s`;
  return `${(bps / 1e9).toFixed(2)} GB/s`;
}

export function formatEta(seconds: number | null): string {
  if (!ok(seconds)) return DASH;
  const s = Math.max(1, Math.round(seconds));
  if (s < 60) return `${s}s`;
  if (s < 3600) return `${Math.floor(s / 60)}m ${s % 60}s`;
  return `${Math.floor(s / 3600)}h ${Math.floor((s % 3600) / 60)}m`;
}

/** "38.0%", or the bytes written when the size is unknown. `done`: the download completed. */
export function formatProgress(written: number, size: number | null, done = false): string {
  if (size === null) return formatBytes(written);
  if (done) return '100.0%';
  if (size <= 0) return '0.0%';
  return `${Math.min(100, Math.max(0, (written / size) * 100)).toFixed(1)}%`;
}

/** Seconds left at the current speed, or null when it can't be known. */
export function etaSeconds(written: number, size: number | null, bps: number): number | null {
  if (size === null || !(bps > 0)) return null;
  return Math.max(0, size - written) / bps;
}
