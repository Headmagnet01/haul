import { base, withTimeout, type Fetcher } from './net';

/** Haul takes the first free port in this range. */
export const PORTS: readonly number[] = Array.from({ length: 11 }, (_, i) => 17654 + i);

const STATUS_TIMEOUT_MS = 800;

async function isHaul(fetch: Fetcher, port: number): Promise<boolean> {
  try {
    const r = await withTimeout(STATUS_TIMEOUT_MS, (signal) => fetch(`${base(port)}/v1/status`, { signal }));
    if (r.status !== 200) return false;
    const v = (await r.json()) as { app?: unknown } | null;
    return typeof v === 'object' && v !== null && v.app === 'haul';
  } catch {
    return false;
  }
}

/** The port Haul answers on: `last` first, then each port in order. */
export async function findPort(fetch: Fetcher, last: number | null): Promise<number | null> {
  if (last !== null && (await isHaul(fetch, last))) return last;
  for (const p of PORTS) {
    if (p !== last && (await isHaul(fetch, p))) return p;
  }
  return null;
}
