# Haul Plan 7: Firefox, and your own pause beats the schedule — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The Haul extension runs in Firefox (signed by Mozilla, shipped in the installer) with the same catch, cookies and pairing as Chrome and Edge, and a hand pause/resume/remove takes a download off the schedule's restart list.

**Architecture:** One extension source, two builds: `extension/dist` (Chromium, unchanged) and `extension/dist-firefox` (manifest rewritten by one pure function). Browser differences are run-time feature checks in small pure modules. Haul's API guard accepts `moz-extension://<uuid>`. The engine queue forgets a download's auto-pause on any hand action.

**Tech Stack:** TypeScript + esbuild + Vitest (extension), `web-ext` (signing), Rust/axum (API guard), Tauri 2 + Svelte 5 + Playwright (app), Firefox WebDriver BiDi (real-browser test).

**Spec:** `docs/superpowers/specs/2026-11-09-haul-plan-7-firefox-design.md`

## Global Constraints

- Gecko id `haul@meraaki410.github.io`; `strict_min_version` `128.0`; esbuild target `firefox128` for the Firefox build.
- `extension/dist/` output and the Chromium manifest do not change.
- API keys only from env vars `WEB_EXT_API_KEY` and `WEB_EXT_API_SECRET`; never written to a file in the repo.
- Signed add-on path: `extension/firefox/haul-firefox.xpi`; installed resource folder `firefox/`.
- `.gitignore` adds `extension/dist-firefox/` and `*.xpi`.
- Extension code keeps calling `chrome.*`; no `webextension-polyfill`.
- Copy, exact: button "Open the Firefox add-on folder"; help "Drag haul-firefox.xpi onto Firefox, choose Add, then click the Haul button and Pair with Haul."; error "The Firefox add-on wasn't found. Sign it with npm run sign:firefox in the extension folder."
- Write the code yourself (owner's standing rule); no worker delegation.
- Rust tests while `tauri dev` runs: `CARGO_TARGET_DIR=_tmp/target-alt`.
- Scratch files go in `_tmp/`.

## Rulings made while planning (cost if wrong)

- **Partitioned cookies without a public-suffix list.** The spec says "partitioned under the referrer's site". Computing a site needs the public-suffix list, so instead the lookup passes `partitionKey: {}` (Firefox returns every partition) and keeps a partitioned cookie only when its `partitionKey.topLevelSite` host equals the referrer's host or is a dot-suffix of it. Cost: a cookie partitioned under a parent domain the referrer shares only through a public suffix (e.g. `co.uk`) could slip through; Firefox never creates such a site, so none in practice.
- **Release check runs in `beforeBundleCommand`, not `cargo test`.** The spec says "a config test fails a release build without it"; `cargo test` never builds a release, so a `check-release.mjs` script runs as Tauri's `beforeBundleCommand` and the config test asserts that wiring. Cost: none.
- **The Firefox button also goes in Settings › Browsers**, which today has no extension button at all; both buttons are added there. Cost: two buttons.
- **`final URL` mapping is a pure function** `factsFromItem` in `decide.ts`, so it is unit-tested rather than living untested in `background.ts`.

## Review Focus

1. **A download from a Firefox container tab** (`cookieStoreId: "firefox-container-2"`): only that container's cookies are sent — Task 4 `cookies_use_the_items_store`.
2. **Referrer missing or not http(s)** (`about:blank`, `moz-extension:`): no partitioned cookies are kept and nothing throws — Task 4 `no_referrer_keeps_only_unpartitioned`.
3. **Chrome and Edge must not change**: one `getAll({ url })` call, no `storeId`, no `partitionKey` (Chrome rejects unknown keys) — Task 4 `chromium_lookup_is_unchanged`.
4. **Signing the same version twice** (AMO refuses a reused version) or an `.xpi` left from an older version: the release check fails with a message naming both versions — Task 5 `stale_xpi_fails`.
5. **A Firefox origin with an uppercase or wrong-length uuid, or a path after it**: refused (403), never matched to a Chrome key — Task 3 `origin_accepts_firefox_uuid` bad cases.

---

### Task 1: The Firefox build

**Files:**
- Create: `extension/manifest.mjs`, `extension/manifest.test.ts`, `extension/firefox/README.txt`
- Modify: `extension/build.mjs`, `.gitignore`

**Interfaces:**
- Produces: `firefoxManifest(chromium: object): object` (pure, does not mutate its input); `extension/dist-firefox/` after `npm run build`; `extension/firefox/` always present.

- [ ] **Step 1: Write the failing tests** in `extension/manifest.test.ts`, reading the real `static/manifest.json`:
  - `firefox_background_uses_scripts`: `background` equals `{ scripts: ['background.js'], type: 'module' }`.
  - `firefox_has_gecko_id`: `browser_specific_settings.gecko` equals `{ id: 'haul@meraaki410.github.io', strict_min_version: '128.0' }`.
  - `firefox_drops_chrome_only_keys`: no `minimum_chrome_version`, no `options_page`; `options_ui` equals `{ page: 'options.html', open_in_tab: true }`.
  - `firefox_keeps_everything_else`: `permissions`, `host_permissions`, `content_scripts`, `action`, `icons`, `version`, `name` deep-equal the input's.
  - `input_is_not_changed`: the input still deep-equals a `structuredClone` taken before the call.
- [ ] **Step 2:** `npm --prefix extension test -- manifest` → FAIL (module not found).
- [ ] **Step 3: Implement `firefoxManifest`** in `extension/manifest.mjs`, with a JSDoc type comment for the TS test.
- [ ] **Step 4: Extend `build.mjs`:** after the Chromium build, copy `static` to `dist-firefox`, write `firefoxManifest(JSON.parse(static/manifest.json))` as `dist-firefox/manifest.json` (2-space JSON), and run the same two esbuild passes with `target: 'firefox128'`, `outdir: 'dist-firefox'`. `mkdirSync('firefox', { recursive: true })` and write `firefox/README.txt` if missing, with the help copy from Global Constraints plus "Sign it with npm run sign:firefox."
- [ ] **Step 5:** `.gitignore`: add `extension/dist-firefox/` and `*.xpi`. Commit `extension/firefox/README.txt`.
- [ ] **Step 6:** `npm --prefix extension test`, `npm --prefix extension run check`, `npm --prefix extension run build` → all pass; `extension/dist/manifest.json` still deep-equals `static/manifest.json`; `dist-firefox/manifest.json` has the gecko block.
- [ ] **Step 7:** Commit `feat(extension): Firefox build from the same source`.

### Task 2: Firefox BiDi helper and the Origin probe (gate)

**Files:**
- Create: `app/e2e-real/firefox-bidi.ts`, `_tmp/origin-probe.mjs` (scratch, not committed)

**Interfaces:**
- Produces: `class Firefox { static launch(opts: { profileDir: string; port: number; prefs: Record<string, string | number | boolean> }): Promise<Firefox>; installTemporary(dir: string): Promise<void>; openTab(url: string): Promise<Tab>; close(): Promise<void> }` and `class Tab { eval<T>(expr: string): Promise<T>; navigate(url: string): Promise<void> }`. Constants: `FIREFOX = 'C:\\Program Files\\Mozilla Firefox\\firefox.exe'`, `EXT_UUID = '6c1d9f2e-1b7a-4e3c-9a55-0d2f7e8b4c31'`.

- [ ] **Step 1: Implement the helper.** Launch `firefox.exe -profile <dir> -no-remote --remote-debugging-port <port>` with a `user.js` holding the prefs; always include `extensions.webextensions.uuids` = `{"haul@meraaki410.github.io":"<EXT_UUID>"}` (JSON string), `browser.download.useDownloadDir=true`, `browser.download.dir=<profileDir>\\dl`, `browser.download.folderList=2`. Connect to `ws://127.0.0.1:<port>/session` (Node's global `WebSocket`), `session.new`, `webExtension.install` with `extensionData: { type: 'path', path: dir }`, `browsingContext.create`, `browsingContext.navigate` (`wait: 'complete'`), `script.evaluate` with `awaitPromise: true` and the result's primitive/JSON value. Same shape as the Edge `Target` class in `extension.spec.ts`.
- [ ] **Step 2: Write `_tmp/origin-probe.mjs`:** a Node `http` server on `127.0.0.1:17654` that logs method, path and every header of each request and answers `/v1/status` with `200 {"app":"haul","paired":false,"active":0}`. Haul must not be running (port free).
- [ ] **Step 3: Run the probe:** launch Firefox with the helper (temp profile under `_tmp/ff-probe`), install `extension/dist-firefox`, open `moz-extension://<EXT_UUID>/popup.html` (its load posts `/v1/status`). Run it via a one-off `npx tsx` script in `_tmp/`.
- [ ] **Step 4: Record the result** in the ledger (`docs/superpowers/plans/2026-11-09-haul-plan-7-firefox.ledger.md`): the exact `Origin` and `Host` headers seen.
  - `Origin: moz-extension://<EXT_UUID>` → carry on.
  - Missing or `null` → **stop**; tell the owner what was seen and wait for their decision before Task 3.
- [ ] **Step 5:** Commit `test: Firefox WebDriver BiDi helper` (helper and ledger only).

### Task 3: Haul accepts Firefox origins

**Files:**
- Modify: `app/src-tauri/src/api/guard.rs` (`check_origin`, tests), `app/src-tauri/tests/api.rs`

**Interfaces:**
- Produces: `check_origin(origin: Option<&str>) -> Option<&str>` accepting both shapes; signature unchanged.

- [ ] **Step 1: Write the failing tests.**
  - `guard.rs` `origin_accepts_firefox_uuid`: `Some("moz-extension://6c1d9f2e-1b7a-4e3c-9a55-0d2f7e8b4c31")` returns itself. `None` for: the same uuid uppercased; 35 and 37 characters; no dashes (32 hex); dashes in the wrong places; `"moz-extension://<uuid>/"`; `"moz-extension://<uuid>/popup.html"`; `"moz-extension://"`; a non-hex `g` in it.
  - `origin_must_be_extension` keeps its current cases.
  - `tests/api.rs` `firefox_pairs_and_captures`: pair from `moz-extension://<uuid>`, then `/v1/capture` with that token and origin → 200; the same token from `chrome-extension://testid` → 401.
- [ ] **Step 2:** `cargo test -p haul-app guard` and `--test api firefox` → FAIL.
- [ ] **Step 3: Implement:** a `FIREFOX_SCHEME = "moz-extension://"` branch requiring exactly 36 chars, `-` at indexes 8, 13, 18, 23, lowercase hex elsewhere. Update the doc comment.
- [ ] **Step 4:** `cargo test -p haul-app` → all pass; `cargo clippy -p haul-app --all-targets -- -D warnings` clean.
- [ ] **Step 5:** Commit `feat(api): accept Firefox extension origins`.

### Task 4: Extension code works in Firefox

**Files:**
- Create: `extension/src/cookies.ts`, `extension/src/cookies.test.ts`, `extension/src/browser.ts`, `extension/src/browser.test.ts`
- Modify: `extension/src/decide.ts` (+ `factsFromItem`), `extension/src/decide.test.ts`, `extension/src/background.ts`

**Interfaces:**
- Produces:
  - `factsFromItem(item: { url: string; finalUrl?: string; filename: string; fileSize: number; totalBytes: number; incognito: boolean }): DownloadFacts` — `finalUrl: item.finalUrl || item.url`; `size` as `background.ts` computes it today.
  - `type CookieApi = { getAll(details: Record<string, unknown>): Promise<{ name: string; value: string; partitionKey?: { topLevelSite?: string } }[]> }`
  - `cookieHeader(api: CookieApi, url: string, item: { cookieStoreId?: string; referrer?: string }): Promise<string | undefined>`
  - `browserName(runtime: { getBrowserInfo?: () => Promise<{ name: string }> }, nav: Navigator): Promise<string>`

- [ ] **Step 1: Write the failing tests.**
  - `decide.test.ts` `firefox_item_without_final_url_uses_url`: `factsFromItem({ url: 'https://a.org/x.zip', filename: '', fileSize: -1, totalBytes: 5_000_000, incognito: false })` has `finalUrl` `'https://a.org/x.zip'`, `size` 5_000_000, and `decide` on it is `'catch'`. `chrome_item_keeps_final_url`: a `finalUrl` given is kept.
  - `cookies.test.ts` with a fake `CookieApi` that records each `details`:
    - `chromium_lookup_is_unchanged`: no `cookieStoreId` → exactly one call, `{ url }` only; `a=1; b=2` out.
    - `cookies_use_the_items_store`: `cookieStoreId: 'firefox-container-2'` → every call has `storeId: 'firefox-container-2'`.
    - `partitioned_wins_for_referrer_site`: referrer `https://www.shop.example/p`; unpartitioned `sid=old`, partitioned `sid=new` with `topLevelSite: 'https://shop.example'` → `sid=new`.
    - `other_sites_partitions_dropped`: partitioned `sid=x` under `https://other.example` → not sent.
    - `no_referrer_keeps_only_unpartitioned`: referrer `undefined`, and again `'about:blank'` → partitioned cookies dropped, no throw.
    - `errors_mean_no_cookies`: `getAll` rejects → `undefined`.
    - `no_cookies_is_undefined`: empty lists → `undefined`.
  - `browser.test.ts`: `getBrowserInfo` resolving `{ name: 'Firefox' }` → `'Firefox'`; absent with Edge brands → `'Microsoft Edge'`; absent with Chrome brands → `'Google Chrome'`; absent, no brands → `'Chromium'`.
- [ ] **Step 2:** `npm --prefix extension test` → the new tests FAIL.
- [ ] **Step 3: Implement.** `cookieHeader`, Firefox path: two calls, `{ url, storeId }` and `{ url, storeId, partitionKey: {} }`; from the second keep cookies with a `partitionKey.topLevelSite` whose host equals the referrer host or is a `.`-suffix of it (referrer must parse as http/https); merge by name, partitioned last so it wins; join `name=value` with `; `. Catch all errors → `undefined`. Move `browserName` to `browser.ts` (async now; callers await it).
- [ ] **Step 4: Wire `background.ts`:** facts from `factsFromItem(item)`; `const url = facts.finalUrl`; cookies from `cookieHeader(chrome.cookies, url, item)`; pairing uses `await browserName(chrome.runtime, navigator)`. `@types/chrome` lacks `cookieStoreId`/`getBrowserInfo`: use narrow local types, no `any`.
- [ ] **Step 5:** `npm --prefix extension test`, `run check`, `run build` → pass.
- [ ] **Step 6:** Commit `feat(extension): Firefox names, final URL and container/partition cookies`.

### Task 5: Signing and the release check

**Files:**
- Create: `extension/sign.mjs`, `extension/check-release.mjs`, `extension/check-release.test.ts`
- Modify: `extension/package.json` (script `sign:firefox`, dev dependency `web-ext`), `app/src-tauri/tauri.conf.json`, `app/src-tauri/tests/config.rs`

**Interfaces:**
- Consumes: `dist-firefox/` (Task 1).
- Produces: `checkRelease(dir: string, manifestVersion: string): string | null` (null = OK, else the error text) exported from `check-release.mjs`; running the file as a script exits 1 with that text.

- [ ] **Step 1: Write the failing tests.**
  - `check-release.test.ts` (temp dirs): `missing_xpi_fails` → message contains `npm run sign:firefox`; `stale_xpi_fails` (`haul-firefox.version` holds `0.0.9`, manifest `0.1.0`) → message contains both versions; `matching_xpi_passes` → `null`.
  - `config.rs` `bundled_firefox_folder`: `bundle.resources["../../extension/firefox/"] == "firefox/"` and `extension/firefox/README.txt` exists. `before_bundle_checks_firefox`: `build.beforeBundleCommand == "node ../extension/check-release.mjs"`.
- [ ] **Step 2:** run both → FAIL.
- [ ] **Step 3: Implement `check-release.mjs`** (version from `static/manifest.json` when run as a script) and **`sign.mjs`:** refuse with a clear message if either env var is missing; `npm run build`; run `web-ext sign --channel unlisted --source-dir dist-firefox --artifacts-dir firefox/.signed` with `--api-key`/`--api-secret` from the env vars (passed as arguments to the child process, never logged); move the produced `.xpi` to `firefox/haul-firefox.xpi`; write `firefox/haul-firefox.version`. If web-ext reports the version already exists, print "Mozilla already has version X. Raise version in extension/static/manifest.json and sign again." Add `firefox/.signed/` and `haul-firefox.version` to `.gitignore`.
- [ ] **Step 4:** `tauri.conf.json`: add the resource and `"beforeBundleCommand": "node ../extension/check-release.mjs"`.
- [ ] **Step 5:** extension tests and `cargo test -p haul-app --test config` → pass. `sign:firefox` without env vars prints the missing-key message and exits 1.
- [ ] **Step 6:** Commit `build: sign the Firefox add-on and require it in the installer`.

### Task 6: "Open the Firefox add-on folder"

**Files:**
- Modify: `app/src-tauri/src/commands.rs`, `app/src-tauri/src/lib.rs` (handler list), `app/src/lib/backend.ts`, `backend-tauri.ts`, `backend-mock.ts`, `app/src/components/Welcome.svelte`, `app/src/components/BrowserList.svelte`, `app/e2e/welcome.spec.ts`, `app/e2e/settings.spec.ts`

**Interfaces:**
- Produces: Tauri command `open_firefox_folder`; `Backend.openFirefoxFolder(): Promise<void>`.

- [ ] **Step 1: Write the failing e2e tests** (mock backend records calls the way `openExtensionFolder` does today):
  - `welcome.spec.ts` `firefox_folder_button`: the welcome shows the help copy and the button "Open the Firefox add-on folder"; clicking it calls `openFirefoxFolder` once; a rejection with the error copy shows it as a toast.
  - `settings.spec.ts` `browsers_has_both_folder_buttons`: Settings › Browsers shows "Open the extension folder" and "Open the Firefox add-on folder", each calling its backend method.
- [ ] **Step 2:** `npx playwright test welcome settings` → FAIL.
- [ ] **Step 3: Implement** `open_firefox_folder` exactly like `open_extension_folder`: bundled `resource_dir/firefox` when it holds `haul-firefox.xpi`; in debug builds `extension/firefox` when it holds it; else the error copy. Add the method to the three backends and the buttons (same `btn` + `folder` icon) to Welcome (under the Edge/Chrome text) and BrowserList.
- [ ] **Step 4:** `npx playwright test`, `npm run check`, Vitest, `cargo test -p haul-app` → pass. Run the a11y spec too.
- [ ] **Step 5:** Commit `feat: open the Firefox add-on folder from the welcome and Settings`.

### Task 7: Your own pause beats the schedule

**Files:**
- Modify: `crates/engine/src/queue.rs` (`pause`, `resume`, `remove`), `crates/engine/tests/queue.rs`

- [ ] **Step 1: Write the failing test** `hand_pause_beats_schedule` next to `schedule_pauses_running_and_resumes`: two slow downloads `a`, `b` running; set the never-window schedule; wait both Paused; `q.resume(a)` then `q.pause(a)`; wait `a` Paused; `q.set_schedule(None)`; wait `b` Running; sleep 1 s; assert `a` is still Paused. Also `hand_remove_forgets_auto_pause`: after the schedule pauses `a`, `q.remove(a)`, then lift the schedule → no panic and no `StateChanged` for `a` after its `Removed`.
- [ ] **Step 2:** `cargo test -p haul-engine --test queue hand_` → `hand_pause_beats_schedule` FAILS (a re-queued).
- [ ] **Step 3: Implement:** at the start of `pause`, `resume` and `remove`, `self.inner.auto_paused.lock().unwrap().remove(&id)`. Mind lock order: take it before `running`, or release it before locking `running` (the dispatcher locks `running` then `auto_paused`), so never hold `auto_paused` while locking `running`.
- [ ] **Step 4:** `cargo test -p haul-engine` (run the queue tests 3× for timing) and clippy → pass.
- [ ] **Step 5:** Update the handoff's "Schedule vs. manual pause" line to settled. Commit `fix(engine): a hand pause, resume or remove beats the schedule`.

### Task 8: Real Firefox test

**Files:**
- Create: `app/e2e-real/firefox.spec.ts`

**Interfaces:**
- Consumes: `Firefox`, `Tab`, `EXT_UUID` (Task 2); a running `tauri dev` with DevTools port 9222 and the range server, as `extension.spec.ts` needs.

- [ ] **Step 1: Write the test**, serial, following `extension.spec.ts` for Haul's window (Playwright `connectOverCDP` to 9222) and the range server:
  - `firefox_pair_and_capture`: launch Firefox (temp profile), install `extension/dist-firefox`, open `moz-extension://<EXT_UUID>/popup.html`, click Pair (evaluate in that tab), Allow in Haul's pairing dialog, and check the dialog named "Firefox". Open a page served by the test's own page server with a link to a 4 MiB range-server file and click it. Expect it in Haul's Queue, then Completed, and Firefox's download list (evaluate `chrome.downloads.search({})` in the popup tab) has no in-progress copy.
  - `firefox_small_file_stays`: a 10 KiB `.bin` stays in Firefox (completes in the profile's `dl` folder), nothing new in Haul.
  - `firefox_kept_when_unpaired`: clear the token (`chrome.storage.local`) and click the 4 MiB link → completes in Firefox.
- [ ] **Step 2:** start the range server and `tauri dev` (per `extension.spec.ts` header), build the extension, run `npx playwright test -c playwright.real.config.ts firefox` → all pass. If a step fails, fix the cause in the extension or app with a unit test that fails first.
- [ ] **Step 3:** Commit `test: real Firefox pairs and hands over a download`.

### Task 9: Docs, signing and owner checks

**Files:**
- Modify: `docs/release-checklist.md`, `handoff.md`, `docs/superpowers/specs/2026-11-09-haul-plan-7-firefox-design.md` (§4 cookie wording per the ruling above)

- [ ] **Step 1: Release checklist:** build includes `dist-firefox`; signing step (`npm run sign:firefox`, env vars, raise the version for each signing); `beforeBundleCommand` refuses a missing or stale `.xpi`; hand checks: install the `.xpi` in normal Firefox by drag, restart Firefox, still installed and paired; a logged-in download from a real site; a container tab download.
- [ ] **Step 2: Owner, once:** create the AMO API key (spec §9) and set the env vars. Then run `npm --prefix extension run sign:firefox` and confirm `extension/firefox/haul-firefox.xpi` exists.
- [ ] **Step 3: Owner checks:** the release checklist's Firefox hand checks on a rebuilt installer.
- [ ] **Step 4: Handoff:** a "Plan 7" section (what was built, rulings, Origin probe result, owner checks, deferred minors). Commit `docs: Plan 7 handoff`.
