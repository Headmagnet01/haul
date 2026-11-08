# Haul plan 7: Firefox, and your own pause beats the schedule

Design agreed with the owner on 9 Nov 2026. It refines sections 4, 6, 9 and 10 of `2026-11-05-haul-design.md` and the extension design `2026-11-08-haul-browser-extension-design.md`; where they differ, this file wins.

## 1. Goal and scope

A download clicked in Firefox lands in Haul with its cookies, the same way it already does from Chrome and Edge. The Firefox add-on is signed by Mozilla, so it stays installed across Firefox restarts, and it ships inside the Haul installer.

Also in this plan: a download you pause, resume or remove yourself is no longer restarted by the schedule.

- **Success:**
  - In Firefox, a paired Haul catches a 1 MB+ download with its cookies; the download disappears from Firefox's list and completes in Haul.
  - With Haul closed, the same click downloads in Firefox as usual.
  - Pairing from Firefox shows "Firefox" in Haul's pairing dialog and in Settings › Browsers.
  - The signed `.xpi` installs in a normal Firefox release by dragging it onto the window and survives a Firefox restart.
  - Pausing a schedule-paused download by hand keeps it paused when the next window opens.
- **Out of scope:** a public addons.mozilla.org listing, Firefox for Android, Safari, code signing of `haul.exe`, the other 6a and 6b deferred minors.

## 2. Owner decisions

| Question | Decision |
|---|---|
| How the Firefox add-on is installed | Unlisted signing on addons.mozilla.org (AMO): the owner makes a free Mozilla account and API key once; a script signs each release; the signed `.xpi` ships in the installer. Rejected: a public AMO listing (later, for the public release); temporary loading only (removed on every Firefox restart). |
| Schedule vs. manual pause | Your own action wins. The schedule restarts only downloads it paused and nobody touched since. |
| Code layout | Approach A: one source, two builds. Rejected: `webextension-polyfill` (B, a dependency for a problem Firefox's `chrome.*` support already solves); a separate Firefox folder (C, every fix twice). |

## 3. Build and manifests (`extension/`)

- `build.mjs` builds the same `src/` twice:
  - `dist/`: Chrome and Edge, unchanged (the installer, the config tests and the hand-loading steps keep working).
  - `dist-firefox/`: the same scripts and pages with the Firefox manifest.
- The Firefox manifest is generated from `static/manifest.json` by one pure function, `firefoxManifest(chromium)`, in `extension/manifest.mjs` (unit-tested). It differs only in:
  - `background`: `{ "scripts": ["background.js"], "type": "module" }` instead of `service_worker`.
  - `browser_specific_settings.gecko`: `id` `haul@meraaki410.github.io`, `strict_min_version` `128.0` (the current ESR line).
  - `minimum_chrome_version` removed; `options_page` becomes `options_ui: { "page": "options.html", "open_in_tab": true }`.
- esbuild targets for the Firefox build: `firefox128`.
- **Signing:** `npm run sign:firefox` runs `web-ext sign --channel unlisted --source-dir dist-firefox --artifacts-dir firefox`, reading `WEB_EXT_API_KEY` and `WEB_EXT_API_SECRET` from the environment (never from a file in Git). `web-ext` becomes a dev dependency. The signed file is renamed to `extension/firefox/haul-firefox.xpi`.
- `extension/firefox/` always exists (it holds a short `README.txt` saying how to install the `.xpi`), so dev builds and fresh clones still compile when nothing is signed yet. `.gitignore` adds `extension/dist-firefox/` and `*.xpi`.
- **Installer:** `tauri.conf.json` adds the resource `../../extension/firefox/` → `firefox/`. The release checklist requires `haul-firefox.xpi` to be there, and a config test fails a release build without it (the same style as the existing extension checks).

## 4. The extension code

All calls stay on `chrome.*`, which Firefox's Manifest V3 supports with promises. The differences are found by feature checks at run time, not by build flags, so both builds run the same code.

- **Browser name** (`background.ts` `browserName`): if `chrome.runtime.getBrowserInfo` exists (Firefox only), use its `name` ("Firefox"); otherwise the current `userAgentData` brands check.
- **Final URL** (`background.ts` facts): `finalUrl: item.finalUrl ?? item.url`. Firefox reports one `url`, already after redirects. Cost: a form-POST download can't be told apart in Firefox, so one may be caught and then fail in Haul; Alt+click still leaves it to Firefox. This is the same trade-off Chrome has in practice (plan 5 rulings).
- **Cookies** (new pure module `cookies.ts`, unit-tested; `background.ts` passes the item):
  - When `item.cookieStoreId` is set (Firefox, including containers), every lookup passes `storeId: item.cookieStoreId`.
  - When `cookieStoreId` is set, it reads the unpartitioned cookies and, when there is an http(s) referrer, the partitioned cookies whose `partitionKey.topLevelSite` host is the referrer's host or a parent domain of it (one `partitionKey: {}` lookup returns every partition, so no public-suffix list is needed). The two lists merge by name; the partitioned value wins.
  - Chrome and Edge: unchanged (one `getAll({ url })`).
  - Any error still means "no cookies", as today.
- **Private windows:** `item.incognito` is true in Firefox private windows; the existing rule already skips them.
- Pairing, the popup, the Collector picker, the right-click menu and Alt+click need no change.

## 5. Haul's local API (`app/src-tauri/src/api/guard.rs`)

- `check_origin` also accepts `moz-extension://<uuid>`, where the uuid is 36 characters: lowercase hex and dashes in the 8-4-4-4-12 pattern. `chrome-extension://` keeps its current rule. Anything else is still 403.
- Pairing and keys are unchanged: a key is bound to the exact origin that paired. Firefox picks a random uuid per profile and install, so reinstalling the add-on means pairing again (the same as reloading an unpacked Chrome extension under a new id).
- **Risk checked first:** it is not yet known whether Firefox sends an `Origin` header on an extension's POST to `127.0.0.1`. The plan's first task loads `dist-firefox` with `web-ext run` (a throwaway profile, the installed Firefox 159) and records the headers Haul receives. If `Origin` is missing or `null`, work stops and the owner decides before any change to the locks.

## 6. Haul's Settings and welcome

- Settings › Browsers and the welcome screen get a second button, "Open the Firefox add-on folder", next to the existing extension one. It opens the installed `firefox` folder (dev builds: `extension/firefox`). Text: "Drag haul-firefox.xpi onto Firefox, choose Add, then click the Haul button and Pair with Haul."
- New command `open_firefox_folder`, the same shape as `open_extension_folder`, with the error "The Firefox add-on wasn't found. Sign it with npm run sign:firefox in the extension folder." when the `.xpi` is missing.

## 7. Schedule: your own pause wins (`crates/engine/src/queue.rs`)

- Today `auto_paused` holds every download the schedule paused, and the next window re-queues all of them that are Paused.
- Change: `pause`, `resume` and `remove` (and so Pause all and Resume all, which call them) drop the id from `auto_paused`. The window then re-queues only what the schedule paused and nobody touched.
- `auto_paused` stays in memory only. A restart outside the window keeps everything paused, as today.

## 8. Testing

- **Extension (Vitest):** `firefoxManifest` output; `decide` with no `finalUrl` uses `url`; cookie merging (no store id, store id, partitioned wins, referrer missing, an error gives none); `browserName` with and without `getBrowserInfo`.
- **Rust:** `check_origin` accepts a real-shaped `moz-extension://` uuid and refuses uppercase, wrong length, missing dashes and a trailing path; `tests/api.rs` pairs and captures from a `moz-extension://` origin; the release config test requires the `.xpi`.
- **Engine (test first):** the schedule pauses a running download; the user resumes then pauses it; the window opens; it stays Paused, while a second untouched download is re-queued.
- **Real Firefox (`web-ext run`):** the Origin probe (task 1); pair, catch a 1 MB+ download from the range server, leave a small one in Firefox, and with Haul closed the click stays in Firefox.
- **Owner checks after signing:** install the signed `.xpi` in normal Firefox, restart Firefox, check it is still there and paired; a logged-in download on a real site.
- **Release checklist:** gains the Firefox build, the signing step, the `.xpi` check and the Firefox hand checks.

## 9. Setup the owner does once

1. Sign in or create an account at addons.mozilla.org.
2. Developer Hub › Manage API Keys: create a key; keep the JWT issuer and secret.
3. Set them as user environment variables `WEB_EXT_API_KEY` and `WEB_EXT_API_SECRET` (never commit them).

The first signing also registers the add-on id under that account.
