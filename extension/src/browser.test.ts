import { expect, test } from 'vitest';
import { browserName } from './browser';

const nav = (brands?: string[]) =>
  (brands ? { userAgentData: { brands: brands.map((brand) => ({ brand, version: '1' })) } } : {}) as unknown as Navigator;

test('firefox_named_by_browser_info', async () => {
  expect(await browserName({ getBrowserInfo: async () => ({ name: 'Firefox' }) }, nav())).toBe('Firefox');
});

test('edge_named_by_brands', async () => {
  expect(await browserName({}, nav(['Chromium', 'Microsoft Edge']))).toBe('Microsoft Edge');
});

test('chrome_named_by_brands', async () => {
  expect(await browserName({}, nav(['Chromium', 'Google Chrome']))).toBe('Google Chrome');
});

test('unknown_is_chromium', async () => {
  expect(await browserName({}, nav())).toBe('Chromium');
});
