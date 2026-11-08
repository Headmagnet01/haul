import type { CaptureBody } from './types';

/** The bit of `chrome.downloads` the hand-over needs. */
export interface DownloadsApi {
  pause(id: number): Promise<void>;
  resume(id: number): Promise<void>;
  cancel(id: number): Promise<void>;
  erase(id: number): Promise<void>;
}

export interface HaulClient {
  capture(body: CaptureBody): Promise<'ok' | 'unpaired' | 'failed'>;
}

/**
 * Moves a browser download to Haul without ever losing it: pause, ask Haul, and only cancel the
 * browser's copy once Haul said yes. Anything else lets the browser carry on.
 * 'gone': the download had already finished (or vanished) before it could be paused.
 */
export async function handOver(id: number, body: CaptureBody, d: DownloadsApi, haul: HaulClient): Promise<'sent' | 'kept' | 'gone'> {
  try {
    await d.pause(id);
  } catch {
    return 'gone';
  }
  let r: 'ok' | 'unpaired' | 'failed';
  try {
    r = await haul.capture(body);
  } catch {
    r = 'failed';
  }
  if (r !== 'ok') {
    await d.resume(id).catch(() => {});
    return 'kept';
  }
  await d.cancel(id).catch(() => {});
  await d.erase(id).catch(() => {});
  return 'sent';
}
