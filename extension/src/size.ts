/** How often, and how many times, to ask again for a size the browser didn't know yet (1.5 s in all). */
export const SIZE_TRIES = 15;
export const SIZE_STEP_MS = 100;

/**
 * The download's size once the browser knows it, or null if it never says within the tries.
 * Firefox fires onCreated before the response headers arrive, so its first item has no size.
 */
export async function waitForSize(
  read: () => Promise<{ totalBytes: number; fileSize: number } | undefined>,
  tries: number,
  sleep: (ms: number) => Promise<void>,
): Promise<number | null> {
  for (let i = 0; i < tries; i++) {
    let item: { totalBytes: number; fileSize: number } | undefined;
    try {
      item = await read();
    } catch {
      return null;
    }
    if (!item) return null;
    if (item.fileSize > 0) return item.fileSize;
    if (item.totalBytes > 0) return item.totalBytes;
    if (i < tries - 1) await sleep(SIZE_STEP_MS);
  }
  return null;
}
