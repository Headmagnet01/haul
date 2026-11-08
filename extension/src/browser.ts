/** What `runtime` offers for naming the browser: `getBrowserInfo` exists in Firefox only. */
export interface RuntimeLike {
  getBrowserInfo?: () => Promise<{ name: string }>;
}

/** The browser's name as Haul shows it when pairing. */
export async function browserName(runtime: RuntimeLike, nav: Navigator): Promise<string> {
  if (runtime.getBrowserInfo) {
    try {
      return (await runtime.getBrowserInfo()).name;
    } catch {
      // Fall through to the Chromium check.
    }
  }
  const brands = (nav as Navigator & { userAgentData?: { brands: { brand: string }[] } }).userAgentData?.brands ?? [];
  if (brands.some((b) => b.brand === 'Microsoft Edge')) return 'Microsoft Edge';
  if (brands.some((b) => b.brand === 'Google Chrome')) return 'Google Chrome';
  return 'Chromium';
}
