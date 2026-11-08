/** Settings' number fields: what the user typed, or null when Haul should refuse it. */

/** A whole number from `min` to `max` (spaces around it are fine), else null. */
export function parseWhole(text: string, min: number, max: number): number | null {
  if (!/^\s*\d+\s*$/.test(text)) return null;
  const n = Number(text);
  return n >= min && n <= max ? n : null;
}

/** A speed in MB/s of 0.01 or more, else null. */
export function parseSpeed(text: string): number | null {
  if (!/^\s*\d*\.?\d+\s*$/.test(text)) return null;
  const n = Number(text);
  return Number.isFinite(n) && n >= 0.01 ? n : null;
}
