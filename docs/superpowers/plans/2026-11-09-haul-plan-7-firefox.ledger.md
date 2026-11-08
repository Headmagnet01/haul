# SDD ledger — plan: docs/superpowers/plans/2026-11-09-haul-plan-7-firefox.md
Spec: docs/superpowers/specs/2026-11-09-haul-plan-7-firefox-design.md. Branch feat/firefox in worktree .claude/worktrees/plan7, from master e8f33eb.
Pre-flight: T1→T2 dist-firefox consumed by probe: consistent. T2→T8 Firefox/Tab/EXT_UUID: consistent. T1→T5 dist-firefox for signing: consistent. T4 browserName async used only in background.ts: consistent. No conflicts.
Task 1: complete (commits e8f33eb..8fc5a1b, tests: npm --prefix extension test → 37/37 pass, check + build OK)
Task 2: Origin probe (Firefox 159.0.1, extension page fetch, POST to 127.0.0.1:17699): Origin "moz-extension://6c1d9f2e-1b7a-4e3c-9a55-0d2f7e8b4c31", Host "127.0.0.1:17699" exact. GET carries no Origin (same as Edge; status is already a POST). Carry on.
Task 2: Ruling: BiDi refuses to navigate to moz-extension: and to evaluate in add-on pages — the helper makes a test-only copy whose pages are web_accessible, lets an http page navigate itself there (Tab.openAddonPage), and starts Firefox with MOZ_REMOTE_ALLOW_SYSTEM_ACCESS=1 (+ --remote-allow-system-access) — the shipped add-on is untouched — cost if wrong: test-only differences in the copy (web_accessible_resources) could hide a bug that only shows without them; none known.
Task 2: complete (commits 8fc5a1b..0d27cba, tests: _tmp/origin-probe.ts against Firefox 159 → Origin present)
Task 3: note: capture::tests::collector_capture_starts_with_cookies failed once in a full run (file NotFound after Completed); passed 3/3 alone and in the next full run; not touched by this plan, a flake.
Task 3: complete (commit 87371f1, tests: cargo test -p haul-app --no-fail-fast -> 144 lib + 21 api + 1 + 8 config pass; clippy clean; api firefox RED seen by disabling the branch)
Task 4: Ruling: the right-click "Download with Haul" also passes the tab cookieStoreId and page URL as referrer, so it gets the same Firefox cookie lookup as caught downloads - the plan only named the download path - cost if wrong: none (Chrome tabs have no cookieStoreId).
Task 4: Ruling: a replaced cookie keeps its first position in the header (Map overwrite) - order is meaningless to servers - cost: none.
Task 4: complete (tests: npm --prefix extension test -> 52/52, check + build OK)
Task 5: Ruling: the API key reaches web-ext through its own WEB_EXT_API_KEY/WEB_EXT_API_SECRET env vars, not --api-key/--api-secret arguments (the plan said arguments) - keeps the secret off command lines and process lists - cost: none.
Task 5: Ruling: added xpi_without_version_fails (an .xpi with no version file) - cost: none.
Task 5: Ruling: beforeBundleCommand lives in the base tauri.conf.json, so a local debug bundle also needs a signed add-on - matches the spec (installer always ships it) - cost if wrong: an unsigned local bundle needs the check bypassed by hand.
Task 5: complete (tests: extension 56/56; cargo test --test config 10/10; sign without keys prints the key message; check-release exits 1 with no xpi)
Task 6: Ruling: Settings has no toast, so BrowserList shows a folder error inline (role=status) under the two buttons, plus one help line for both browsers - cost: a line of text.
Task 6: Ruling: the mock openFirefoxFolder always rejects with the missing-add-on error (a dev checkout before signing) - cost: none.
Task 6: note: collector_capture_starts_with_cookies failed again in a full run (2 of 3); likely post-processing moves the file after Completed before the test reads target_path; pre-existing (6b), not in this plan - follow-up.
Task 6: complete (tests: playwright 174/174, svelte-check 0 errors, vitest 111/111, cargo test -p haul-app all but the known flake, clippy clean)
Task 7: note: hand_remove_forgets_auto_pause passed before the fix (a removed row cannot be re-queued: the requeue checks the row is Paused) - kept as a guard; hand_pause_beats_schedule RED (a Running) then GREEN.
Task 7: complete (tests: cargo test -p haul-engine --no-fail-fast all pass; queue 15/15 three times; clippy clean)
Task 8: finding: Firefox fires downloads.onCreated with totalBytes -1 (size unknown), so big unlisted-type downloads were never caught. Fixed test-first: needsSize + waitForSize (needs_size_when_only_size_decides, size.test.ts RED->GREEN), background asks downloads.search up to 15 x 100 ms when only the size decides.
Task 8: Ruling: wait up to 1.5 s for the size whenever the unknown size alone decides, in every browser (Chrome too, for servers with no length) - the browser keeps downloading meanwhile; Haul pauses it after - cost if wrong: up to 1.5 s of browser download before the hand-over, and one extra search per such download.
Task 8: finding: under BiDi, Firefox 159 never requests a navigation whose path ends in .bin or .zip (same URL with a query goes through); the test adds ?t=<stamp>. Not seen outside the driver; the owner hand check covers real use.
Task 8: Ruling: since 6a a completed download leaves Queue for Finished, so the Firefox test checks Finished instead of Complete in Queue - cost: none. The Edge real test (extension.spec.ts) still checks Complete in Queue and is likely stale the same way; not changed (not in this plan).
Task 8: note: the Edge real test could not run: with the installed Haul on 17654, Edge paired with it (its dialog went to the installed app, unanswered = Deny). Needs the installed Haul closed. Chromium path covered by unit tests only.
Task 8: complete (tests: playwright real firefox 3/3 against tauri dev; extension 62/62)
Final review: fresh reviewer (opus), verdict ready with fixes; 0 Critical, 2 Important, 8 Minor.
Final: fixed I1 sign.mjs split paths with spaces (shell: true) - sign_command_keeps_paths_with_spaces_whole RED (module had no exports / ran on import) -> GREEN; children now node <script>, no shell; suite 66/66.
Final: fixed I2 no data_collection_permissions - firefox_declares_no_data_collection RED->GREEN; web-ext lint 0/0/0; Firefox 159 installs it (origin probe); suite 66/66.
Final: minor (deferred): hand pause right as the window closes can be re-added to auto_paused by a dispatch tick while the cancelled transfer is still in running (fix: skip cancelled entries).
Final: minor (deferred): the size wait uses only the size; filename/state/Alt-clicks are from onCreated, so an Alt-click landing during the 1.5 s wait is missed.
Final: minor (deferred): servers slower than 1.5 s to send headers leave big Firefox downloads in Firefox with no message.
Final: minor (deferred): partitioned cookie wins by name, which can send an iframe-partition session cookie for a top-level link; privacy.firstparty.isolate makes getAll throw (no cookies).
Final: minor (deferred): check-release cannot tell a signed .xpi built from older code with the same version.
Final: minor (deferred): a failed sign can leave extension/firefox/.signed, which the installer would bundle.
Final: minor (deferred): sign.mjs prints web-ext output only at the end (silent while Mozilla reviews).
Final: minor (deferred): the open-folder error tells installed users to run npm run sign:firefox.
