import { readFileSync } from 'node:fs';
import { expect, test } from 'vitest';
import { firefoxManifest } from './manifest.mjs';

type M = Record<string, unknown>;
const chromium = (): M => JSON.parse(readFileSync(new URL('./static/manifest.json', import.meta.url), 'utf8'));

test('firefox_background_uses_scripts', () => {
  expect(firefoxManifest(chromium()).background).toEqual({ scripts: ['background.js'], type: 'module' });
});

test('firefox_has_gecko_id', () => {
  const m = firefoxManifest(chromium()) as { browser_specific_settings?: { gecko?: unknown } };
  expect(m.browser_specific_settings?.gecko).toMatchObject({ id: 'haul@meraaki410.github.io', strict_min_version: '128.0' });
});

// Mozilla asks new add-ons to declare what they collect; Haul sends nothing anywhere but the local app.
test('firefox_declares_no_data_collection', () => {
  const m = firefoxManifest(chromium()) as { browser_specific_settings?: { gecko?: { data_collection_permissions?: unknown } } };
  expect(m.browser_specific_settings?.gecko?.data_collection_permissions).toEqual({ required: ['none'] });
});

test('firefox_drops_chrome_only_keys', () => {
  const m = firefoxManifest(chromium());
  expect(m).not.toHaveProperty('minimum_chrome_version');
  expect(m).not.toHaveProperty('options_page');
  expect(m.options_ui).toEqual({ page: 'options.html', open_in_tab: true });
});

test('firefox_keeps_everything_else', () => {
  const c = chromium();
  const m = firefoxManifest(c);
  for (const k of ['permissions', 'host_permissions', 'content_scripts', 'action', 'icons', 'version', 'name', 'manifest_version']) {
    expect(m[k], k).toEqual(c[k]);
  }
});

test('input_is_not_changed', () => {
  const c = chromium();
  const before = structuredClone(c);
  firefoxManifest(c);
  expect(c).toEqual(before);
});
