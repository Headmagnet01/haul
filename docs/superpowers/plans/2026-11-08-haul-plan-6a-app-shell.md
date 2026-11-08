# Haul Plan 6a: Settings, Tray, "When Done", Finished and First Run Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Haul can be left running: settings apply live, closing the window keeps downloading in the tray, completed downloads live in Finished, and "when done" can sleep or shut the PC down.

**Architecture:**
- **Engine:** migration 6 adds `downloads.finished_at`; the progress speed window grows to 5 s. Nothing else in the engine changes.
- **App crate:**
  - `settings.rs`: the `Settings` record, validation and a patch type. `AppCore` holds it in a `RwLock` and applies each change live.
  - `tray.rs`: the tray icon, menu and tooltip; `TrayStats` (pure) turns progress events into the tooltip text.
  - `when_done.rs`: `should_fire` and the countdown (pure), and the `PowerAction` trait with the real Windows calls.
  - `toast.rs`: Windows toasts with an Open folder button, replacing the plugin notifier.
  - `lib.rs`: single instance, autostart (`--hidden`), the close handler.
- **UI:** `SettingsView`, `FinishedView`, `CountdownDialog`, a When done selector in the status bar, and a first-run `Welcome` in place of the empty state. Which rows are Finished is decided in the UI by a pure function, from the snapshot and post-processing runs.

**Tech Stack:** as in plan 5, plus:
- App: `tauri` feature `tray-icon`; `tauri-plugin-single-instance 2`, `tauri-plugin-autostart 2`, `tauri-plugin-dialog 2` (and `@tauri-apps/plugin-dialog 2` in `app/package.json`); `tauri-winrt-notification 0.7`; `windows-sys 0.59` with feature `Win32_System_Power`.

**Spec:** `docs/superpowers/specs/2026-11-08-haul-plan-6a-app-shell-design.md`. Two refinements found while planning, which the plan follows:
- Post-processing creates a run for every finished package and lone file (state `held` when auto mode is off), and its `notify` step already sends "<name> is ready" with the folder. So section 6.4's notifications are that notice, shown through the new toast; there is no separate "download finished" notice.
- "Post-processing ended" (spec 6.2) means the run is `done` or `held`, or there is no run. Runs that are `queued`, `running`, `waiting`, `needs_password` or `failed` keep the rows in Queue, where Retry and the password prompt are.

**Not in this plan (on purpose):** the installer, update check, extension store listing, signing (plan 6b); themes, proxy, languages.

## Global Constraints

- Everything in plans 2–5's Global Constraints still holds (tokens, fonts, motion, icon plus word, never destroy data, test-first, no secrets in logs).
- **Settings ranges:** `maxParallel` 1–20 (default 3), `connections` 1–16 (default 8), speed limit ≥ 1024 B/s when enabled. Out-of-range numbers are clamped, never refused.
- **Settings keys:** the existing `download_dir`, `max_parallel`, `speed_limit`, `watch_clipboard` keep their names and JSON shapes. New keys: `connections`, `browser_start`, `schedule`, `close_to_tray`, `autostart`, `notify_finished`, `tray_hint_shown`, `first_download_seen`.
- **When done** is never persisted. The engine's `when_done` setting key is no longer read or written.
- **Countdown:** 60 s. Copy: title `Shutting down in 60 s` / `Going to sleep in 60 s` (the number counts down), buttons `Cancel` (default, focused) and `Do it now`.
- **Tray tooltip:** `Haul · <n> downloading · <speed>` or `Haul · idle`; at most one update per second. Speed formatted like the status bar (`7.0 MB/s`).
- **One-time tray notice:** title `Haul is still running`, body `Downloads continue in the tray. Right-click the tray icon to quit.`
- **Welcome copy:** step 2's line is exactly `Haul notices download links you copy; nothing is sent anywhere.`
- **Events (Rust → UI):** `haul:show-view` with payload `"queue" | "finished" | "settings"`; `haul:when-done` with payload `{ mode: "nothing"|"sleep"|"shutdown", countdown: number | null }`.
- **Never** call a real `SetSuspendState` or `shutdown` from a test; every test uses the fake `PowerAction`.

## Review Focus

1. **Download folder on a removed drive or a read-only folder** (a USB stick unplugged after it was chosen): expected, starting a download fails that download with a clear "folder not writable" message; Settings still opens and shows the path. (Task 2: `start_with_missing_folder_fails_download_not_app`.)
2. **"When done" chosen, then the last download fails:** expected, it still fires (failed is finished). And a run that needs a password must stop it. (Task 6: `fires_after_failure`, `password_wait_blocks`.)
3. **A second Haul launched while the first is hidden in the tray** (double-click the shortcut): expected, no second window and no second API server; the first window shows. (Task 4: real-app smoke `second_launch_shows_first`.)
4. **Delete file… on a file another program has open:** expected, the Recycle Bin call fails, the row stays, and a toast says the file is in use. (Task 5: `delete_file_in_use_keeps_row`.)
5. **The clock passes midnight or Haul runs for days:** expected, Finished shows `14:32` only for today and a date otherwise, computed when rendered, not when stored. (Task 5: `finished_time_today_vs_older`.)

---

### Task 1: Engine, `finished_at` and a steadier speed

**Files:**
- Create: `crates/engine/src/store/migration_6.sql`
- Modify: `crates/engine/src/store/mod.rs` (migration list, `Download.finished_at`, `set_completed`, `reset_download`), `crates/engine/src/types.rs` if `Download` lives there, `crates/engine/src/transfer/mod.rs` (`emit_progress`)
- Test: store tests in `crates/engine/src/store/mod.rs`, `crates/engine/tests/transfer*.rs` or the `transfer` unit tests

**Interfaces:**
- Produces: `Download.finished_at: Option<String>` (UTC, `%Y-%m-%dT%H:%M:%SZ`); `SPEED_WINDOW: Duration = 5 s` in `transfer/mod.rs`.

- [ ] **Step 1: Write the failing tests:**
  - `completed_sets_finished_at`: after `set_completed`, `finished_at` is `Some` and parses as RFC 3339.
  - `reset_clears_finished_at`: after `reset_download`, `finished_at` is `None`.
  - `migration_6_keeps_old_rows`: a DB at version 5 with a completed row opens at version 6, row intact, `finished_at` `None`.
  - `speed_uses_five_second_window`: feed `emit_progress`'s sample logic 1 MB/s for 4 s then 0 for 2 s; the reported speed is > 0 (a 2 s window would report 0). Extract the windowing into `fn windowed_speed(samples: &mut VecDeque<(Instant, u64)>, now: Instant, rx: u64, window: Duration) -> u64` so it can be tested with synthetic `Instant`s.
- [ ] **Step 2: Run** `cargo test -p haul-engine`; expect the four to FAIL.
- [ ] **Step 3: Implement:** `migration_6.sql` is `ALTER TABLE downloads ADD COLUMN finished_at TEXT;` plus the version bump in the same style as migration 5. `set_completed` sets `finished_at = strftime('%Y-%m-%dT%H:%M:%SZ','now')`; `reset_download` sets it to NULL. `emit_progress` calls `windowed_speed` with `SPEED_WINDOW`.
- [ ] **Step 4: Run** `cargo test --workspace`; expect all green.
- [ ] **Step 5: Commit** `feat(engine): finished_at and a 5 s speed window`

### Task 2: App, the `Settings` record applied live

**Files:**
- Create: `app/src-tauri/src/settings.rs`
- Modify: `app/src-tauri/src/core.rs` (hold `RwLock<Settings>`; `download_dir` and connections read from it; remove `KEY_*` constants that move to `settings.rs`), `app/src-tauri/src/capture.rs` (`browserStart`), `app/src-tauri/src/commands.rs`, `app/src-tauri/src/lib.rs` (register commands; remove `set_speed_limit`, `set_watch_clipboard`)
- Test: unit tests in `settings.rs`; `app/src-tauri/src/core.rs` tests

**Interfaces:**
- Produces:
  - `pub struct Settings { download_dir: PathBuf, max_parallel: usize, connections: u32, speed_limit: SpeedLimit, browser_start: BrowserStart, watch_clipboard: bool, schedule: Option<Schedule>, close_to_tray: bool, autostart: bool, notify_finished: bool, tray_hint_shown: bool, first_download_seen: bool }`, serde camelCase. `pub enum BrowserStart { Now, Collector }` serialised `"now"`/`"collector"`.
  - `pub struct SettingsPatch` — the same fields, each `Option<_>` (`schedule: Option<Option<Schedule>>` with `#[serde(default, deserialize_with = …)]` so `null` means "turn off").
  - `Settings::load(store: &Store, default_dir: PathBuf, has_downloads: bool) -> Settings` (missing keys take defaults; `first_download_seen` defaults to `has_downloads`).
  - `fn validate(patch: SettingsPatch) -> Result<SettingsPatch, String>` — clamps numbers; refuses an unwritable folder (`"downloadDir: <reason>"`), a schedule with `start == stop` or no days (`"schedule: …"`).
  - `AppCore::settings(&self) -> Settings`, `AppCore::set_settings(&self, patch: SettingsPatch) -> Result<Settings, EngineError>` — validate, save each changed key, apply (`queue.set_max_parallel`, `set_global_limit`, `set_schedule`, clipboard flag), return the full record. `autostart` is applied by the command layer (Task 4), not here.
  - Commands `settings() -> Settings`, `set_settings(patch) -> Settings`.

- [ ] **Step 1: Write the failing tests** (in-memory store, temp dirs):
  - `defaults_when_empty`: `maxParallel 3`, `connections 8`, `closeToTray true`, `autostart false`, `notifyFinished true`, `browserStart Now`.
  - `existing_keys_read_back`: a store with the old `download_dir`, `max_parallel`, `speed_limit`, `watch_clipboard` values loads them.
  - `numbers_are_clamped`: `maxParallel 0 → 1`, `99 → 20`; `connections 0 → 1`, `40 → 16`; enabled limit `10 → 1024`.
  - `unwritable_folder_refused_nothing_saved`: a path that doesn't exist → `Err` starting `downloadDir:`, and a `maxParallel` in the same patch is not saved.
  - `bad_schedule_refused`: `start == stop` → `Err` starting `schedule:`.
  - `folder_change_applies_to_next_download`: after `set_settings(downloadDir = B)`, a new `add_links` → `collector_start` download has `target_dir == B`; an earlier queued one keeps A.
  - `connections_apply_to_next_download`.
  - `browser_start_collector_sends_capture_to_collector`: with `Collector`, `capture` adds a Collector link (source `browser`) and queues nothing.
  - `start_with_missing_folder_fails_download_not_app` (Review Focus 1): settings point at a folder that is then deleted; starting a download marks it failed with an error containing `not writable` (reuse the engine's existing disk-check error if it has one), and `settings()` still returns the path.
- [ ] **Step 2: Run** `cargo test -p haul-app`; expect FAIL.
- [ ] **Step 3: Implement** as in Interfaces. Writability probe: create and delete `<dir>\.haul-write-test`. Replace every `DEFAULT_CONNECTIONS` use with the setting.
- [ ] **Step 4: Run** `cargo test --workspace` and `cargo clippy --workspace --all-targets -- -D warnings`; expect green.
- [ ] **Step 5: Commit** `feat(app): one Settings record, applied live`

### Task 3: UI, the Settings page

**Files:**
- Create: `app/src/components/SettingsView.svelte`, `app/src/lib/settings-state.svelte.ts`
- Modify: `app/src/lib/types.ts` (`Settings`, `SettingsPatch`, `BrowserStart`), `backend.ts`, `backend-tauri.ts`, `backend-mock.ts`; `StatusBar.svelte` and wherever the clipboard and speed toggles call `setWatchClipboard`/`setSpeedLimit` (switch to `setSettings`); `App.svelte` (route `settings`); `RulesView.svelte` (remove the Browsers section); `app/package.json` (`@tauri-apps/plugin-dialog`); `app/src-tauri/capabilities/default.json` (`dialog:allow-open`)
- Test: `app/src/lib/settings-state.svelte.test.ts`, `app/src/lib/backend-mock.test.ts`, `app/e2e/settings.spec.ts`; add the page to `app/e2e/a11y.spec.ts`; move the Browsers e2e from `pairing.spec.ts` to the Settings page

**Interfaces:**
- Consumes: Task 2's commands and shape.
- Produces: `Backend.settings(): Promise<Settings>`, `Backend.setSettings(p: SettingsPatch): Promise<Settings>` (rejects with the message), `Backend.pickFolder(): Promise<string | null>`, `Backend.openPath(path: string): Promise<void>`. `setWatchClipboard` and `setSpeedLimit` are removed from `Backend`.

- [ ] **Step 1: Write the failing tests:**
  - Vitest `save_failure_reverts`: a rejected `setSettings` leaves the state at the last saved value and sets `error[field]` to the message.
  - Mock: `set_settings_clamps` (same numbers as Task 2), `folder_change_used_by_next_download`.
  - e2e `settings.spec.ts` (light and dark): change Downloads at once to 5 and back; the status bar clipboard toggle and the Settings switch show the same state after toggling either; choosing Wait in the Collector then a mock browser capture lands in the Collector; a mock-refused folder shows the message under the folder row; Browsers section lists a paired browser with Remove.
- [ ] **Step 2: Run** `npm test` and `npm run e2e -- settings` in `app/`; expect FAIL.
- [ ] **Step 3: Implement** the page per spec 3.2: groups Downloads, Adding links, Schedule, App, Browsers, each a `<section aria-labelledby>`. Number inputs save on change (blur or Enter). Schedule: Off/On, two `<input type="time">`, seven day toggles (Mon first). The mock's `pickFolder` returns `C:\Users\you\Downloads\Picked`.
- [ ] **Step 4: Run** `npm test`, `npm run check`, `npm run e2e`; expect green.
- [ ] **Step 5: Commit** `feat(ui): Settings page`

### Task 4: Tray, close to tray, single instance, autostart

**Files:**
- Create: `app/src-tauri/src/tray.rs`
- Modify: `app/src-tauri/Cargo.toml` (feature `tray-icon`, the three plugins), `app/src-tauri/src/lib.rs`, `app/src-tauri/src/commands.rs` (`set_settings` applies `autostart` via `tauri_plugin_autostart::ManagerExt`), `app/src-tauri/capabilities/default.json`, `app/src-tauri/icons/` (`tray.png`, `tray-active.png`, 32×32, from the existing icon plus a `--signal` dot)
- Modify: `app/src/App.svelte` (listen for `haul:show-view`)
- Test: unit tests in `tray.rs`; `app/e2e-real/smoke.spec.ts` gains two cases

**Interfaces:**
- Produces:
  - `pub struct TrayStats` with `fn on_event(&mut self, e: &Event)` and `fn tooltip(&self) -> String` and `fn busy(&self) -> bool`; it tracks per-download speed from `Progress` and running ids from `StateChanged`.
  - `pub fn build_tray(app: &AppHandle) -> tauri::Result<TrayIcon>` with menu ids `open`, `pause_all`, `resume_all`, `speed_limit`, `wd_nothing`, `wd_sleep`, `wd_shutdown`, `quit`.
  - `AppCore::resume_all(&self)` (resume every paused download).
  - `Backend.onShowView(cb: (v: 'queue'|'finished'|'settings') => void): () => void`.

- [ ] **Step 1: Write the failing tests:**
  - `tooltip_idle`: `Haul · idle`.
  - `tooltip_two_running`: two `StateChanged → Running`, progress 3 MB/s and 4 MB/s → `Haul · 2 downloading · 7.0 MB/s`.
  - `completed_leaves_count`: a `StateChanged → Completed` drops that id and its speed.
  - `hidden_flag_parsed`: `fn starts_hidden(args: &[String]) -> bool` true for `--hidden`.
- [ ] **Step 2: Run** `cargo test -p haul-app tray`; expect FAIL.
- [ ] **Step 3: Implement:**
  - Single instance is the first plugin registered; its callback shows, unminimises and focuses `main`.
  - Autostart plugin with `MacosLauncher::LaunchAgent` (ignored on Windows) and args `["--hidden"]`. If `starts_hidden`, the window is not shown at startup (set `visible: false` in `tauri.conf.json` and call `show()` in `setup` unless hidden).
  - `on_window_event` `CloseRequested`: if `settings().close_to_tray`, `prevent_close()` and `hide()`; the first time, send the one-time notice (Global Constraints copy) and save `trayHintShown`. Otherwise the existing quit path.
  - Tray menu `quit` runs the existing shutdown path then `app.exit(0)`. Left-click shows the window. The tooltip and icon update from a task that reads `TrayStats` once a second.
  - `wd_*` items call Task 6's `set_when_done` (until Task 6 lands, they are present but disabled).
- [ ] **Step 4: Run** `cargo test --workspace`; then the real-app smoke cases:
  - `close_keeps_downloading`: start a 64 MiB download from `app/scripts/range-server.mjs` with a 2 MB/s speed limit, then in the webview evaluate `window.__TAURI_INTERNALS__.invoke('plugin:window|close', { label: 'main' })` (this raises `CloseRequested` like the X button). After 3 s, `written` has grown and `invoke('plugin:window|is_visible', { label: 'main' })` is false.
  - `second_launch_shows_first` (Review Focus 3): with the window hidden, run `target\debug\haul.exe` again; it exits within 5 s, the first window is visible, and only one process listens on the API port.
- [ ] **Step 5: Commit** `feat(app): tray, close to tray, single instance, autostart`

### Task 5: The Finished view

**Files:**
- Create: `app/src/lib/finished.ts`, `app/src/components/FinishedView.svelte`
- Modify: `app/src-tauri/src/dto.rs` (`DownloadView.finished_at`), `app/src-tauri/src/core.rs` and `commands.rs` (new commands below), `app/src/lib/types.ts` (`finishedAt: string | null`), `backend*.ts`, `app/src/lib/app-state.svelte.ts` (Queue list excludes Finished rows; rail counts), `App.svelte` (route `finished`), `Rail.svelte` (count), `Drawer.svelte` (Finished actions)
- Test: `app/src/lib/finished.test.ts`, `app/src-tauri/src/core.rs` tests, `app/e2e/finished.spec.ts`, a11y

**Interfaces:**
- Produces:
  - `isFinished(downloads: DownloadView[], run: Run | null): boolean` — every download completed and (`run === null` or `run.state` is `done` or `held`). Applied per package (its downloads and its run) and per lone download (its run by `downloadId`).
  - `finishedTime(iso: string | null, now: Date): string` — `HH:MM` if the same local day, else a short local date (`8 Nov 2026`); `''` for null.
  - Commands: `open_file(id)`, `reveal(id)` (Explorer with the file selected; for a package folder, the folder), `files_exist(ids: Vec<i64>) -> Vec<i64>` (the ids whose `target_path` exists), `delete_files(ids: Vec<i64>) -> Result<(), String>` (recycles each existing `target_path` through the existing `RecycleBin` recycler, then deletes the rows; stops at the first failure and returns `"<name> is in use"` or the OS message, leaving that row and later ones), `download_again(id) -> i64` (new download, same URL, folder from settings).
  - `Backend` gains the matching `openFile`, `reveal`, `filesExist`, `deleteFiles`, `downloadAgain`.

- [ ] **Step 1: Write the failing tests:**
  - Vitest: `isFinished` for all-completed/no run → true; one running → false; run `running`/`needs_password`/`failed` → false; `held`/`done` → true. `finished_time_today_vs_older` (Review Focus 5): same day → `14:32`; yesterday → a date; null → `''`.
  - Rust: `delete_files_recycles_and_removes` (fake recycler records the path; row gone); `delete_file_in_use_keeps_row` (Review Focus 4: fake recycler errors; row stays; message names the file); `delete_files_missing_file_just_removes_row`; `download_again_uses_same_url`.
  - e2e `finished.spec.ts` (light and dark, `?demo=1`): a completed demo package is in Finished not Queue; Delete removes it with Undo restoring it; Shift+Delete shows a confirm dialog and, confirmed, removes the row; a mock-missing file shows `Missing` and Open is disabled; the rail count matches.
- [ ] **Step 2: Run** the three suites; expect FAIL.
- [ ] **Step 3: Implement** per spec 6.3. Queue's list and active count use `!isFinished`. The Finished list reuses `buildList` grouping with columns Name, Size, Finished, Location (folder of `targetPath`). `filesExist` is called on opening Finished and on selection. The drawer for a Finished row shows Open, Open folder, Download again, Remove from list, Delete file…, and the existing after-download panel (so a `held` package can still be processed).
- [ ] **Step 4: Run** `cargo test --workspace`, `npm test`, `npm run check`, `npm run e2e`; expect green.
- [ ] **Step 5: Commit** `feat: Finished view`

### Task 6: When done

**Files:**
- Create: `app/src-tauri/src/when_done.rs`, `app/src/components/CountdownDialog.svelte`, `app/src/components/WhenDoneSelect.svelte`
- Modify: `app/src-tauri/src/core.rs` (hold `WhenDoneState`; react to `AllDone` and post-processing run changes), `lib.rs` (register the real `PowerAction`, emit `haul:when-done`), `commands.rs`, `tray.rs` (enable `wd_*`, radio checks), `StatusBar.svelte` (mount the selector), `App.svelte` (mount the dialog), `backend*.ts`, `types.ts`
- Test: unit tests in `when_done.rs`, `app/e2e/when-done.spec.ts`

**Interfaces:**
- Produces:
  - `pub enum Mode { Nothing, Sleep, Shutdown }` (reuse `haul_engine::schedule::WhenDone` if it serialises as `"nothing"|"sleep"|"shutdown"`; otherwise map).
  - `pub struct Idle { pub running_or_queued: usize, pub postproc_busy: bool, pub postproc_waiting_on_user: bool, pub held_by_schedule: bool }`
  - `pub fn should_fire(mode: Mode, armed: bool, idle: &Idle) -> bool` — `armed` becomes true once anything has run after the mode was chosen.
  - `pub struct Countdown` with `start(now)`, `remaining(now) -> Option<u32>`, `cancel()`; 60 s.
  - `pub trait PowerAction: Send + Sync { fn sleep(&self) -> io::Result<()>; fn shutdown(&self) -> io::Result<()>; }`; `WindowsPower` calls `SetSuspendState(0, 0, 0)` and `shutdown /s /t 0`.
  - Commands `set_when_done(mode)`, `when_done_cancel()`, `when_done_now()`. Event `haul:when-done` (Global Constraints).
  - `Backend.setWhenDone`, `whenDoneCancel`, `whenDoneNow`, `onWhenDone(cb)`.

- [ ] **Step 1: Write the failing tests:**
  - `not_armed_does_not_fire`: idle, mode Shutdown, nothing ran since → false.
  - `fires_when_idle_and_armed`.
  - `fires_after_failure` (Review Focus 2): the last download failed, nothing running → true.
  - `password_wait_blocks` (Review Focus 2): `postproc_waiting_on_user` → false.
  - `schedule_hold_blocks`.
  - `countdown_counts_from_60`, `cancel_resets_to_nothing`, `new_work_cancels_countdown` (a download starting during the countdown).
  - `do_it_now_calls_power_once` with a recording fake: Shutdown runs the pause-and-save path before the fake's `shutdown`.
  - `when_done_resets_on_start`: a fresh `AppCore` reports Nothing even if the old `when_done` key is present.
  - e2e (mock exposes `window.__haulMock.queueDrained()`): choose Shut down in the status bar, drain → dialog `Shutting down in 60 s` with Cancel focused; Esc closes it and the selector shows Nothing; Do it now records `whenDoneNow` in `__haulMockCalls`.
- [ ] **Step 2: Run**; expect FAIL.
- [ ] **Step 3: Implement.** The decision runs on `AllDone`, on every post-processing run change and on each state change while a countdown is active. Countdown ticks emit `haul:when-done` once a second and show and focus the window at start. The tray's radio items mirror the mode.
- [ ] **Step 4: Run** all suites; expect green.
- [ ] **Step 5: Commit** `feat: when done (sleep or shut down) with a countdown`

### Task 7: Notifications with Open folder

**Files:**
- Create: `app/src-tauri/src/toast.rs`
- Modify: `app/src-tauri/Cargo.toml` (`tauri-winrt-notification`; drop `tauri-plugin-notification` if nothing else uses it), `lib.rs` (the notifier closure in `setup`; the one-time tray notice from Task 4 uses `toast` too)
- Test: unit tests in `toast.rs`

**Interfaces:**
- Produces: `pub fn toast_for(n: &Notice) -> ToastSpec` (pure: title, body, `open_folder: Option<PathBuf>`); `pub fn show(app: &AppHandle, spec: ToastSpec)`; clicking the toast emits `haul:show-view` `"finished"` and shows the window; the button opens `folder` in Explorer. Notices are dropped when `notify_finished` is false, except "needs attention" notices, which always show.

- [ ] **Step 1: Write the failing tests:** `ready_notice_has_open_folder`; `attention_notice_ignores_switch` (a fn `fn should_show(n: &Notice, notify_finished: bool) -> bool`: titles ending `is ready` respect the switch, `needs attention` always true); `no_folder_no_button`.
- [ ] **Step 2: Run**; expect FAIL.
- [ ] **Step 3: Implement** with `tauri_winrt_notification::Toast` using the app's AUMID (`tauri.conf.json` `identifier`, `dev.haul.desktop`), `.add_button("Open folder", "open")` and `on_activated`. If activation callbacks don't fire in the dev build (unregistered AUMID), ship the fallback from spec 6.4 and write which one shipped in `handoff.md`.
- [ ] **Step 4: Run** `cargo test --workspace`; then by hand in the dev build: finish a small download with auto mode on and check the toast, its button and its click. Record the result.
- [ ] **Step 5: Commit** `feat(app): toasts with Open folder`

### Task 8: First run

**Files:**
- Create: `app/src/components/Welcome.svelte`
- Modify: `app/src/components/EmptyState.svelte` or `QueueView.svelte` (show `Welcome` when the Queue is empty and `!settings.firstDownloadSeen`), `app/src-tauri/src/core.rs` (set `first_download_seen` on the first `queue.add` or Collector intake), `commands.rs` (`open_extension_folder`: opens `<resource dir>\extension` if bundled, else the repo's `extension\dist` in dev), `backend*.ts`
- Test: `app/e2e/welcome.spec.ts`, a11y; a Rust test `first_add_sets_first_download_seen`

**Interfaces:**
- Consumes: `Settings.firstDownloadSeen`, `Settings.watchClipboard`, `Backend.pairedClients()`.
- Produces: `Backend.openExtensionFolder(): Promise<void>`.

- [ ] **Step 1: Write the failing tests:** Rust `first_add_sets_first_download_seen`; e2e on a fresh mock (no `?demo`): the welcome shows three steps; turning on the clipboard switch ticks step 2; a mock pairing ticks step 3 with `Paired with Microsoft Edge`; adding a link replaces the welcome, and after removing that download the plain empty state shows.
- [ ] **Step 2: Run**; expect FAIL.
- [ ] **Step 3: Implement** per spec 7. Steps are an ordered list; a ticked step shows the check icon and the word `Done`.
- [ ] **Step 4: Run** all suites; expect green.
- [ ] **Step 5: Commit** `feat(ui): first-run welcome`

### Task 9: Real-app check and handoff

**Files:**
- Modify: `handoff.md`

- [ ] **Step 1: Run** every suite: `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --check`, and in `app/` `npm test`, `npm run check`, `npm run e2e`; then the real-app smoke including Task 4's two cases. Expect all green; note any known flake by name.
- [ ] **Step 2: Real app by hand:** set the folder in Settings and download into it; close the window mid-download and watch the tray tooltip; Quit from the tray; turn on autostart and confirm the registry entry (`HKCU\Software\Microsoft\Windows\CurrentVersion\Run`) carries `--hidden`, then turn it off and confirm it is gone.
- [ ] **Step 3: Update `handoff.md`:** Plan 6a status, test counts, the toast outcome from Task 7, the owner's hand checks still owed (tray menu, autostart after a real sign-in, one real Sleep), and the next step (Plan 6b).
- [ ] **Step 4: Commit** `docs: handoff after plan 6a`
