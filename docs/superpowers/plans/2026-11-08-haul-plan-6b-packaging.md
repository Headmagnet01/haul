# Haul Plan 6b: installer, update check, Collector cookies, quoted autostart — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. In this repo the owner asked for executing-plans, with the code written in-session (no worker delegation).

**Goal:** Make Haul a per-user installed Windows app with its extension and 7-Zip bundled, a quoted Start with Windows entry, cookies kept on Collector captures, and a daily GitHub update check.

**Architecture:** Dev and release builds get separate identifiers through a Tauri config overlay. A new `autostart.rs` replaces `tauri-plugin-autostart` and writes the Run value itself. The engine's Collector gains a sealed headers column that the prober and Start use. A new `update.rs` asks GitHub's latest-release API and reports through a command and an event. NSIS bundles everything.

**Tech Stack:** Rust (Tauri 2, rusqlite, reqwest, windows-sys, axum test servers), Svelte 5 + TypeScript (Vitest, Playwright), Tauri NSIS bundler.

**Spec:** `docs/superpowers/specs/2026-11-08-haul-plan-6b-packaging-design.md`

## Global Constraints

- Dev identifier `dev.haul.desktop.dev` (base `tauri.conf.json`); release identifier `dev.haul.desktop` (`tauri.release.conf.json`). Release builds only through `npm run release` = `tauri build --config src-tauri/tauri.release.conf.json`.
- Run value: key `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`, value name `Haul`, data `"<exe>" --hidden`. Tests use `HKCU\Software\Haul-test-<random>`, never the real Run key.
- Collector headers: DPAPI-sealed with `haul_engine::secret::protect`, column `collector_links.headers_blob`, migration 7, `SCHEMA_VERSION = 7`. Never in any UI-facing type.
- Update feed: `https://api.github.com/repos/meraaki410/haul/releases/latest`; `User-Agent: Haul/<CARGO_PKG_VERSION>`; `Accept: application/vnd.github+json`; 15 s timeout; first check 30 s after start, then every 24 h, skipped if the last check was under 24 h ago. Only URLs starting `https://github.com/meraaki410/haul/releases/` are opened; fallback `https://github.com/meraaki410/haul/releases`.
- Settings keys added: `check_updates` (bool, default true, camelCase `checkUpdates`), `last_update_check` (internal UTC RFC 3339 string, not in the `Settings` record).
- Installer: NSIS only, `installMode: "currentUser"`, English only, licence `LICENSE-MIT`. Haul's licence: MIT OR Apache-2.0, holder "The Haul contributors".
- Repo rules: write files with the Write tool or `python -I` scripts; never write `%APPDATA%\dev.haul.desktop*\haul.db` directly; while `tauri dev` runs, run Rust tests with `CARGO_TARGET_DIR=_tmp/target-alt`; scratch files in `_tmp/`. Ledger: `.superpowers/sdd/2026-11-08-haul-plan-6b-packaging/progress.md`, every deviation a `Ruling:` line with its cost if wrong.
- Gate (each task's end): `cargo test --workspace --no-fail-fast`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --check`, and in `app/`: `npx vitest run`, `npm run check`, `npx playwright test`. The known engine timing flakes (handoff) are rerun alone before they are believed.

## Review Focus

1. **A path with spaces in the user profile** (`C:\Users\Jane Doe\AppData\Local\Haul\haul.exe`) must still give a quoted Run value. Pinned by `run_value_quotes_spaces` (Task 2).
2. **Turning Start with Windows back on after Task Manager disabled it** must actually start Haul at sign-in. Pinned by `enable_clears_startup_approved_disabled` (Task 2).
3. **The same login-only link captured twice into the Collector**, the second time with fresh cookies, must use the fresh ones. Pinned by `second_capture_refreshes_headers` (Task 3).
4. **GitHub answering 403 (rate limit) or with a malformed body** must never crash or nag. It stays quiet in the status bar and shows a reason under Check now. Pinned by `rate_limited_reply_is_an_error_not_a_crash` (Task 5).
5. **An extension file named in `manifest.json` but missing from `dist`** must fail the checks, not the installed copy. Pinned by `bundled_extension_has_every_manifest_file` (Task 7).

---

### Task 1: Separate dev and release identifiers; split the capabilities

**Files:**
- Modify: `app/src-tauri/tauri.conf.json` (identifier, `app.security.capabilities`)
- Create: `app/src-tauri/tauri.release.conf.json`, `app/src-tauri/capabilities/dev-test.json`, `app/src-tauri/tests/config.rs`
- Modify: `app/src-tauri/capabilities/default.json`, `app/package.json` (`release` script), `app/e2e-real/smoke.spec.ts:224`

**Interfaces:**
- Produces: dev data in `%APPDATA%\dev.haul.desktop.dev`; `npm run release`; capability ids `default` and `dev-test`.

- [ ] **Step 1: Write the failing test** `app/src-tauri/tests/config.rs`, which reads the JSON files with `include_str!` and `serde_json`:
  - `dev_and_release_identifiers_differ`: base `identifier == "dev.haul.desktop.dev"`; release overlay `identifier == "dev.haul.desktop"`.
  - `allow_close_is_dev_only`: `default.json` permissions do not contain `core:window:allow-close`; `dev-test.json` does; base `app.security.capabilities == ["default", "dev-test"]`; release `app.security.capabilities == ["default"]`.
  - `release_script_uses_overlay`: `app/package.json` `scripts.release == "tauri build --config src-tauri/tauri.release.conf.json"`.
- [ ] **Step 2: Run it and watch it fail.** Run `cargo test -p haul-app --test config`. Expected: 3 failures (wrong identifier, missing files or keys).
- [ ] **Step 3: Make the changes.** `dev-test.json` has `"windows": ["main"]` and only `core:window:allow-close`. The release overlay holds only `identifier` and `app.security.capabilities` (Tauri merges it as a JSON Merge Patch). The smoke test reads `api-port` from `dev.haul.desktop.dev`.
- [ ] **Step 4: Run the test and the gate; both should pass.** Then start `npx tauri dev` once and confirm `%APPDATA%\dev.haul.desktop.dev\haul.db` is created. In it, set the download folder to `_tmp\smoke` through Settings. Run `npm run smoke`; expected 6/6. Note in the ledger that the dev data is now fresh, so the extension pairs again with the dev build.
- [ ] **Step 5: Commit.** `feat: separate dev and release identifiers; allow-close only in dev`

### Task 2: Quoted Start with Windows (`autostart.rs`)

**Files:**
- Create: `app/src-tauri/src/autostart.rs`
- Modify: `app/src-tauri/Cargo.toml` (drop `tauri-plugin-autostart`; `windows-sys` features add `Win32_System_Registry`, `Win32_Foundation`), `app/src-tauri/src/lib.rs` (remove the plugin; manage `Autostart`; start-up repair; single-instance callback), `app/src-tauri/src/commands.rs:31-45` (`set_settings`), `app/src-tauri/src/tray.rs` (add `should_show`), `app/src-tauri/capabilities/default.json` (drop `autostart:default`), `app/src-tauri/src/core.rs` (an `AppCore::set_autostart_flag(bool)` that saves the setting without applying anything)

**Interfaces:**
- Produces:
  - `pub fn run_value(exe: &Path) -> String`
  - `pub struct Autostart { key: String }` with `Autostart::real() -> Autostart` and `Autostart::at(key: &str) -> Autostart`, plus `enable(&self, exe: &Path) -> io::Result<()>`, `disable(&self) -> io::Result<()>`, `is_enabled(&self) -> io::Result<bool>` and `value(&self) -> io::Result<Option<String>>`
  - `pub fn repair(a: &Autostart, setting_on: bool, exe: &Path) -> io::Result<bool>`: returns the setting's new value
  - `tray::should_show(args: &[String]) -> bool`

- [ ] **Step 1: Write the failing tests** in `autostart.rs` `#[cfg(test)]`:
  - `run_value_quotes_spaces`: `run_value(Path::new(r"C:\Users\Jane Doe\AppData\Local\Haul\haul.exe")) == r#""C:\Users\Jane Doe\AppData\Local\Haul\haul.exe" --hidden"#`.
  - `run_value_strips_verbatim_prefix`: `\\?\C:\a b\haul.exe` gives `"C:\a b\haul.exe" --hidden`.
  - `enable_writes_quoted_value` / `disable_removes_value` / `disable_twice_is_fine` / `is_enabled_follows_value`: on `Autostart::at(&format!(r"Software\Haul-test-{}", random))`. A guard struct deletes the scratch key tree (`RegDeleteTreeW`) on drop.
  - `enable_clears_startup_approved_disabled`: the scratch layout mirrors the real one. `Autostart::at(k)` uses `<k>\Run`, plus `<k>\StartupApproved\Run` for the approved key. Pre-write a REG_BINARY `Haul` = `[3,0,0,0,0,0,0,0,0,0,0,0]` there (Windows' "disabled" mark); after `enable`, that value is gone.
  - `repair_rewrites_unquoted`: pre-write `C:\a b\haul.exe --hidden`. Then `repair(.., true, exe)` returns `true` and `value()` equals `run_value(exe)`.
  - `repair_turns_setting_off_when_value_gone`: no value, so `repair(.., true, exe)` returns `false` and writes nothing.
  - `repair_leaves_off_alone`: `repair(.., false, exe)` returns `false` and writes nothing.
  - In `tray.rs`: `second_launch_args_hidden_keeps_window_hidden`: `should_show(&["haul.exe".into(), "--hidden".into()]) == false` and `should_show(&["haul.exe".into()]) == true`.
- [ ] **Step 2: Run them and watch them fail.** Run `cargo test -p haul-app autostart` and `cargo test -p haul-app should_show`. Expected: compile errors (missing items), which count as RED.
- [ ] **Step 3: Implement.** Registry calls use `RegCreateKeyExW` / `RegSetValueExW` (REG_SZ) / `RegQueryValueExW` / `RegDeleteKeyValueW` on `HKEY_CURRENT_USER`. A missing value (`ERROR_FILE_NOT_FOUND`) means `Ok(None)` or `Ok(())` on delete. `real()` uses `Software\Microsoft\Windows\CurrentVersion\Run` and `Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run`. `enable` deletes the approved value only when its first byte is odd (Windows' disabled mark), so it touches nothing else.
- [ ] **Step 4: Wire it up.**
  - `set_settings` calls `Autostart::real().enable(&current_exe)` or `.disable()` and keeps today's error text prefix `autostart: `.
  - In `lib.rs` setup, after `AppCore` exists, call `repair(&Autostart::real(), core.settings().autostart, &current_exe)`; if the result differs from the setting, call `core.set_autostart_flag(result)`. A repair error is logged, not fatal.
  - The single-instance callback calls `show_main` only when `should_show(&args)`.
- [ ] **Step 5: Run the tests and the gate; both should pass.** Then by hand on the dev build: turn the switch on, and `reg query HKCU\Software\Microsoft\Windows\CurrentVersion\Run /v Haul` shows `"C:\tools\claude for project\target\debug\haul.exe" --hidden`. Turn it off and the value is gone. Record both in the ledger.
- [ ] **Step 6: Commit.** `fix: write the Start with Windows path in quotes (own autostart module)`

### Task 3: Engine: Collector links keep sealed headers

**Files:**
- Create: `crates/engine/src/store/migration_7.sql` (`ALTER TABLE collector_links ADD COLUMN headers_blob BLOB;`)
- Modify: `crates/engine/src/store/mod.rs` (`SCHEMA_VERSION = 7`, migration list, a `migration_7_keeps_links` test), `crates/engine/src/store/collector.rs`, `crates/engine/src/grabber/mod.rs`, `crates/engine/src/transfer/mod.rs:134` (make `parse_headers` `pub(crate)`), `crates/engine/src/lib.rs` (exports), `crates/engine/tests/grabber.rs` (the fake prober takes the new argument)

**Interfaces:**
- Consumes: `secret::protect` and `secret::unprotect`; `transfer::parse_headers(Option<&str>) -> HeaderMap`.
- Produces:
  - `pub type ProbeFn = Arc<dyn Fn(String, Option<String>) -> BoxFuture<'static, Result<ProbeInfo, EngineError>> + Send + Sync>`. The second argument is the headers JSON; `http_prober` turns it into a `HeaderMap` with `parse_headers`.
  - `Store::set_link_headers(id: i64, headers_json: Option<&str>) -> Result<(), EngineError>` seals the headers (NULL for `None`).
  - `Store::link_headers(id: i64) -> Result<Option<String>, EngineError>` unseals them; an unseal failure gives `Ok(None)` plus `tracing::warn!(id, "collector link headers unreadable")`.
  - `Store::find_link_by_url(url: &str) -> Result<Option<i64>, EngineError>`.
  - `Grabber::intake_capture(url: String, headers_json: Option<String>) -> Result<IntakeResult, EngineError>`. If the URL is already waiting: replace its headers, set it Checking, re-probe, and return `{added: 0, skipped: 1}`. Otherwise: a one-link batch with source `"browser"` and the pasted label, its headers stored before the probe is spawned, and `{added: 1, skipped: 0}`.
  - `spawn_probe(id, url)` reads `link_headers(id)` itself, so `recheck` and `intake` need no change. `start` puts `link_headers(l.id)?` into `NewDownload.headers_json`.

- [ ] **Step 1: Write the failing tests** in `crates/engine/tests/grabber.rs`, using a recording fake prober that stores each `(url, headers)` call:
  - `captured_link_probe_gets_its_cookie`: `intake_capture(u, Some(r#"{"Cookie":"sid=1"}"#))`; the fake sees `Some` containing `sid=1`.
  - `pasted_link_probe_gets_no_headers`: `intake(vec![u], "paste")`; the fake sees `None`.
  - `captured_headers_are_sealed`: a raw `SELECT headers_blob` holds bytes that do not contain `sid=1`, and `CollectorState` serialised to JSON does not contain `sid` either.
  - `start_passes_capture_headers`: after the probe settles Online, `start(&[id], ..)`; the queued download's `headers_json` contains `sid=1`.
  - `second_capture_refreshes_headers`: capture with `sid=1`, then with `sid=2`. The result is `{added:0, skipped:1}`, `link_headers` contains `sid=2`, and the fake's last call carried `sid=2`.
  - `remove_drops_headers`: after `remove(&[id])`, `link_headers(id)` is `Ok(None)` and the row is gone.
  - `captured_login_link_is_online_with_cookie`: against `TestServer` with `require_header: Some(("cookie".into(), "sid=1".into()))`, use `http_prober(http_client())`. The captured link ends Online; the same URL pasted (no headers) ends Offline or Expired.
  - In `store/mod.rs`: `migration_7_keeps_links`, in the same style as `migration_6_keeps_old_rows`. Run migrations 1–6 and insert a link, then open the store at 7; the link is still there with `link_headers == None`.
- [ ] **Step 2: Run them and watch them fail.** Run `cargo test -p haul-engine --test grabber` and `cargo test -p haul-engine migration_7`. Expected: compile errors or failures.
- [ ] **Step 3: Implement as in the Interfaces block.** `list_links` does not select `headers_blob`.
- [ ] **Step 4: Run the tests and the gate; both should pass.**
- [ ] **Step 5: Commit.** `feat(engine): Collector links keep DPAPI-sealed capture headers (migration 7)`

### Task 4: App: "Wait in the Collector" keeps the capture's cookies (I5)

**Files:**
- Modify: `app/src-tauri/src/capture.rs:38-44` (and its tests), `app/src-tauri/src/core.rs` (the `ProbeFn` signature in test deps, if any)

**Interfaces:**
- Consumes: `Grabber::intake_capture` (Task 3); `headers_json(&CaptureRequest)` (exists).

- [ ] **Step 1: Write the failing tests** in `capture.rs`:
  - `collector_capture_keeps_cookies`: with `browser_start = Collector`, `capture(req with cookies "sid=1")` returns 0. The Collector's one link has `store.link_headers(id)` containing `sid=1` and `Referer`.
  - `collector_capture_starts_with_cookies` (tokio, `TestServer` with `require_header` cookie `sid=1`): capture to the Collector, wait for Online, `collector_start` that link, wait for Completed; the file length matches.
- [ ] **Step 2: Run them and watch them fail.** Run `cargo test -p haul-app collector_capture`. Expected: FAIL, because the headers are `None` and the link is Offline.
- [ ] **Step 3: Implement.** The Collector branch calls `self.grabber.intake_capture(req.url.clone(), headers)` and drops the "keeps no headers" comment. The log line keeps the cookie count only, never values.
- [ ] **Step 4: Run the tests and the gate; both should pass.**
- [ ] **Step 5: Commit.** `fix: browser captures sent to the Collector keep their cookies (I5)`

### Task 5: Update check (Rust)

**Files:**
- Create: `app/src-tauri/src/update.rs`
- Modify: `app/src-tauri/Cargo.toml` (`reqwest` in `[dependencies]` with the engine's version and features, `default-features = false, features = ["rustls-no-provider", "json"]`), `app/src-tauri/src/settings.rs` (`check_updates` field and patch, key `KEY_CHECK_UPDATES = "check_updates"`, default true; `KEY_LAST_UPDATE_CHECK = "last_update_check"`), `app/src-tauri/src/commands.rs` (`update_status`, `check_updates_now`, `open_release`), `app/src-tauri/src/lib.rs` (register the commands, spawn the loop), `app/src-tauri/src/dto.rs` if DTOs live there

**Interfaces:**
- Produces:
  - `pub const FEED: &str = "https://api.github.com/repos/meraaki410/haul/releases/latest";`
  - `pub fn is_newer(current: &str, tag: &str) -> bool`
  - `pub fn safe_release_url(html_url: Option<&str>) -> String`
  - `#[serde(rename_all = "camelCase")] pub struct UpdateStatus { pub current: String, pub latest: Option<String>, pub url: Option<String>, pub checked_at: Option<String>, pub error: Option<String> }`, where `latest` and `url` are `Some` only when newer.
  - `pub async fn check(client: &reqwest::Client, feed: &str, current: &str) -> UpdateStatus`
  - `pub fn due(last: Option<&str>, now: chrono::DateTime<Utc>) -> bool`: true when `None`, unparsable, or 24 h or more ago.
  - `pub struct Updater { status: Mutex<UpdateStatus>, feed: String }`, managed state. `run_loop(app)` sleeps 30 s, then once an hour asks `due` and checks when the setting is on, saving `last_update_check` after each attempt. A newer result emits `haul:update-available {version, url}`.
  - Commands: `update_status() -> UpdateStatus`; `check_updates_now() -> UpdateStatus` (always checks and saves the time); `open_release(url: String) -> R<()>`, which opens the URL only if `safe_release_url(Some(&url)) == url`, else returns an error.

- [ ] **Step 1: Write the failing tests** in `update.rs`:
  - `newer_versions_table`: `is_newer("0.1.0","v0.2.0")`, `is_newer("0.9.9","0.10.0")` and `is_newer("0.1.0","1.0.0")` are true; `is_newer("0.1.0","v0.1.0")`, `is_newer("0.2.0","0.1.9")`, `is_newer("0.1.0","v1.0.0-beta")`, `is_newer("0.1.0","latest")` and `is_newer("0.1.0","")` are false.
  - `foreign_url_falls_back`: `safe_release_url(Some("https://evil.example/x")) == "https://github.com/meraaki410/haul/releases"`; `Some("https://github.com/meraaki410/haul/releases/tag/v0.2.0")` is kept; `None` gives the fallback.
  - `due_after_a_day`: `None` and `"junk"` are due; 23 h ago is not; 24 h ago is.
  - Against a local axum server, each test spawning `/latest`:
    - `newer_release_reported`: `{"tag_name":"v0.2.0","html_url":"https://github.com/meraaki410/haul/releases/tag/v0.2.0"}` gives `latest == Some("0.2.0")` and `url` set.
    - `same_release_is_up_to_date`: `v0.1.0` gives `latest None`, `error None`.
    - `no_release_404_is_quiet`: 404 gives `latest None`, `error None`.
    - `rate_limited_reply_is_an_error_not_a_crash`: 403 with `{"message":"API rate limit exceeded"}` gives `error == Some(..)` mentioning 403.
    - `bad_json_is_an_error`: 200 `not json` gives `error Some`.
    - `request_sends_user_agent`: the server records the `User-Agent`, which equals `format!("Haul/{}", env!("CARGO_PKG_VERSION"))`, and `Accept` is `application/vnd.github+json`.
  - In `settings.rs`: `check_updates_defaults_on_and_saves`: a fresh store gives `check_updates == true`; a patch with `false` reads back false.
- [ ] **Step 2: Run them and watch them fail.** Run `cargo test -p haul-app update` and `cargo test -p haul-app check_updates`. Expected: compile errors.
- [ ] **Step 3: Implement as in the Interfaces block.** The version is parsed as `trim_start_matches('v')`, then exactly three `u64` parts. An error message reads like `GitHub answered 403` or `couldn't read GitHub's reply`. The client is built with `.timeout(15 s)` and `.user_agent(..)`.
- [ ] **Step 4: Run the tests and the gate; both should pass.**
- [ ] **Step 5: Commit.** `feat: daily update check against GitHub Releases`

### Task 6: Update check UI

**Files:**
- Modify: `app/src/lib/types.ts` (`UpdateStatus`, `Settings.checkUpdates`), `app/src/lib/backend.ts` / `backend-tauri.ts` / `backend-mock.ts` (`updateStatus()`, `checkUpdatesNow()`, `openRelease(url)`, `onUpdateAvailable(cb)`; the mock seeds `checkUpdates: true` and takes `?update=0.2.0` to report a newer version), `app/src/components/StatusBar.svelte`, `app/src/components/SettingsView.svelte`
- Create: `app/src/lib/update-state.svelte.ts`, `app/src/lib/update-state.svelte.test.ts`, `app/e2e/update.spec.ts`

**Interfaces:**
- Consumes: the Task 5 commands and the `haul:update-available` event.
- Produces: `updateState` with `status: UpdateStatus | null`, `checking: boolean`, `load()`, `checkNow()` and `message(): string`.

- [ ] **Step 1: Write the failing tests.**
  - Vitest `update-state.svelte.test.ts`, with the exact copy for `message()`:
    - Up to date: `"You have the latest version (0.1.0)."`
    - Newer: `"Version 0.2.0 is available."`
    - Error: `"Couldn't check: GitHub answered 403."`
    - Also: `checking` is true during `checkNow()`, and an `onUpdateAvailable` event sets `status.latest`.
  - E2e `update.spec.ts` on the mock, in light and dark, with axe:
    - `status_bar_shows_update`: `/?update=0.2.0` shows a button "Update available 0.2.0"; clicking it calls the mock's `openRelease` with the tag URL.
    - `no_notice_when_up_to_date`: no such button without `?update`.
    - `settings_check_now_reports`: Settings › App has the switch "Check for updates" (role=switch, checked) and a button Check now that shows the up-to-date line.
    - `switch_off_persists`: turn it off and reload; it is still off.
- [ ] **Step 2: Run them and watch them fail.** Run `npx vitest run update-state` and `npx playwright test update`.
- [ ] **Step 3: Implement.** The status-bar button sits at the right end, in the same style as the other status-bar buttons (`DESIGN.md` tokens). It is shown only when `status.latest` is set. Settings › App gets the switch after "Notify when a download finishes", and below it Check now with the message in an `aria-live="polite"` line.
- [ ] **Step 4: Run the tests and the gate; both should pass.** Screenshot the status bar and Settings › App in the mock, light and dark.
- [ ] **Step 5: Commit.** `feat: update notice in the status bar and Check now in Settings`

### Task 7: Bundle the extension, licences and the NSIS installer config

**Files:**
- Create: `LICENSE-MIT`, `LICENSE-APACHE` (repo root), `app/src-tauri/installer-hooks.nsh`, `docs/release-checklist.md`
- Modify: `app/src-tauri/tauri.conf.json` (`bundle.targets`, `bundle.resources`, `bundle.windows.nsis`, `bundle.license`, `publisher`, `copyright`, `shortDescription`, `build.beforeBuildCommand`), `app/src-tauri/tests/config.rs`

**Interfaces:**
- Produces: `<resource dir>\extension\` (read by the existing `open_extension_folder`) and `<resource dir>\7zip\`.

- [ ] **Step 1: Write the failing tests** in `tests/config.rs`:
  - `bundle_is_per_user_nsis`: `bundle.targets == ["nsis"]`, `bundle.windows.nsis.installMode == "currentUser"`, `bundle.windows.nsis.installerHooks == "installer-hooks.nsh"`, and `bundle.license == "../../LICENSE-MIT"`; both licence files exist and mention their licence's name.
  - `bundled_extension_has_every_manifest_file`: `bundle.resources` maps `"../../extension/dist/"` to `"extension/"`. Every file path in `extension/dist/manifest.json` exists under `extension/dist`: `background.service_worker`, `action.default_popup`, `options_page` or `options_ui.page`, every `icons` and `action.default_icon` value, and every `content_scripts[].js`. If `extension/dist` is missing, the test fails with `run npm run build in extension/`.
  - `uninstall_hook_removes_run_value`: `installer-hooks.nsh` contains `!macro NSIS_HOOK_POSTUNINSTALL`, `DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "Haul"` and the `Explorer\StartupApproved\Run` line.
  - `before_build_builds_extension`: `build.beforeBuildCommand == "npm --prefix ../extension run build && npm run build"`.
- [ ] **Step 2: Run them and watch them fail.** Run `cargo test -p haul-app --test config`.
- [ ] **Step 3: Make the changes.**
  - The NSIS block: `"nsis": {"installMode": "currentUser", "languages": ["English"], "displayLanguageSelector": false, "installerHooks": "installer-hooks.nsh", "installerIcon": "icons/icon.ico"}`.
  - The licence files: the standard MIT text with "Copyright (c) 2026 The Haul contributors", and the standard Apache-2.0 text.
  - `docs/release-checklist.md`, in this order: the gate → `npm run release` → the path of `target\release\bundle\nsis\Haul_0.1.0_x64-setup.exe` and its SHA-256 (`certutil -hashfile … SHA256`) → install → the spec's section 7 checks → uninstall.
- [ ] **Step 4: Run the tests and the gate; both should pass.**
- [ ] **Step 5: Commit.** `build: per-user NSIS installer bundling the extension, 7-Zip and licences`

### Task 8: Build, install and check the installed Haul

**Files:**
- Modify: `handoff.md`, `docs/release-checklist.md` (fill in what the first run taught)

- [ ] **Step 1: Quit the dev build first.** The release build writes the same `target\release`, so stop `tauri dev`. Then run `npm run release` in `app/` (first run: Tauri downloads NSIS; the owner allowed it). Expected: `Haul_0.1.0_x64-setup.exe` under `target\release\bundle\nsis\`. Record its size and SHA-256 in the ledger, and record what Tauri downloaded (from the build output).
- [ ] **Step 2: Inspect the bundle without installing it.** List `target\release\bundle\nsis\` and confirm the release exe's resources sit next to it: `target\release\extension\manifest.json` and `target\release\7zip\7z.exe`.
- [ ] **Step 3: Owner install.** Ask the owner to run the installer. Afterwards, check that `%LOCALAPPDATA%\Haul\haul.exe` exists, along with `extension\` and `7zip\` beside it (or wherever Tauri put the resources: read the install folder and record it). Check that `%APPDATA%\dev.haul.desktop\haul.db` is the one in use (its modified time changes).
- [ ] **Step 4: Owner checks, spec section 7, items 1–7,** each recorded in the ledger as passed or failed with what was seen. For item 5, show the `reg query` output. A failure becomes a fix task here, test-first.
- [ ] **Step 5: Update `handoff.md`.** Cover: Plan 6b done; the two data folders; `npm run release`; the autostart module; the I5 fix; the update check; the installer path; what remains (signing, Firefox, the remaining 6a minors). Commit `docs: plan 6b handoff`.
- [ ] **Step 6: Request a fresh whole-branch review** (superpowers:requesting-code-review), fix its Important findings test-first, then finish the branch (superpowers:finishing-a-development-branch).
