import { expect, test } from 'vitest';
import { cookieHeader, type CookieApi, type CookieLike } from './cookies';

const URL_ = 'https://dl.shop.example/file.zip';

/** A fake cookie store: `plain` for unpartitioned lookups, `parts` for `partitionKey: {}` ones. */
function fake(plain: CookieLike[], parts: CookieLike[] = []) {
  const calls: Record<string, unknown>[] = [];
  const api: CookieApi = {
    async getAll(details) {
      calls.push(details);
      return 'partitionKey' in details ? [...plain, ...parts] : plain;
    },
  };
  return { api, calls };
}

const c = (name: string, value: string, site?: string): CookieLike =>
  site ? { name, value, partitionKey: { topLevelSite: site } } : { name, value };

test('chromium_lookup_is_unchanged', async () => {
  const { api, calls } = fake([c('a', '1'), c('b', '2')]);
  expect(await cookieHeader(api, URL_, { referrer: 'https://shop.example/' })).toBe('a=1; b=2');
  expect(calls).toEqual([{ url: URL_ }]);
});

test('cookies_use_the_items_store', async () => {
  const { api, calls } = fake([c('a', '1')]);
  await cookieHeader(api, URL_, { cookieStoreId: 'firefox-container-2', referrer: 'https://shop.example/' });
  expect(calls.length).toBeGreaterThan(0);
  for (const d of calls) expect(d.storeId).toBe('firefox-container-2');
});

test('partitioned_wins_for_referrer_site', async () => {
  const { api } = fake([c('sid', 'old'), c('lang', 'en')], [c('sid', 'new', 'https://shop.example')]);
  const h = await cookieHeader(api, URL_, { cookieStoreId: 'firefox-default', referrer: 'https://www.shop.example/p' });
  expect(h).toBe('sid=new; lang=en');
});

test('exact_referrer_host_matches_its_partition', async () => {
  const { api } = fake([], [c('sid', 'x', 'https://shop.example')]);
  expect(await cookieHeader(api, URL_, { cookieStoreId: 'firefox-default', referrer: 'https://shop.example/' })).toBe('sid=x');
});

test('other_sites_partitions_dropped', async () => {
  const { api } = fake([c('a', '1')], [c('sid', 'x', 'https://other.example'), c('t', 'y', 'https://notshop.example')]);
  expect(await cookieHeader(api, URL_, { cookieStoreId: 'firefox-default', referrer: 'https://shop.example/' })).toBe('a=1');
});

test('no_referrer_keeps_only_unpartitioned', async () => {
  for (const referrer of [undefined, 'about:blank', 'moz-extension://x/picker.html']) {
    const { api } = fake([c('a', '1')], [c('sid', 'x', 'https://shop.example')]);
    expect(await cookieHeader(api, URL_, { cookieStoreId: 'firefox-default', referrer }), String(referrer)).toBe('a=1');
  }
});

test('errors_mean_no_cookies', async () => {
  const api: CookieApi = { getAll: () => Promise.reject(new Error('no permission')) };
  expect(await cookieHeader(api, URL_, {})).toBeUndefined();
  expect(await cookieHeader(api, URL_, { cookieStoreId: 'firefox-default', referrer: 'https://shop.example/' })).toBeUndefined();
});

test('no_cookies_is_undefined', async () => {
  expect(await cookieHeader(fake([]).api, URL_, {})).toBeUndefined();
  expect(await cookieHeader(fake([]).api, URL_, { cookieStoreId: 'firefox-default' })).toBeUndefined();
});

test('first_party_isolation_keeps_the_referrers_cookies', async () => {
  const calls: Record<string, unknown>[] = [];
  const api: CookieApi = {
    async getAll(details) {
      calls.push(details);
      if (!('firstPartyDomain' in details)) {
        throw new Error("First-Party Isolation is enabled, but the required 'firstPartyDomain' attribute was not set.");
      }
      return [
        { name: 'sid', value: '1', firstPartyDomain: 'shop.example' },
        { name: 'track', value: '9', firstPartyDomain: 'ads.example' },
      ];
    },
  };
  const header = await cookieHeader(api, URL_, { cookieStoreId: 'firefox-default', referrer: 'https://www.shop.example/p' });
  expect(header).toBe('sid=1');
  expect(calls.at(-1)).toEqual({ url: URL_, storeId: 'firefox-default', firstPartyDomain: null });
});

test('first_party_isolation_without_referrer_uses_the_url_site', async () => {
  const api: CookieApi = {
    async getAll(details) {
      if (!('firstPartyDomain' in details)) throw new Error("the required 'firstPartyDomain' attribute was not set");
      return [
        { name: 'a', value: '1', firstPartyDomain: 'shop.example' },
        { name: 'b', value: '2', firstPartyDomain: 'other.example' },
      ];
    },
  };
  expect(await cookieHeader(api, URL_, { cookieStoreId: 'firefox-default' })).toBe('a=1');
});
