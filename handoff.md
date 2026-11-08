# Haul: handoff

Last updated: 8 Nov 2026 (night). Plans 1–5 and 6a are merged into `master`. **Plan 6b (installer, update check, Collector cookies, quoted autostart) is done on branch `feat/packaging`**, not yet merged: see "Plan 6b" below.

**Plan 6a: all 9 tasks done and the review's fix pass done** merged into `master` (branch `feat/app-shell` deleted). Spec `docs/superpowers/specs/2026-11-08-haul-plan-6a-app-shell-design.md`, plan `docs/superpowers/plans/2026-11-08-haul-plan-6a-app-shell.md`; the executor ledger (every ruling) is kept at `docs/superpowers/plans/2026-11-08-haul-plan-6a-app-shell.ledger.md`. Built: `finished_at` + a 5 s speed window, the Settings record and page, tray / close to tray / single instance / autostart, the Finished view, When done with a countdown, toasts with Open folder, and a first-run welcome. **Next: Plan 6b** (packaging and the rest of Plan 6).

## What Haul is
A free, open-source Windows download manager. It pairs IDM's fast engine and browser capture with JDownloader 2's link collector, packages and auto-extraction, in a calm "dispatch board" interface.
- `PRODUCT.md`: product principles.
- `DESIGN.md`: design tokens and rules.
- `docs/mockup/index.html`: the clickable mockup.
- `docs/superpowers/specs/2026-11-05-haul-design.md`: the spec, the source of truth.

## Where things stand
| | |
|---|---|
| Branch | `master` holds everything through Plan 6a. Plan 6b is on `feat/packaging` (8 tasks done, final review next, merge on the owner's OK). No Git remote exists. |
| Plan 6b (8 Nov) | Done: dev/release identifiers, quoted autostart module, Collector captures keep cookies (I5), daily update check + UI, per-user NSIS installer. Rust 428 tests, front end 111 Vitest + 170 Playwright. Installer built and installed by the owner to `C:\tools\Haul`; checks 1–4 and 6 passed. **Still open:** autostart after a real sign-in, and the uninstall check. |
| Done | Plans 1–5 merged. Plan 5: `docs/superpowers/plans/2026-11-08-haul-plan-5-browser-extension.md`, Tasks 1–10, design in `docs/superpowers/specs/2026-11-08-haul-browser-extension-design.md`. |
| Tests (8 Nov, Plan 6a after the fix pass) | Rust: 380 tests. Front end: 105 Vitest, 160 Playwright e2e runs (light and dark). Clippy, rustfmt and svelte-check are clean. Real-app smoke 6/6, including `close_keeps_downloading` and `second_launch_shows_first`. **Engine timing flakes predate 6a** (measured: `master` failed 3 of 4 engine-only runs, the branch 1 of 6): `crash_simulation`, `speed_limit_respected`, `slow_connection_replaced`, `stalled_connections_replaced`, `restart_resumes_interrupted` fail now and then under load and pass alone. Rerun before believing one. |
| Real browser (8 Nov) | `app/e2e-real/extension.spec.ts` loaded the extension into Edge, found Haul, paired through the real Allow dialog. It found a real bug (status must be a POST, below). The click → Haul steps could not run: Edge started from the sandboxed session never completes a download, even with no extension and no Playwright attached. |
| Review | A fresh reviewer (8 Nov): no Critical; 3 Important, all fixed test-first (stale pairing dialog, no-final-URL downloads, a stuck Pair button). Minors deferred, listed in the Plan 5 section. |
| Owner hand check (8 Nov) | Passed in Edge: a 2.1 GB click went to Haul's Queue; with Haul closed, Edge kept the download. It found one bug: `.btn`'s `display` beat the `hidden` attribute, so the popup always showed Pair with Haul (fixed in `style.css`). Speed matched plain `curl` (~7 MB/s on the owner's line). |
| Plan 6a hand checks (8 Nov, dev build) | Toasts: Open folder opened Explorer; clicking a toast with Haul hidden in the tray brought it back on Finished (the button version shipped). Closing the window hid Haul to the tray and the one-time "still running" notice showed; the tray tooltip read "1 downloading" with the speed; Quit from the tray saved the running download as paused. A changed download folder took the next download. Autostart on wrote `HKCU\...\Run` `Haul` = `haul.exe --hidden`; off removed it. |
| Owner checks (8 Nov, after the merge, dev build) | Passed: every tray menu item (Open Haul, Pause all, Resume all, the Speed limit tick, the When done ticks in step with the status bar); one real Sleep from When done (log: `when done: carrying out mode=Sleep`; Haul was still running after the wake). Real-app smoke 6/6 after the fix pass, with the download folder at `_tmp\smoke`; real downloads and the files 7-Zip extracted carry `Zone.Identifier` ZoneId=3. Skipped by the owner (8 Nov): autostart after a real sign-in; check it on an installed 6b build instead (a dev build started that way has no Vite server, so its window is blank). Never a real shutdown unless the owner chooses. |
| Dev DB reset (8 Nov) | `%APPDATA%\dev.haul.desktop\haul.db` lost its recent contents on 8 Nov at 20:02 and is back to its ~14:35 state (schema 5, no downloads or settings, two Plan 5 test pairings). Most likely cause: a Python `sqlite3` connection opened read-write after Haul quit (to undo a test speed limit). Files on disk were not touched. Effects: the next launch migrates it to 6 and starts empty; **the download folder is back to the real Downloads**, so set it to `_tmp\smoke` in Settings before a smoke run; the Edge extension may need pairing again. Rule: change dev settings only through the app (Settings, or `invoke('set_settings', …)` over DevTools), never by writing the DB directly. |
| Review (8 Nov, Plan 6a) | A fresh reviewer: no Critical, 6 Important, 10 Minor; merge with fixes. **Fix pass done**, each fix test-first (RED then GREEN), full gate green: **I1** finished downloads get the Mark of the Web (`Zone.Identifier`: ZoneId=3, HostUrl, ReferrerUrl when known, no `user:password`), and 7-Zip runs with `-snz` so extracted files inherit it; **I2** a stale Finished row never recycles a newer same-path file, and `files_exist` gives the file to the newest row; **I3** Shut down now holds the queue (`Queue::hold`/`release`: pause and save, dispatcher kept), so a refused shutdown requeues the downloads and shows a "When done didn't work" toast; **I4** Delete files on a whole extracted package also recycles its extracted folder (never the download folder), and an archive recycled after extracting counts as present while its folder exists; **I6** a missing or read-only download folder fails with "Download folder … isn't available. Change it in Settings." I5 (Collector captures drop cookies) is deferred to 6b. Rulings: the ledger's `Final:` lines; the 10 deferred minors are listed below. |
| Plan 6a deferred minors | In-use check misses "in use by another process"; `when_done_step` reads all downloads every second even with Nothing; Pause all with Shut down chosen starts the countdown; a failed checksum run doesn't block When done; Do it now doesn't re-check for work started a moment earlier; the autostart switch can drift from the registry and a `--hidden` launch while open still shows the window; Settings number fields (empty becomes 1, 2.5 shows serde's text); no "<name> finished" toast with auto post-processing off; toast filtering keys on title wording; `core:window:allow-close` ships in the production capability. |
| Found 8 Nov: unquoted autostart path | `tauri-plugin-autostart` writes `HKCU\...\Run` `Haul` as `C:\tools\claude for project\target\debug\haul.exe --hidden`, with no quotes. Windows still finds the exe, but it first tries `C:\tools\claude.exe` and `C:\tools\claude for.exe` (the classic unquoted-path weakness: a program planted there would run at sign-in). Fix in 6b, test-first: write the Run value with the exe path in quotes (directly, or a plugin option if one exists), and check an installed build's path. |
| Next | Finish Plan 6b (sign-in check, uninstall check, review fixes, merge). After that: code signing, Firefox, the remaining 6a deferred minors. |

## The app, and how to run it
- **Real app:** `cd app` and `npx tauri dev`. The window is titled Haul, 1280×800 by default.
  - Since Plan 6b, dev runs use the identifier `dev.haul.desktop.dev`: the database is `%APPDATA%\dev.haul.desktop.dev\haul.db` and the logs are in `%LOCALAPPDATA%\dev.haul.desktop.dev\logs\`. The installed Haul uses `dev.haul.desktop` (the old dev folder). They can run side by side.
  - Closing the window pauses and saves every download, then quits.
- **Browser mock, no Rust needed:** `cd app`, then `npm run dev:mock`, then open `http://localhost:5174/?demo=1`.
  - `?demo=1` seeds the mockup's rows. Without it you get the empty state.
  - The mock simulates the engine: a new link takes about 6 s, and a URL containing `404` or `expired` fails the matching way.
- **Front-end checks, run in `app/`:** `npm test` (Vitest), `npm run check` (svelte-check) and `npm run e2e` (Playwright on the installed Edge).
- **Real-app smoke test.** Use three shells:
  1. `node scripts/range-server.mjs 8765`
  2. `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222 npx tauri dev`
  3. `npm run smoke` (four tests: a full download with a pause and a SHA-256 check, removing a running download, the clipboard watcher, and a split 7z package that downloads, extracts and sorts)
  - The smoke files go to the app's save folder. Point `download_dir` in the settings table at a scratch folder first, or they land in your Downloads.
- **Layout of `app/`:**
  - `src-tauri/`: the `haul-app` crate.
    - `core.rs`: the commands as plain methods.
    - `dto.rs`: what the UI sees.
    - `bridge.rs`: batches engine events into one `haul:update` every 250 ms and asks for a resync when it falls behind.
    - `commands.rs` and `lib.rs`: Tauri glue.
  - `src/lib/`: pure TypeScript, unit-tested: format, status, strip, model (list building, updates, moves and drops), speeds, keys and sparkline. It also holds `backend*.ts` (the interface, Tauri and the mock) and `app-state.svelte.ts` (all Queue state).
  - `src/components/`: the Svelte views.

## Plan 6b: installer, update check, Collector cookies, quoted autostart (8 Nov 2026)
- **Docs:** spec `docs/superpowers/specs/2026-11-08-haul-plan-6b-packaging-design.md`, plan `docs/superpowers/plans/2026-11-08-haul-plan-6b-packaging.md`, release steps `docs/release-checklist.md`. Every ruling is in the ledger `.superpowers/sdd/2026-11-08-haul-plan-6b-packaging/progress.md` (git-ignored).
- **Two data folders:** `tauri.conf.json` (dev, tests) has identifier `dev.haul.desktop.dev`; `tauri.release.conf.json` overrides it to `dev.haul.desktop` and drops the test-only `core:window:allow-close` capability (`capabilities/dev-test.json`).
- **Release builds only through `npm run release`** in `app/` (= `tauri build --config src-tauri/tauri.release.conf.json`). Quit the dev build first. The build first builds the extension. The first build downloaded NSIS 3.13 and `nsis_tauri_utils.dll` 0.5.3 into Tauri's tools folder.
- **Installer:** `target\release\bundle\nsis\Haul_0.1.0_x64-setup.exe` (6.92 MiB; SHA-256 of the 8 Nov build `0a3b9529…72d9`). Per-user (no admin), English, licence page `LICENSE-MIT`. Default folder `%LOCALAPPDATA%\Haul`; the owner chose `C:\tools\Haul`. Beside `haul.exe`: `extension\`, `7zip\`, `LICENSE-MIT`, `LICENSE-APACHE`. A real uninstall deletes the Run value `Haul` (Tauri's template and `app/src-tauri/installer-hooks.nsh`); Task Manager's mark is kept so a disabled startup stays disabled across an upgrade. Not signed: SmartScreen warns.
- **Autostart:** `app/src-tauri/src/autostart.rs` replaces `tauri-plugin-autostart` and writes `"<exe>" --hidden` with quotes. While the setting is on, each start writes the value for the current exe if it differs or is missing (an upgrade that uninstalls first deletes it). Dev builds use their own value name `Haul (dev)`, so they never touch the installed Haul's entry. A second launch with `--hidden` no longer shows the window. Turning the switch on clears Task Manager's "disabled" mark; the start-up repair never does.
- **I5 fixed:** a browser capture sent to the Collector keeps its cookies, DPAPI-sealed in `collector_links.headers_blob` (migration 7), used for the online check, Recheck and Start; never in a UI type.
- **Update check:** `app/src-tauri/src/update.rs`. 30 s after start, then hourly, it checks when the "Check for updates" setting is on and the last check (`last_update_check` setting) is 24 h old. One GET to `api.github.com/repos/meraaki410/haul/releases/latest` with `User-Agent: Haul/<version>`. A 404 (no release yet) is "up to date". A newer `vX.Y.Z` tag shows "Update available X.Y.Z" in the status bar; only `https://github.com/meraaki410/haul/releases/…` pages are opened. Settings › App has the switch and Check now. To publish a release: see the checklist.
- **Installed-build checks (8 Nov, driven through Windows UI Automation with `_tmp/haul-ui.ps1` in the owner's terminal):** installed data is `%APPDATA%\dev.haul.desktop` (log "Haul started db=…dev.haul.desktop\haul.db"); Open the extension folder opened `C:\tools\Haul\extension`; a split 7z from the range server extracted with the bundled 7-Zip; the Start menu shortcut's AUMID is `dev.haul.desktop` (toasts say Haul); Check now said "You have the latest version (0.1.0)"; Start with Windows wrote `"C:\tools\Haul\haul.exe" --hidden`. **Open:** sign out/in (Haul should be in the tray, window hidden; check the process command line has `--hidden`) and the uninstall (Start menu entry and Run value gone).
- **Environment lesson:** the Bash tool in these sessions sees a stale, sandboxed copy of `%APPDATA%` and `%LOCALAPPDATA%`. To see what the real app wrote, run the command in the owner's Terminal panel.
- **What remains after 6b:** code signing, Firefox, an extension store listing, the Plan 6a deferred minors (the autostart-drift and `--hidden` ones are fixed by 6b; `allow-close` is now dev-only).

## Plan 5: local API, pairing and the browser extension (8 Nov 2026)
- **What happens:**
  - Haul runs a small server on `127.0.0.1:17654` (the next free port up to 17664 if busy; the port is written to `%APPDATA%\dev.haul.desktop\api-port`).
  - The Chrome/Edge extension in `extension/` pairs once (Allow/Deny in Haul's window), then hands clicked downloads to Haul, which start at once. "Download all links…" sends a page's links to the Collector, marked Browser.
  - If Haul is closed, not paired, or doesn't answer within 1.5 s, the browser keeps the download. The browser's copy is cancelled only after Haul said yes.
- **Owner decisions:** Chrome and Edge only (Firefox later); a clicked download starts at once; approach A (local HTTP server).
- **Where the code is:**
  - Engine: migration 5 (`downloads.headers_blob`, `paired_clients`), `store/clients.rs`; headers are DPAPI-sealed on write and wiped on Completed and Failed; 401 is Link expired; `refresh_headers`, `find_by_url`.
  - App: `capture.rs` (`capture` with a 5 s duplicate window, and re-capture that refreshes an expired or paused download; `add_browser_links`; pairing wrappers); `api/guard.rs` (the three locks), `api/pairing.rs` (`PairGate`, numbered requests, 60 s), `api/server.rs` (axum; every route is a POST); commands `pair_answer(n, allow)`, `paired_clients`, `remove_paired_client`; events `haul:pair-request {n, browser}` and `haul:pair-closed {n}`.
  - UI: `PairDialog.svelte`, `BrowserList.svelte` (a Browsers section in Rules), `lib/browsers.ts`.
  - Extension: `decide.ts` (catch rule), `port.ts`, `net.ts`, `api.ts` (client), `handover.ts`, `pairstate.ts`, `store.ts`, `picker.ts`; entries `background.ts`, `content.ts`, `popup.ts`, `options.ts`, `picker-page.ts`; pages in `static/`.
- **The locks**, in order: `Host` must be exactly `127.0.0.1:<port>` (421); `Origin` must be `chrome-extension://<id>` (403); a Bearer key whose SHA-256 is stored and bound to that origin (401; not needed for pair and status). No CORS headers. Body ≤ 2 MiB, ≤ 5,000 links (413).
- **Load the extension by hand:**
  1. `cd extension`, `npm install` (once), `npm run build`.
  2. In Edge: `edge://extensions`, turn on Developer mode, Load unpacked, pick `extension\dist`.
  3. Start Haul, click the Haul toolbar button, Pair with Haul, then Allow in Haul.
- **Hand check (passed 8 Nov):** with the extension paired, click a real download of 1 MB or more on a normal site. It should appear in Haul's Queue, complete, and vanish from Edge's download list. With Haul closed, the same click should download in Edge as usual.
- **Bugs found while building, each fixed with a test that failed first:**
  - Edge sends no `Origin` on an extension's GET, so `GET /v1/status` was refused and the popup said Haul wasn't running. Status is now a POST (GET gets 405). Found by the real-browser test.
  - Review: a stale pairing dialog could approve a newer request. Answers now carry the request number, and Haul closes the dialog when a request ends.
  - Review: a download with no final URL was caught. It's now skipped, as the spec says.
  - Review: a "waiting" pair state saved by a stopped worker kept Pair disabled for good. It now expires after 70 s.
- **Rulings** (cost if wrong in each):
  - Capture code lives in `capture.rs` (`impl AppCore`, fields `pub(crate)`), because `core.rs` was 718 lines. Cost: a file move.
  - The catch rule runs before the pause, so downloads Haul won't take are never touched. Cost: a few ms of browser download first.
  - A timed-out capture doesn't rescan (Haul was there, just slow); a refused connection rescans once. Cost: none.
  - "Download all links…" sends no cookies. Cost: links from login-only sites show Offline in the Collector.
  - Incognito downloads are never caught. Cost: one switch if wanted.
  - Alt works through a recorded Alt+click (2 s). Cost: downloads a site starts by itself can't be skipped with Alt.
  - Form-POST downloads: the spec's "no final URL" rule is followed, but Chrome often fills the final URL for POSTs too, so one can still be caught and then fail in Haul. Cost: start it again holding Alt. A real fix needs a spec change (track request methods, or Haul probes before answering).
  - The Browsers list lives in the Rules view until a Settings page exists. Cost: it moves in plan 6.
  - esbuild 0.28 (vitest's vite needs it), `@types/chrome` 0.3. Icons are the app's 32/64/128 PNGs. Cost: a soft 16 px toolbar icon.
  - The Task 1 cookie test downloads 1 MiB, not 4: 4 MiB starved the timing tests in the same file. Cost: none.
  - The real-browser test drives Edge over raw DevTools, because Playwright takes over downloads (even when only attached) and Edge then fails them. Cost: a small CDP helper in the test.
  - Tests named in the plan whose failing run I did not see: the e2e pairing tests (their mock calls failed first instead), `dialog_closes_when_request_ends`, and the Rust `status_by_get_is_refused` (the running app locked `haul.exe`). Cost: weaker evidence for those only.
- **Deferred minors (from the review):**
  - The pause also waits for the cookie read, so more tiny files finish in the browser first (harmless).
  - Pairing identity is only the name the extension sends; control characters aren't stripped (log lines); re-pairing the same extension adds a row and keeps the old key.
  - No test runs migration 5 on a schema-4 DB with plain-text headers; old plain text may linger in SQLite free pages (consider `secure_delete` or a VACUUM).
  - `EngineError::Network` keeps reqwest's full URL, including any `user:password` (today only in debug logs and the row's error).
  - The extension rescans only on a dead port, not when another program answers there.
  - A stray run of spaces in one store SQL string; the `api-port` file is never removed.
  - A capture that took over 1.5 s but was queued downloads twice (browser and Haul): the accepted never-lose trade-off.
- **Accepted for v1 (outside approach A's threat model):** another Windows user's program squatting the port while Haul is closed; same-user local programs forging Host/Origin or spamming pair requests; Strict/HttpOnly cookies sent with page-started downloads; the key in `chrome.storage.local`; Chrome's "ask where to save" setting is ignored for caught downloads.
- **Flaky under load:** `resume::crash_simulation`, `queue::restart_resumes_interrupted` (simulated-crash timing) each failed once in a full run while other work loaded the PC; both pass alone (3/3). Not caused by plan 5.
- **Testing while the app runs:** `tauri dev` locks `target\debug\haul.exe`, so run Rust tests with `CARGO_TARGET_DIR=_tmp/target-alt`.
- **Real-browser test:** `npx playwright test -c playwright.real.config.ts extension` (needs the range server, `tauri dev` with port 9222, and the built extension). In a normal, unsandboxed session it should run fully; here only the pairing half could.

## Plan 4: post-processing and Rules (6 Nov 2026)
- **What happens:** when every download in a package (or a lone download) is complete, a **run** starts: verify → extract → sort → scan → notify. One run at a time; every step's state is saved, so a restart carries on from the first unfinished step. The drawer's "After download" list shows the steps; package rows say Verifying / Extracting / Sorting / Needs password / Needs attention.
- **Owner decisions:** bundle 7-Zip (WinRAR is paid and RAR-only on the command line; the owner's `Rar.exe` only made test files); auto-process on; archives kept after extracting (a switch recycles them); sort into category folders on.
- **Where the code is:**
  - Engine: `rules.rs` (pure: categories, rule matching, rename tokens), `postproc/` (`checksum.rs`, `extract.rs` with `SevenZip`, `sort.rs` moves that never overwrite + Recycle Bin, `scan.rs` Defender, `run.rs` types, `mod.rs` the `PostProc` service), `secret.rs` (DPAPI), `store/postproc.rs` + `migration_4.sql`.
  - App: commands `postproc_runs`, `postproc_process`, `postproc_retry`, `postproc_password`, `set_expected_hash`, `rules_config`, `set_postproc_settings`, `set_categories`, `set_rules`, `try_rules`, `add_password`, `remove_password`, `order_passwords`; `haul:update` has `postproc: true` when a run changed (the UI reloads runs and the snapshot); notifications via `tauri-plugin-notification`.
  - UI: `lib/postproc.ts`, `postproc-state.svelte.ts`, `rules.ts`, `rules-state.svelte.ts`, `backend-mock-postproc.ts`; `AfterDownload`, `RulesView`, `RuleEditor`, `CategoryTable`, `PasswordList`.
- **7-Zip:** 26.05 x64 in `app/src-tauri/resources/7zip/` (`7z.exe`, `7z.dll`, `License.txt`, README with SHA-256s), shipped as a Tauri resource; `tauri dev` falls back to the source-tree copy. Test archives: `crates/engine/tests/fixtures/` (`make-fixtures.ps1` rebuilds them with the bundled 7z and WinRAR's `Rar.exe`).
- **Settings keys:** `postproc` (`{auto, sortCategories, recycleArchives, virusScan}`, default on/on/off/off) and `categories` (the six defaults until edited). Rules live in the `rules` table, passwords (DPAPI blobs) in `passwords`; the UI only ever sees password ids.
- **Database:** schema version 4 (runs, passwords, rules; downloads gain `expected_hash`, `server_digest`, `verify_retries`). The dev DB was moved to 4 by the smoke run.
- **Rulings** (cost if wrong in each):
  - A switched-off scan shows Skipped "Off", not Done. Cost: one icon.
  - An extracted folder keeps its own name; rename patterns apply to files. Cost: a rule can't rename an extracted folder.
  - Checksum files move with the package's archives. Cost: none.
  - A "needs attention" notification is sent each time a run stops failed or needing a password. Cost: an extra toast per failed retry.
  - Notifications are sent from Rust, so the window has no notification permission. Cost: one capability line if the UI ever notifies.
  - The Expected checksum field starts empty (the saved hash isn't sent to the UI). Cost: a saved hash can't be seen.
  - "Try a file name" previews the saved rules, not an unsaved edit. Cost: save before trying.
  - `oldstyle.rar/.r00/.r01` fixtures are RAR5 volumes renamed (RAR 7 can't write RAR4). Cost: a real RAR4 old-style set is untested.
  - The four switches live in the Rules view until a Settings page exists (plan 7).
- **Bugs found by the new smoke test, each fixed with a failing test first:**
  - Downloading the same archive set again joined the first, finished package, so it was never post-processed (`same_set_again_gets_a_new_package`). A late missing part still joins its siblings.
  - A name clash turned `split.7z.001` into `split.7z (1).001`, which is no longer a volume of any set; it is now `split (1).7z.001` (`numbered_archive_volumes_stay_in_their_set`).
- **Smoke folder:** `_tmp/smoke` holds leftovers from those failing runs (`split.7z (n).00N` in its root). Scratch, safe to clear by hand.
- **Review:** a self-review only (the session could not start a fresh reviewer unasked); the owner merged on that basis. Deferred minors: a run that failed for a missing part needs Retry once the part arrives; the archive password reaches 7-Zip on its command line; downloads finished before plan 4 show Process instead of a run; with auto off, finished rows say "Waiting to process". The executor ledger was deleted after the merge; this section and the commit messages are the record.
- **Not checked by eye:** the Rules view was covered by e2e and axe in both themes, but not screenshotted (the browser pane was hidden). The drawer's steps were screenshotted in the mock.

## Plan 3: the Collector (6 Nov 2026)
- **How links flow:** clipboard (while Watch clipboard is on), Ctrl+V, drop and the Queue's Add links box all send links to the Collector (`add_links(text, source)`). Each link is checked online, 8 at a time, 20 s timeout. "Start package", a lone row's "Start" or "Start all online" moves links to the Queue.
- **Where the code is:**
  - Engine: `crates/engine/src/grabber/` (`grouping.rs` archive parts and names, `dupes.rs` duplicates, `mod.rs` the `Grabber`) and `store/collector.rs` + `store/migration_2.sql` (tables `collector_batches`, `collector_links`, `collector_groups`).
  - App: commands `collector_snapshot`, `collector_start`, `collector_remove`, `collector_recheck`, `set_collector_filters`, `set_watch_clipboard`; `clipboard.rs` (filter + watcher thread); `haul:update` has `collector: true` when the Collector changed.
  - UI: `src/lib/collector.ts` (pure), `collector-state.svelte.ts`, `CollectorView`/`CollectorRow`/`FilterChips`; paste and drop live in `App.svelte`.
- **Settings keys:** `watch_clipboard` (bool, default off) and `collector_filters` (`{html, images, small, duplicates, exts}`, default html + images hidden).
- **Collector packages are not stored:** they are worked out from the batch and the current file names on every read, so a name learned by the check can move a link into an archive set. A real `packages` row is made on first start and remembered in `collector_groups`.
- **Rulings** (owner decision first, then calls made on the owner's behalf; cost if wrong in each):
  - **Watch clipboard starts off** (owner: "you tell me"): with it on, every copied link anywhere gets a request from Haul. Cost: one click to turn it on.
  - Duplicates are worked out on read, not stored as `duplicate_of`; an exact URL already waiting is skipped, not flagged. Cost: none.
  - The Grabber keeps a Tokio handle so the clipboard thread can add links. Cost: none.
  - `arboard` without its image feature. Cost: re-enable later if images are needed.
  - The mock groups only `.partN.rar`/`.7z.NNN` and has no prefix names; the engine has the real rules. Cost: e2e can't see prefix names (engine tests cover them).
  - A user file-type chip shows pressed; clicking it removes it. Cost: no way to keep an unpressed custom chip.
  - The Collector list is a treegrid like the Queue. Cost: an ARIA role swap.
  - `note_own_write` has no caller yet (Haul copies nothing). Cost: none until a Copy action exists.
- **Bugs found while building, each fixed with a failing test first:**
  - A filter or switch change could be undone by a Collector snapshot already in flight (`filter_change_beats_snapshot_in_flight`).
  - Chip toggles sent the reactive proxy; the mock could not copy it and silently rolled back (caught by e2e `filter_chip_hides_and_persists`).
  - **Watch clipboard broke copying in other apps:** reading the clipboard the instant it changed made multi-step copies (.NET/OLE apps, `Set-Clipboard`) fail. The watcher now waits 150 ms before reading (caught by the real-app clipboard smoke test, which failed 3 of 3 before and passes 3 of 3 after).
- **Smoke server:** `scripts/range-server.mjs` now serves any `/<name>.bin` path (the same 64 MiB file).
- **Database:** schema version 3. Migration 3 adds `collector_archives` (the Queue package an archive set became). The dev DB in `%APPDATA%\dev.haul.desktop` is already at version 2 from the smoke run; the app moves it to 3 on next launch.

## Plan 3 review (6 Nov 2026)
A fresh reviewer read the whole branch: no Critical findings, 4 Important, 11 Minor. Each fix below has a test that failed first.
- **Two starts at once queued every link twice** (double click, held Enter). The `Grabber` now makes intake, start, remove and recheck take turns, and the UI ignores a second start while one is running and held-key repeats of Enter. Tests: `concurrent_starts_queue_each_link_once`, `double_start_calls_backend_once`.
- **Two pastes at once added the same links twice.** Same lock; a batch's links also insert in one transaction (a 500-link paste is now one commit). Test: `concurrent_intakes_add_each_link_once`.
- **Archive parts copied one at a time never formed one package.** Archive sets now group across every waiting link, and a part that arrives after its siblings were started joins their Queue package (while that package still has downloads). Tests: `archive_parts_from_separate_intakes_join`, `late_part_joins_started_package`, `archive_package_needs_live_downloads`.
- **No mouse way to remove a link.** Every Collector row and package has an × Remove button (with Undo). Test: e2e `remove_button_removes_with_undo`.
- **Copying the same link again later was ignored for ever** (a reviewer Minor, re-graded Important because the main clipboard flow failed silently). Only a repeat within 2 s is ignored now. Test: `filter_accepts_same_text_again_later`.

Deferred minors from the review, each a small follow-up:
- Links removed from the Collector still get their queued probe (up to 8 at a time, 20 s each).
- A recheck pressed while a check is running can let the older result win.
- A non-archive package started one link at a time can split in the Queue (the last link becomes a lone link).
- If `queue.add` fails partway through a start, the links already queued stay in the Collector too.
- A package's key changes when a check learns the real file name, which drops its selection and collapsed state.
- Enter in a future `<select>` or contentEditable starts the selection (`CollectorView` checks only INPUT/TEXTAREA).
- Deleting an expanded package leaves nothing selected instead of the next package.
- The watcher does not honour password managers' "exclude from clipboard monitoring" formats.
- The 1 MB clipboard limit is checked after `arboard` has read the whole text.
- A failed `collector_remove` leaves the links hidden until restart, with no message.
- Plus the plan 2 minors still open (error toasts, startup dialog, and the rest listed below).

## Plan 2 rulings (decisions made on the owner's behalf)
Each line says what it costs if the decision is wrong.
- **Contrast** (from the plan): `--ink-3` is `#6B6559` light and `#8F8B82` dark. Status words are `--ink-2`, with the colour on the icon. `DESIGN.md` is updated. The a11y test fails on the old colours. Cost: labels a little darker than the mockup.
- **Pasted links queue directly**: replaced in plan 3; every link now goes through the Collector.
- **TypeScript is pinned to 5.** npm picked 7.0, the native port, which svelte-check can't use yet. Cost: an upgrade later.
- **Columns are narrower than the mockup's.** The mockup's minimum widths add up to 870px, but the list area is only 732px at the default window size, so Status was cut off. Below about 1180px the list scrolls sideways inside itself. Cost: a narrower name column.
- **Sparkline:** the plan's formula contradicted its own test, so the test won: the peak sits 10% below the top. Cost: none.
- **Reorder rules** (`planMove`/`planDrop` in `model.ts`):
  - A package moves as a block, and a file inside a package moves among its siblings.
  - A dropped row takes the target's place.
  - A moved row takes its new neighbour's priority.
  - A file dragged out of a package stays in its package.
  - Cost: a drag may land one row off what someone expects.
- **Remove:** Remove waits 6 s for Undo. A removal still pending when the app closes is never sent, so the row survives. Cost: the user deletes it again.
- **Native drag-and-drop is off** (`dragDropEnabled: false`) so HTML5 drops of links and `.txt` files reach the page. Cost: none until something needs native file drops.
- **Fixes from the smoke run, each test-first:**
  - Finished rows now reload a snapshot, so they learn their saved path.
  - Finished rows show the name on disk, e.g. `file (3).bin`.
- **Smoke server throttle:** 1 MB/s per response, so a test can pause at 30%. Cost: none.
- **Smaller calls:**
  - The snapshot hides packages that have no downloads left.
  - The demo seed adds a running `.msi` row.
  - The speed-limit label reads "10 MB/s".
  - Speed history gets one sample per update.
  - The drawer has a Remove button.
  - The executor ledger was deleted after the review, as the workflow says; this section and the commit messages are the record.

## Plan 2 review (5 Nov 2026)
A fresh reviewer read the whole branch and found no Critical issues and 4 Important ones. Each fix has a test that failed first.
- **Removing a running download left a ghost "Paused" row.** The engine now sends `Event::Removed` after the delete, and the UI keeps the row hidden until a snapshot drops it.
- **Updates that arrived during a snapshot load were lost.** Only the newest load now applies, the updates seen during a load are replayed onto it, and resync requests are coalesced.
- **Commands ran on the main thread.** They are now async and do their store work on a blocking thread. A snapshot also loads all segments in one query.
- **Markdown `[url](url)` and comma-joined links** produced bogus downloads. `extract_links` now splits them.

Deferred minors, each a small follow-up:
- **Error handling:**
  - Errors in remove, reorder and add get no toast or rollback, and the same goes for a failed speed-limit change.
  - A startup failure panics with no dialog.
- **Small display and state bugs:**
  - Rows waiting in the Undo window still count toward the total speed.
  - `lastSteal` is never pruned.
  - A queued file whose size is known to be 0 bytes shows 100%.
- **Capability:** it grants `opener:default`, which the UI doesn't use; drop it along with the JS package.
- **Data folder:** dev and release builds share the `dev.haul.desktop` data folder.

## How to build and test
The toolchain is already installed on this PC:
- Rust stable (MSVC).
- VS 2022 Build Tools with the C++ workload and the Windows 11 SDK 10.0.26100.

New shells may not have cargo on PATH:
```bash
export PATH="$HOME/.cargo/bin:$PATH"
```
Run the Rust tests for the engine and the app (about 20 s):
```bash
cargo test --workspace
```
Run clippy:
```bash
cargo clippy --workspace --all-targets -- -D warnings
```
Download a real file with the CLI example:
```bash
cargo run -p haul-engine --release --example haul_get -- https://sin-speed.hetzner.com/100MB.bin --out _tmp/dl
```
- The CLI's flags are `--conns N`, `--out DIR` and `--limit BYTES_PER_SEC`.
- Rerunning the same command resumes, using `<out>/.haul-cli.db`.
- Put scratch output in `_tmp/` (git-ignored).

## The engine, in one page (`crates/engine`)
| File | Job |
|---|---|
| `types.rs`, `error.rs`, `events.rs` | Ids, `DownloadState`, `Priority`, `ProbeInfo` and `Event`; `EngineError` with `is_retryable()` and the status mapping; and `EventBus` (a broadcast with capacity 1024). |
| `naming.rs` | Safe Windows names. Name sources: `filename*` first, then `filename`, then the URL's last path segment. `reserve_unique_path` atomically picks `name (n).ext`. |
| `segmenter.rs` | Pure split-and-steal maths: `initial_plan` and `pick_steal`. |
| `store/` | SQLite (WAL) with `schema.sql` as migration 1. Downloads, segments, packages and settings. |
| `probe.rs` | `http_client()` and `probe()`: HEAD, then a fallback GET with `Range: bytes=0-0`. |
| `ratelimit.rs` | `RateLimiter` (a token bucket with burst = rate / 4) and `HostLimiter` (at most 16 connections per host). |
| `transfer/` | `run_transfer`: probe, resume or plan, run the connections, save every 2 s, finish. See the details below. |
| `queue.rs` | `Queue`: priority order, a parallel limit of 1–20, pause/resume/remove, crash recovery on start, `AllDone`, and the schedule. |
| `schedule.rs` | `Schedule` (overnight windows count from their start day), `is_within_window`, and `WhenDone`, which is stored only. |
| `examples/haul_get.rs` | The CLI used for real-world checks. |
| `tests/support/mod.rs` | A local axum server that can throttle, drop, ignore ranges, force statuses, send 429s, hide the length and redirect. |

How `transfer/` works:
- **`state.rs`**: the shared range table.
- **`conn.rs`**: one connection at a time. It claims a range or steals half of the biggest one, GETs it, buffers 256 KiB, writes with `seek_write`, and only then advances `pos`, which makes it crash-safe.
- **Retries:** a connection retries with backoff. On a 429 it sheds itself while other connections remain, and the last one retries.
- **`mod.rs`**: runs the passes and the slow-connection monitor.
- **Fallbacks:** if the server ignores ranges, it falls back to one sequential connection. A 416 restarts once.
- **Resume:** before resuming it checks the fingerprint (ETag, then Last-Modified, then size).

## Plan 1 rulings (engine)
Each line says what it costs if the decision is wrong.
- **reqwest TLS:** uses `rustls-no-provider` plus `rustls` with **ring**, not the default aws-lc, so the build needs no cmake or NASM. Cost: none; switching back is a feature flag.
- **Part file name:** `<name>.<id>.haulpart`, not the spec's `<name>.haulpart`, so two same-name downloads can't collide. Cost: none.
- **Engine location:** the engine lives in `crates/engine`, not inside `app/src-tauri`. Cost: none; plan 2 adds the Tauri app as a second workspace member.
- **Unique names:** `reserve_unique_path` creates an empty placeholder, so two same-name downloads finishing together never pick the same name. Cost: one extra helper.
- **Range alignment:** ranges are rounded down to 64 KiB multiples. The plan's own tests needed it. Cost: the last range can be slightly larger.
- **Probe fallback:** the probe falls back to the ranged GET on *any* HEAD failure or missing Accept-Ranges, because signed S3-style links reject HEAD. Cost: one extra request per probe.
- **Pinned file name:** the first probe's file name is stored (`Store::set_file_name`), so later probes can't orphan the part file. Cost: none.
- **Retry timing in TDD:** per-connection retry was built in Task 6, ahead of Task 7, so Task 7's retry tests never failed first. Cost: weaker evidence for those tests only.
- **Test tuning:** the slow, steal, expiry and 416 tests were tuned so they actually exercise the behaviour and finish in seconds, not 8+ s. Cost: none.
- **Range-ignored memory:** after a range-ignored fallback, the stored probe still says resumable, so the next resume detects it again. Cost: one wasted request.
- **Real-world test file:** the plan's `proof.ovh.net/files/1Gb.dat` is really 1 GiB, and that host rate-limits with 429 and then blocks the IP. Hetzner's Singapore 100 MB file is used instead. Cost: none.
- **New fix, 429 handling:** a 429 is retryable and sheds extra connections. The real download found this bug, and the tests went RED then GREEN. Cost: hosts that use 429 as a hard limit get retried up to 10 times.
- **Kill-and-resume check:** it was done with a hard kill, not Ctrl+C, because Git Bash can't send Ctrl+C to a native exe. Cost: the CLI's Ctrl+C path hasn't been tried by hand.
- **Formatting:** added `rustfmt.toml` (max width 120). Cost: none.

## Known minors, deferred (plan 1)
- **`AllDone` fires too often:** it also fires when the user pauses every download, so plan 6 ("when done") must check states before acting on it.
- **Stalls under a speed limit:** the stall rule (no bytes for `slow_window` → reconnect) is off while a speed limit is set, so a stall there still waits for the 30 s read timeout.
- **Crash during finish:** a crash between the final rename and `set_completed` re-downloads the file as `name (1)`. Fix: store `target_path` before the rename and check it on recovery.
- **Schedule vs. manual pause:** when a schedule window opens, it re-queues every download it auto-paused, even one the user has since decided to keep paused. This needs an owner decision. It was raised with them and is still open; settle it with the schedule settings in plan 6.

## Plan 1 post-review fixes (5 Nov 2026)
`/code-review` (high) ran on the branch. Fixed, each with a test that failed first except the 429 race:
- A changed file could resume onto stale bytes after an early failure.
- Resume used too few connections.
- The free-space check ignored the already-sized part file.
- Disk full is now a single Paused state.
- Pause/remove could lose a race with the dispatcher (`Store::start_if_queued`).
- A 429 shed could orphan a range.
- The stall rule.
- `.gitattributes`.
- The unused `unique_path` was removed.

Details and rulings are in `.superpowers/sdd/2026-11-05-haul-plan-1-engine-core/progress.md`. There are now 87 tests.

## Working agreements (read before continuing)
- **Write the code yourself.** Don't delegate to the codex/deepseek/opencode workers, even though `C:\Users\User\.claude\deepseek-on` exists. The owner asked for this explicitly.
- **Workflow:** superpowers, in the order spec → writing-plans → executing-plans with TDD. Watch each test fail before writing the code.
  - The ledger lives in `.superpowers/sdd/<plan>/progress.md` (git-ignored). Record each deviation in it as a `Ruling:` line.
- **The owner is new to coding.** Explain choices plainly, make reasonable calls, and flag only real decisions.
- **Never destroy user data.** Deletion goes to the Recycle Bin. Ask before installing software or doing anything destructive.
- **Folder:** the project is `C:\tools\claude for project`. The separate `C:\tools\claude` folder is an unrelated Roblox project.
- **Scope:** Haul is a general-purpose download manager. Don't build anything aimed at piracy sites or their file hosts.

## Environment notes
- **Bash heredocs:** they mangle quotes and backslashes on this machine. Write files with the Write tool, or put edit scripts in the scratchpad and run them with `python -I`.
- **Mockup preview:** `.claude/launch.json` has a python `http.server` config on port 5178.
- **Mock app preview:** `.claude/launch.json` also has `app-mock` (`npm --prefix app run dev:mock`, port 5174), for the browser pane.
- **The PowerShell tool is sandboxed:** it sees a different `%APPDATA%` and can't reach the app's windows. Use Bash (or `powershell` called from Bash) for anything touching the real app data, clipboard or windows.
- **An app started in the background from Bash can't be closed with window messages:** its window isn't visible to the tools, although DevTools on port 9222 works. Close it by hand, or use `taskkill` (crash-like, test data only).
