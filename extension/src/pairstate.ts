import type { PairState } from './store';

/** Haul gives up on a pairing request after 60 s; a little longer and "waiting" is stale. */
export const PAIR_WAIT_MS = 70_000;

/**
 * The pairing state as the popup should show it. A "waiting" saved by a worker that was stopped
 * mid-request (browser closed, extension reloaded) would otherwise keep Pair disabled for good.
 */
export function pairStateNow(state: PairState, since: number | undefined, now: number): PairState {
  if (state !== 'waiting') return state;
  return since !== undefined && now - since <= PAIR_WAIT_MS ? 'waiting' : 'idle';
}
