// The Cookie header Haul gets with a download.
//
// Chrome and Edge: one plain lookup for the URL.
// Firefox: cookies live per container (`cookieStoreId`), and with Total Cookie Protection a site
// opened under another site keeps its cookies in a partition named after that top-level site. So
// Firefox reads the container's unpartitioned cookies plus those partitioned under the referrer's
// site; a partitioned cookie wins over an unpartitioned one of the same name.
// With first-party isolation on (`privacy.firstparty.isolate`), every lookup must name a
// `firstPartyDomain`: Firefox is asked for all of them and only the referrer's site is kept.

export interface CookieLike {
  name: string;
  value: string;
  partitionKey?: { topLevelSite?: string };
  /** Firefox with first-party isolation: the site the cookie was set under. */
  firstPartyDomain?: string;
}

export interface CookieApi {
  getAll(details: Record<string, unknown>): Promise<CookieLike[]>;
}

/** What the download (or tab) says about where it came from. */
export interface CookieSource {
  /** Firefox only: the container the download belongs to. */
  cookieStoreId?: string;
  referrer?: string;
}

function httpHost(u: string | undefined): string | null {
  if (!u) return null;
  try {
    const p = new URL(u);
    return p.protocol === 'http:' || p.protocol === 'https:' ? p.hostname : null;
  } catch {
    return null;
  }
}

/** Whether `host` is `domain` or under it. */
function covers(domain: string, host: string): boolean {
  return domain !== '' && (host === domain || host.endsWith(`.${domain}`));
}

/** Firefox refuses a lookup without `firstPartyDomain` while first-party isolation is on. */
function needsFirstParty(e: unknown): boolean {
  return String(e instanceof Error ? e.message : e).includes('firstPartyDomain');
}

/** Whether a partition's top-level site is the referrer's site: the same host or a parent domain of it. */
function sameSite(topLevelSite: string | undefined, referrerHost: string): boolean {
  const site = httpHost(topLevelSite);
  return site !== null && (referrerHost === site || referrerHost.endsWith(`.${site}`));
}

export async function cookieHeader(api: CookieApi, url: string, src: CookieSource): Promise<string | undefined> {
  try {
    let list: CookieLike[];
    if (src.cookieStoreId === undefined) {
      list = await api.getAll({ url });
    } else {
      const storeId = src.cookieStoreId;
      const ref = httpHost(src.referrer);
      try {
        // `partitionKey: {}` returns every partition as well as the unpartitioned cookies.
        const [plain, all] = await Promise.all([api.getAll({ url, storeId }), api.getAll({ url, storeId, partitionKey: {} })]);
        const parts = ref === null ? [] : all.filter((c) => c.partitionKey?.topLevelSite && sameSite(c.partitionKey.topLevelSite, ref));
        list = [...plain.filter((c) => !c.partitionKey?.topLevelSite), ...parts];
      } catch (e) {
        if (!needsFirstParty(e)) throw e;
        // `null` asks for every first party; keep the ones set under the referrer's site (or the URL's).
        const site = ref ?? httpHost(url);
        const all = await api.getAll({ url, storeId, firstPartyDomain: null });
        list = site === null ? [] : all.filter((c) => covers(c.firstPartyDomain ?? '', site));
      }
    }
    const byName = new Map<string, string>();
    for (const c of list) byName.set(c.name, c.value);
    return byName.size ? [...byName].map(([n, v]) => `${n}=${v}`).join('; ') : undefined;
  } catch {
    return undefined;
  }
}
