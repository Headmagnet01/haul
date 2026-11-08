import type { SavedStore } from './api';
import { pairStateNow } from './pairstate';
import { DEFAULT_OPTIONS, type AltClick, type Options } from './types';

/** What the popup shows about the last download it handled. */
export interface LastResult {
  at: number;
  text: string;
}

export type PairState = 'idle' | 'waiting' | 'paired' | 'denied' | 'failed';

/** Port and key live in `chrome.storage.local`: the worker can be stopped at any time. */
export const saved: SavedStore = {
  async get() {
    const v = await chrome.storage.local.get(['port', 'token']);
    return { port: typeof v.port === 'number' ? v.port : null, token: typeof v.token === 'string' ? v.token : null };
  },
  set: (p) => chrome.storage.local.set(p),
};

export async function loadOptions(): Promise<Options> {
  const v = await chrome.storage.local.get('options');
  const o = v.options as Partial<Options> | undefined;
  return {
    minSize: typeof o?.minSize === 'number' && o.minSize >= 0 ? o.minSize : DEFAULT_OPTIONS.minSize,
    types: Array.isArray(o?.types) ? o.types.filter((t): t is string => typeof t === 'string') : DEFAULT_OPTIONS.types,
  };
}

export const saveOptions = (o: Options) => chrome.storage.local.set({ options: o });

export async function lastResult(): Promise<LastResult | null> {
  return ((await chrome.storage.local.get('lastResult')).lastResult as LastResult | undefined) ?? null;
}

export const setLastResult = (text: string) => chrome.storage.local.set({ lastResult: { at: Date.now(), text } });

export async function pairState(): Promise<PairState> {
  const v = await chrome.storage.local.get(['pairState', 'pairSince']);
  return pairStateNow((v.pairState as PairState | undefined) ?? 'idle', v.pairSince as number | undefined, Date.now());
}

export const setPairState = (s: PairState) => chrome.storage.local.set({ pairState: s, pairSince: Date.now() });

/** Alt+clicks live in session storage, so a worker restarted between the click and the download still sees them. */
export async function altClicks(): Promise<AltClick[]> {
  return ((await chrome.storage.session.get('altClicks')).altClicks as AltClick[] | undefined) ?? [];
}

export async function addAltClick(url: string): Promise<void> {
  const now = Date.now();
  const recent = (await altClicks()).filter((a) => now - a.at < 10_000);
  recent.push({ url, at: now });
  await chrome.storage.session.set({ altClicks: recent });
}

/** Words shared by the worker, popup and notifications. */
export const WORDS = {
  sent: 'Sent to Haul',
  kept: "Haul didn't answer, so your browser kept it",
  unpaired: 'Not paired: press Pair',
} as const;
