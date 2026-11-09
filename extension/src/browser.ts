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
  const n = nav as Navigator & { userAgentData?: { brands: { brand: string }[] }; brave?: unknown };
  const brands = n.userAgentData?.brands ?? [];
  const has = (name: string) => brands.some((b) => b.brand === name);
  // Brave and Opera list Chromium (and sometimes Google Chrome) too, so they are checked first.
  if (has('Brave') || n.brave) return 'Brave';
  for (const name of ['Opera GX', 'Opera', 'Microsoft Edge', 'Google Chrome']) if (has(name)) return name;
  // Vivaldi and others report no brand of their own.
  return 'Chromium';
}
