// The Firefox manifest, made from the Chromium one in static/manifest.json.

export const GECKO_ID = 'haul@meraaki410.github.io';

/**
 * Firefox's Manifest V3 runs the background as an event page and wants a fixed add-on id.
 * @param {Record<string, unknown>} chromium
 * @returns {Record<string, unknown>}
 */
export function firefoxManifest(chromium) {
  const m = structuredClone(chromium);
  delete m.minimum_chrome_version;
  if (typeof m.options_page === 'string') {
    m.options_ui = { page: m.options_page, open_in_tab: true };
    delete m.options_page;
  }
  m.background = { scripts: ['background.js'], type: 'module' };
  m.browser_specific_settings = { gecko: { id: GECKO_ID, strict_min_version: '128.0' } };
  return m;
}
