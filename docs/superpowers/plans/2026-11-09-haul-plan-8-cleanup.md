# Haul Plan 8: clean-up pass, implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix the chosen deferred minors from Plans 1–7 and repair the flaky capture test and the stale Edge real test, each fix test-first.

**Architecture:** No new modules except one small helper per fix where named. Engine fixes in `crates/engine`, app fixes in `app/src-tauri/src`, UI fixes in `app/src`, extension and script fixes in `extension/`. Six tasks, in spec order; each sub-fix is its own RED → GREEN → commit cycle.

**Tech Stack:** Rust (tokio, rusqlite, reqwest, windows-sys), Tauri 2, Svelte 5 + TypeScript (Vitest, Playwright), Node ESM scripts, MV3 extension.

**Spec:** `docs/superpowers/specs/2026-11-09-haul-plan-8-cleanup-design.md`

## Global Constraints

- Work in `C:\tools\claude for project\.claude\worktrees\plan8`, branch `feat/cleanup`. Never commit to `master` until the owner agrees to the merge.
- Write the code yourself; no codex/deepseek/opencode workers.
- Test-first: watch each new test fail for the stated reason before writing the fix. If it can't fail first, write a `Ruling:` line in the ledger saying why.
- Ledger: `.superpowers/sdd/2026-11-09-haul-plan-8-cleanup/progress.md` in the main checkout (git-ignored): one `Task N: complete` line per task, `Ruling:` lines for deviations.
- While `tauri dev` or the installed Haul runs, use `CARGO_TARGET_DIR=_tmp/target-alt` for cargo.
- Known engine timing flakes (`crash_simulation`, `speed_limit_respected`, `slow_connection_replaced`, `stalled_connections_replaced`, `restart_resumes_interrupted`): rerun alone before believing one.
- No new crates or npm packages, except removing `@tauri-apps/plugin-opener` (5g).
- Deletion always goes through the Recycle Bin; tests use temp folders only.
- User-facing text: plain English, says what to do. Exact strings below are fixed by the spec.
- Write files with the Write/Edit tools (Bash heredocs mangle quotes on this machine).

## Review Focus

1. A watched download that you pause and then remove must not leave When done blocked for good: removed ids drop out (test `removed_paused_download_unblocks`, Task 2).
2. A crash-recovery target path that now holds a different file (you replaced or truncated it) must download again, not be marked Completed (test `saved_target_with_wrong_size_downloads_again`, Task 3).
3. After upgrading to the found version (or past it), the saved "update available" must disappear (test `saved_update_cleared_once_installed`, Task 4).
4. Number fields given `" 5 "`, `"1e1"`, `"-3"`, `"0"`, `"21"` must accept only the whole numbers in range (`" 5 "` → 5; `"1e1"` → rejected) (test in `settings-num.test.ts`, Task 5).
5. A pairing name that is only control characters must show as "Unknown browser", not as an empty name (test `pairing_name_all_control_chars`, Task 6).

---

### Task 1: tests that give wrong signals

**Files:**
- Modify: `app/src-tauri/src/capture.rs` (test `collector_capture_starts_with_cookies`)
- Modify: `app/e2e-real/extension.spec.ts:~150-165`

**Interfaces:** none.

- [ ] **Step 1: Prove the cause.** Temporarily add `std::thread::sleep(Duration::from_millis(500));` between `let path = d.target_path.expect("saved");` and the `metadata` call. Run `cargo test -p haul-app collector_capture_starts_with_cookies` three times. Expected: FAIL each time with "The system cannot find the file specified". Record the result in the ledger, then remove the sleep.
- [ ] **Step 2: Fix the test.** After the Completed loop, wait (deadline 15 s, 50 ms polls) until `c.store.list_runs()` has a row whose `download_id == Some(d.id)` and whose `state` is one of `"done" | "failed" | "skipped"`. Assert it is `"done"`. Then re-read `c.store.get_download(d.id)` and check `metadata(target_path).len() == COOKIE_FILE_LEN`. Keep the sleep from Step 1 in place while you check: it must PASS now. Remove the sleep.
- [ ] **Step 3: Run the app crate tests 3 times in a row** (`cargo test -p haul-app`). Expected: all pass each time.
- [ ] **Step 4: Edge test to Finished.** Replace the `toContainText('Complete', …)` wait with the `firefox.spec.ts` pattern: click the Views navigation's `Finished` button, then wait (90 s) for a row in the treegrid named "Finished downloads" that has the test file's name. Run `npx tsc --noEmit -p app` (or `npm run check` in `app/`) to make sure the spec still type-checks. The real run is in Task 6's gate.
- [ ] **Step 5: Commit** `test: capture test waits for post-processing; Edge real test checks Finished`.

---

### Task 2: "When done" safety

**Files:**
- Modify: `app/src-tauri/src/when_done.rs` (`Idle`, `should_fire`, `WhenDone`)
- Modify: `app/src-tauri/src/core.rs:496-575` (`when_done_*`, `idle`)
- Modify: `app/src-tauri/src/lib.rs:80` (pass the id)
- Test: `when_done.rs` tests module and `core.rs` tests (existing `FakePower`, `core_fake`)

**Interfaces:**
- Produces: `WhenDone::on_started(&mut self, id: DownloadId)`; `AppCore::when_done_started(&self, id: DownloadId)`; `WhenDone::watched(&self) -> &HashSet<DownloadId>`; `WhenDone::counting(&self) -> bool`; `Idle { running_or_queued, postproc_busy, postproc_waiting_on_user, held_by_schedule, stopped_by_hand: bool }`; `AppCore::idle_for(&self, watched: &HashSet<DownloadId>) -> Idle` (replaces `idle()`; `idle()` stays as `idle_for(&HashSet::new())` for its existing callers/tests).

- [ ] **Step 1: Failing tests (pure, `when_done.rs`).**
  - `stopped_by_hand_blocks_fire`: `should_fire(Mode::Shutdown, true, &Idle { stopped_by_hand: true, ..Default::default() }) == false`.
  - `set_mode_clears_watched`: `on_started(7)` with Sleep chosen → `watched()` contains 7; `set_mode(Mode::Shutdown)` → `watched()` empty.
  - `on_started_with_nothing_watches_nothing`: mode Nothing, `on_started(7)` → `watched()` empty.
- [ ] **Step 2: Failing tests (`core.rs`).**
  - `pause_all_does_not_start_countdown`: with `core_fake`, insert a download row, `set_when_done(Shutdown)`, call `when_done_started(id)`, set the row Paused with `store.set_state`, then call `when_done_step()` with an `Instant` past `SETTLE` (step twice, the second time via `self.wd().step(&idle, now + SETTLE)` through a `#[cfg(test)] when_done_step_at(now)` helper); expect `Step::Nothing` and `when_done_status().countdown == None`.
  - `removed_paused_download_unblocks` (Review Focus 1): same as above, then `store.delete_download(id)`; stepping past `SETTLE` returns `Step::Started`.
  - `checksum_mismatch_blocks`: watched download id with a run row whose `state = "failed"` and whose `steps_json` has the Verify step `failed` (insert through the store's run API the postproc tests use) → `idle_for(&{id}).postproc_waiting_on_user == true`; the same run for an unwatched id → false.
  - `do_it_now_without_countdown_does_nothing`: `set_when_done(Sleep)`, no countdown started, `when_done_now().await` → `FakePower` recorded no call and mode is still Sleep.
  - `nothing_mode_skips_reads`: add `#[cfg(test)] idle_reads: Arc<AtomicUsize>` to `AppCore`, incremented in `idle_for`; with mode Nothing, ten `when_done_step()` calls leave it at 0.
  - Run `cargo test -p haul-app when_done`. Expected: compile errors first (new names), then each test FAILs for its reason once stubs exist.
- [ ] **Step 3: Implement.**
  - `WhenDone` gets `watched: HashSet<DownloadId>`; `on_started(id)` inserts when mode ≠ Nothing; `set_mode` clears it; `counting()` = `countdown.active()`.
  - `should_fire` also requires `!idle.stopped_by_hand`. In `step`, `stopped_by_hand` cancels a running countdown like `postproc_waiting_on_user` does.
  - `idle_for(watched)`: `stopped_by_hand` = some watched id is Paused and not `held_by_schedule`'s case (schedule closed). `postproc_waiting_on_user` also true for a `failed` run whose `download_id` is watched, or whose `package_id` equals a watched download's `package_id`, and whose parsed `RunData.steps` has `StepKind::Verify` with `StepState::Failed`.
  - `when_done_step`: if mode is Nothing return `Step::Nothing` before reading anything; else `idle_for(&watched)` (clone the set out of the lock first; never hold the `WhenDone` lock during store reads).
  - `when_done_now`: take the mode only if `counting()`; otherwise return `Ok(())` and leave the mode.
  - `lib.rs:80`: `Ok(Event::StateChanged { id, state: DownloadState::Running, .. }) => core.when_done_started(id)`.
- [ ] **Step 4: Run** `cargo test -p haul-app` and `npx vitest run when-done` + `npx playwright test when-done` in `app/` (the UI's Do it now still works when the countdown shows). Expected: PASS.
- [ ] **Step 5: Commit** `fix(when-done): wait for the downloads that ran; checksum mismatch blocks; Do it now needs the countdown`.

---

### Task 3: engine correctness

**Files:**
- Modify: `crates/engine/src/store/mod.rs` (new `set_target_path`; pragma in `open`; SQL at line 259)
- Modify: `crates/engine/src/transfer/mod.rs:~274, 360-375`
- Modify: `crates/engine/src/queue.rs:284-291`
- Modify: `crates/engine/src/error.rs:~45-55`
- Modify: `app/src-tauri/src/core.rs:87-90` (`in_use`)
- Test: `crates/engine/tests/resume.rs`, `crates/engine/tests/queue.rs`, `crates/engine/src/error.rs` tests, `crates/engine/src/store/mod.rs` tests, `core.rs` tests

**Interfaces:**
- Produces: `Store::set_target_path(&self, id: DownloadId, path: Option<&Path>) -> Result<(), EngineError>`.

- [ ] **Step 1 (3a): failing tests in `tests/resume.rs`.**
  - `finished_file_found_after_crash_completes`: run a download to Completed with the support server; then simulate the crash: `set_state(id, Running)`, keep `target_path`, `save_segments(id, &[])`. Restart the queue (as `restart_resumes_interrupted` does). Expect the row Completed at the same `target_path`, no `name (1)` file in the folder, and the server's request count unchanged after restart (no new GET).
  - `saved_target_with_wrong_size_downloads_again` (Review Focus 2): same, but truncate the target file to 10 bytes before restarting. Expect Completed with a full-size file (at `name (1)` or the same path, whichever `reserve_unique_path` gives) and at least one new GET.
  - Run: `cargo test -p haul-engine --test resume finished_file_found`. Expected: FAIL (a `(1)` copy appears).
- [ ] **Step 2 (3a): implement.** `set_target_path` updates only `target_path`. In `transfer()`, right after `reserve_unique_path` and before `rename`, call `set_target_path(id, Some(&target))`; on rename failure, `set_target_path(id, None)`. At the start of `transfer()` (before the probe): if `d.target_path` is `Some(t)`, the part file `part_path(&d.target_dir, name, id)` (name from `d.file_name`) doesn't exist, and `t` is a file whose length equals `d.probe.size` (`Some`), then `set_completed(id, &t)`, `save_segments(id, &[])` and return `Ok(Some(t))`. Otherwise go on as today. Run the resume tests: PASS.
- [ ] **Step 3 (3b): failing test in `tests/queue.rs`.** `hand_pause_during_window_close_stays_paused`: start a slow download, call `queue.pause(id)` and, before its transfer has stopped, set a schedule whose window is closed and call the queue's dispatch tick (use the existing test hook the schedule tests use to force a tick); then open the window and tick again. Expect the row stays Paused. Run: FAIL (it is Queued again).
- [ ] **Step 4 (3b): implement.** In `dispatch`, skip `r.cancel.is_cancelled()` entries when filling `auto_paused`. Run: PASS; rerun `hand_pause_beats_schedule`.
- [ ] **Step 5 (3c): failing test in `error.rs`.** `network_error_hides_userinfo`: make a reqwest error for `http://user:secret@127.0.0.1:1/f.bin` (connect to a closed port with `http_client()`), convert to `EngineError`; `to_string()` contains `127.0.0.1:1/f.bin` and does not contain `secret` or `user:`. Run: FAIL.
- [ ] **Step 6 (3c): implement.** In the `From<reqwest::Error>` path, before `e.to_string()`, if `e.url_mut()` is `Some(u)`, `let _ = u.set_password(None); let _ = u.set_username("");`. Run: PASS.
- [ ] **Step 7 (3d): failing tests in `store/mod.rs` tests.**
  - `secure_delete_is_on`: `Store::open(tempfile)` → `pragma_query_value("secure_delete")` == 1. FAIL.
  - `migration_5_seals_plain_headers`: build a schema-4 DB the way the existing migration tests do (apply migrations 1–4 by hand), insert a download with plain-text `headers_json` `{"Cookie":"sid=1"}`, open with `Store::open`; `get_download(id).headers_json` contains `sid=1` and the raw `headers_blob` column bytes do not contain `sid=1`. (May pass at once: if so, record a `Ruling:` that it guards existing behaviour.)
- [ ] **Step 8 (3d): implement** `conn.pragma_update(None, "secure_delete", "ON")?` in `Store::open` and `open_in_memory`. Also fix the stray spaces at `store/mod.rs:259` (spec 6b; it lives here). Run: PASS.
- [ ] **Step 9 (3e): failing test in `core.rs` tests.** `locked_file_reports_in_use`: create a temp file, open it with `std::os::windows::fs::OpenOptionsExt::share_mode(0)` and keep the handle; call `RecycleBin.recycle(&[path])`; print the error in the assertion message; assert `in_use(&err)`. Run with `--nocapture`: FAIL, and note the real message in the ledger.
- [ ] **Step 10 (3e): implement.** Extend `in_use` to also match the message seen in Step 9 (and `"in use by another process"`, `"open in another program"`), case-insensitive. Run: PASS. Keep the existing `os error 32` test.
- [ ] **Step 11: Run** `cargo test --workspace --no-fail-fast` and clippy. **Commit** after each pair of steps (3a, 3b, 3c, 3d, 3e: five commits, `fix(engine): …`).

---

### Task 4: update check, release scripts and autostart

**Files:**
- Modify: `app/src-tauri/src/update.rs`
- Modify: `app/src/lib/update-state.svelte.ts`, test `update-state.svelte.test.ts`
- Modify: `extension/sign.mjs`, `extension/check-release.mjs`, test `check-release.test.ts`; create `extension/sign.test.ts`
- Modify: `app/src-tauri/src/commands.rs:175`, `app/src/lib/backend-mock.ts:662`
- Modify: `app/src-tauri/src/autostart.rs:56-62`

**Interfaces:**
- Produces: setting key `update_found` = `{"latest": "X.Y.Z", "url": "https://…"}`; `Updater::load_found(&self, store: &Store)`; `export function distHash(dir: string): string` in `check-release.mjs` (SHA-256 hex over the sorted relative paths and file bytes); the `.version` file becomes two lines: `version\nsha256\n`; `checkRelease(dir, version, distDir)`.

- [ ] **Step 1 (4a, 4b, 4d): failing Rust tests in `update.rs`.**
  - `failed_check_keeps_found_update`: feed returns a newer release, `check_now`; feed switched to 500, `check_now` again → `latest == Some("9.9.9")`, `url` kept, `error` is `Some`.
  - `found_update_survives_restart`: after a found update, a new `Updater` on the same store with `load_found` → `status().latest == Some("9.9.9")`.
  - `saved_update_cleared_once_installed` (Review Focus 3): saved `update_found` 0.2.0, new `Updater` with current `0.2.0` → `load_found` leaves `latest == None` and deletes/clears the key.
  - `release_url_cannot_climb`: `safe_release_url(Some("https://github.com/meraaki410/haul/releases/../../evil"))`, `Some("https://github.com.evil.io/meraaki410/haul/releases/x")`, `Some("http://github.com/meraaki410/haul/releases/x")` all return `RELEASES`; `Some("https://github.com/meraaki410/haul/releases/tag/v0.2.0")` returns itself.
  - Run `cargo test -p haul-app update`. FAIL.
- [ ] **Step 2: implement.** `check_now` merges: on `error`, keep the previous `latest`/`url`. On a found newer release save `update_found`; on "up to date" clear it. `load_found` at `Updater` creation in `lib.rs`. `safe_release_url` parses with `reqwest::Url` (already a dependency), requires scheme `https`, host `github.com`, and the parsed (normalised) path starting with `/meraaki410/haul/releases/`. Run: PASS.
- [ ] **Step 3 (4c): failing Vitest** in `update-state.svelte.test.ts`: `open_release_failure_shows_toast`: mock `openRelease` rejects with `'boom'` → the app toast text is `Couldn't open the release page: boom`. FAIL. **Implement** the catch → toast. PASS.
- [ ] **Step 4 (4e–4g): failing Node tests.**
  - `check-release.test.ts`: `refuses_while_signed_scratch_exists` (`.signed/` present → message mentions `.signed`); `refuses_changed_code` (`.version` hash ≠ `distHash(distDir)` → message `The Firefox code changed since it was signed. Raise the version and run npm run sign:firefox.`); `accepts_matching_hash`; `distHash_is_stable` (two identical temp dirs → same hash; one byte changed → different).
  - `sign.test.ts`: `cleans_scratch_on_failure`: export `signFlow({ here, run })` from `sign.mjs` where `run` is the injected child runner; a fake `run` that fails the sign step leaves no `firefox/.signed`. `prints_waiting_line`: captured output contains `Uploaded to Mozilla; waiting for review (this can take a few minutes)…` before the sign step runs.
  - Run `npm test` in `extension/`. FAIL.
- [ ] **Step 5: implement.** `sign.mjs`: wrap the work after `mkdirSync(out)` in `try/finally { rmSync(out, …) }`; print the waiting line; run web-ext with `spawn` (no shell), writing each stdout/stderr chunk through as it arrives and also keeping it, so the "version already exists" hint still works; `signFlow` returns a Promise and `main` awaits it. Write `.version` as `version\n<distHash(dist-firefox)>\n`. `check-release.mjs`: refuse when `.signed` exists; read both lines; compare the hash with `distHash(join(here, 'dist-firefox'))`. Verify the build is repeatable: run `npm run build` twice and compare `distHash` (by hand, recorded in the ledger). Run: PASS.
- [ ] **Step 6 (4h): failing Rust test** in `commands.rs` (or move the message into a pure fn `firefox_missing_message(installed: bool) -> &'static str` in `commands.rs` and test it): installed → `The Firefox add-on is missing from Haul's folder. Reinstall Haul to get it back.`; dev → the current npm hint. "Installed" = `!cfg!(debug_assertions)`. FAIL, implement, PASS. The mock keeps the dev text.
- [ ] **Step 7 (4i): failing Rust test** in `autostart.rs`: `enable_with_wrong_typed_mark`: write `StartupApproved\Run\<test name>` as `REG_SZ` "x" under the test root (`Autostart::at`), `enable(exe)` → `Ok`, the Run value is written and the wrong-typed mark is gone. FAIL. **Implement**: when `read_binary` errors with `ERROR_UNSUPPORTED_TYPE` (or any type error), `delete_value` it and continue. PASS.
- [ ] **Step 8: Run** the Rust, `app/` Vitest and `extension/` tests. **Commit** in three commits: `fix(update): …`, `fix(release): …`, `fix(autostart): …`.

---

### Task 5: front-end polish

**Files:**
- Modify: `app/src/lib/app-state.svelte.ts`, `collector-state.svelte.ts`, `format.ts`, `postproc.ts`; `app/src/components/SettingsView.svelte`, `CollectorView.svelte`
- Create: `app/src/lib/settings-num.ts` + `settings-num.test.ts`
- Modify: `app/src/lib/backend-mock.ts` (failure hook)
- Modify: `crates/engine/src/postproc/mod.rs:57-61, 565, 952` (`Notice.kind`), `app/src-tauri/src/toast.rs`, `app/src-tauri/src/lib.rs` (finished toast, startup box)
- Modify: `app/src-tauri/capabilities/default.json`, `app/package.json`
- Test: the matching `*.test.ts`, `app/e2e/remove-reorder.spec.ts`, `settings.spec.ts`, `collector.spec.ts`, `postproc.spec.ts`, Rust tests in `toast.rs` and `lib.rs`

**Interfaces:**
- Produces: mock `failNext(method: keyof Backend, message: string): void` (the next call of that method rejects with `message`); `parseWhole(text: string, min: number, max: number): number | null` in `settings-num.ts`; engine `pub enum NoticeKind { Ready, Attention }` and `Notice.kind`; `toast::finished(name: &str, folder: Option<PathBuf>) -> ToastSpec` (title `"<name> finished"`); `startup_failure_text(reason: &str, log_dir: &Path) -> String` in `lib.rs`.

- [ ] **Step 1 (5a): failing Vitest** in `app-state.svelte.test.ts` (with `failNext`):
  - `failed_remove_restores_rows`: `removeWithUndo([id])`, `failNext('remove','disk busy')`, run the timer → row visible again, toast `Couldn't remove: disk busy`.
  - `failed_reorder_reloads`: `failNext('reorder','x')` then a move → toast `Couldn't move: x`, `snapshot` reloaded (a `load` call recorded).
  - `failed_add_toasts`: `failNext('addLinks','x')` → toast `Couldn't add links: x`.
  - `failed_speed_limit_rolls_back`: `failNext('setSettings','x')`, `setSpeedLimit(new)` → `snapshot.speedLimit` equals the old value, toast `Couldn't change the speed limit: x`.
  - In `collector-state.svelte.test.ts`: `failed_collector_remove_restores` → links visible again, toast `Couldn't remove: x`.
  - Run `npx vitest run`. FAIL. **Implement** try/catch at each call; restore as stated. PASS. Add one e2e in `remove-reorder.spec.ts` (`remove_failure_shows_toast`) using a `?fail=remove` mock URL flag mapped to `failNext`. Run in both themes.
- [ ] **Step 2 (5b): failing Vitest** `settings-num.test.ts`: `parseWhole("5",1,20)==5`, `parseWhole(" 5 ",1,20)==5`, `parseWhole("",1,20)==null`, `"2.5"→null`, `"1e1"→null`, `"-3"→null`, `"0"→null`, `"21"→null`, `"20"→20`. FAIL. Implement (`/^\s*\d+\s*$/` then range). In `SettingsView.svelte` `num()`: on `null`, show `Enter a whole number from {min} to {max}.` under the field (the existing inline error slot), reset the input to the saved value, call no backend. Speed limit: reject `< 0.01` or non-number with `Enter a speed of 0.01 MB/s or more.` e2e `settings.spec.ts`: `number_field_rejects_empty` (clear "Downloads at once", blur → the message shows, value back to the saved one). PASS.
- [ ] **Step 3 (5c, 5d): failing Rust tests.**
  - `toast.rs`: `attention_kind_ignores_switch` (a `Notice` with `kind: Attention` and a title not ending in "needs attention" shows with the switch off); `ready_kind_respects_switch`; `finished_toast_has_open_folder` (`finished("a.bin", Some(dir))` → title `a.bin finished`, `open_folder == Some(dir)`).
  - In `core.rs` tests: `completed_with_auto_off_sends_finished_notice`: auto post-processing off (settings patch), a completed download → the injected notify sink got one `Notice { kind: Ready, title: "<name> finished" }` (route it through the same `deps.notify` sink so the switch logic stays in one place).
  - FAIL. **Implement**: add `kind` to `Notice` (set `Attention` at mod.rs:565, `Ready` at 952); `should_show` uses `n.kind == NoticeKind::Attention`; when a download completes and `postproc.auto` is off, the app sends `Notice { kind: Ready, title: format!("{name} finished"), body: String::new(), folder: parent }` through `deps.notify`. PASS.
- [ ] **Step 4 (5e): failing Vitest.**
  - `app-state.svelte.test.ts` `hidden_rows_skip_total_speed`: two running rows with speeds 100 and 200, hide one via `removeWithUndo` → the last `speedHistory` sample is 100 (or 200, the one not hidden).
  - `last_steal_pruned`: a steal for id 5, then a snapshot without 5 → `lastSteal.has(5) == false`.
  - `format.test.ts`: `formatProgress(0, 0) == '0.0%'`, `formatProgress(0, 0, true) == '100.0%'` (add `done = false` as a third parameter to `formatProgress` at `format.ts:31`; callers pass `state === 'completed'`).
  - `postproc.test.ts`: `packageWord({state:'held', …}) == 'Not processed'`.
  - FAIL. Implement each. Update any e2e that expects "Waiting to process" (`postproc.spec.ts`). PASS.
- [ ] **Step 5 (5f): failing e2e** in `collector.spec.ts`: `enter_in_select_does_not_start` (focus a `<select>` inside the Collector, e.g. add one through the page in the test with `page.evaluate`, press Enter → no `collectorStart` call in the mock's `calls`); `remove_expanded_package_selects_next` (expand package A of A,B; Delete → B is selected, `aria-selected="true"`). FAIL. **Implement**: `CollectorView.svelte:40` also skips `SELECT` and `target.isContentEditable`; `removeSelected` picks the next package key (else previous) before removing. PASS.
- [ ] **Step 6 (5g):** remove `"opener:default"` from `capabilities/default.json` and `@tauri-apps/plugin-opener` from `app/package.json` (`npm uninstall @tauri-apps/plugin-opener` in `app/`). Check: `grep -r "plugin-opener" app/src` is empty; `cargo test -p haul-app` (the capability config tests) and `npm run check` pass. No new test (removal); ledger note.
- [ ] **Step 7 (5h): failing Rust test** in `lib.rs`: `startup_failure_text_names_reason_and_log` → `startup_failure_text("database is locked", Path::new(r"C:\L"))` == `"Haul couldn't start: database is locked. The log is in C:\\L."`. FAIL. **Implement** the fn; in `run()`, replace `.expect("error while building Haul")` and the setup's fatal errors with: log the error, `MessageBoxW(null, text, "Haul", MB_OK | MB_ICONERROR)` (windows-sys, already a dependency; enable its `Win32_UI_WindowsAndMessaging` feature if missing), `std::process::exit(1)`. PASS. Hand check later (Task 6 Step 6).
- [ ] **Step 8: Run** `npx vitest run`, `npm run check`, `npx playwright test` in `app/`, and the Rust tests. **Commit** per step group: `fix(ui): failed actions say so`, `fix(settings): number fields`, `fix(toasts): kind and finished toast`, `fix(ui): small display fixes`, `fix(collector): keyboard and selection`, `chore: drop the opener capability`, `fix(app): startup failure message`.

---

### Task 6: privacy and tidiness, then the gate and hand-over

**Files:**
- Modify: `app/src-tauri/src/api/pairing.rs` (or wherever the browser name enters: `capture.rs` pairing wrappers), `crates/engine/src/store/clients.rs:31-37`
- Modify: `app/src-tauri/src/lib.rs:234` area (`api-port` removal on exit)
- Modify: `app/src-tauri/src/clipboard.rs`
- Modify: `crates/engine/src/grabber/mod.rs:176-191, 414-420`
- Modify: `extension/src/cookies.ts`, test `extension/src/cookies.test.ts`
- Modify: `handoff.md`

**Interfaces:**
- Produces: `clean_browser_name(raw: &str) -> String` (control chars removed, trimmed, cut to 64 chars, empty → `"Unknown browser"`); `Store::add_paired_client` replaces an existing row with the same `origin` (returns the id); `fn clipboard_excluded() -> bool` in `clipboard.rs`; the Grabber's `probe_gen: Mutex<HashMap<i64, u64>>`.

- [ ] **Step 1 (6a): failing tests.** `pairing_name_strips_controls` (`clean_browser_name("Edge\u{1b}[31m\n")=="Edge[31m"`), `pairing_name_cut_to_64`, `pairing_name_all_control_chars` (Review Focus 5: `"\n\t"` → `"Unknown browser"`); `clients.rs` `same_origin_replaces_row` (add twice with origin `chrome-extension://abc` and different hashes → one row, only the second hash finds it). FAIL. Implement; call `clean_browser_name` where the pair request's name is received, before the dialog event and the log line. PASS. Commit `fix(pairing): …`.
- [ ] **Step 2 (6b): `api-port` removal.** Test-first if the exit path is reachable from a unit (extract `remove_port_file(data_dir: &Path)` and test it removes the file and ignores a missing one); call it from the app's exit handler. Commit `fix(api): remove api-port on quit`.
- [ ] **Step 3 (6c): clipboard exclusion.** Extract the decision `fn should_read(excluded_format: bool, history_flag: Option<u32>) -> bool` and test: `(true, None)→false`, `(false, Some(0))→false`, `(false, Some(1))→true`, `(false, None)→true`. FAIL, implement. `clipboard_excluded()` uses `RegisterClipboardFormatW` for `ExcludeClipboardContentFromMonitorProcessing` and `CanIncludeInClipboardHistory`, `IsClipboardFormatAvailable`, and for the latter reads its DWORD with `OpenClipboard`/`GetClipboardData` (close the clipboard on every path). The watcher calls it before reading text. Commit `fix(clipboard): respect password managers`.
- [ ] **Step 4 (6d): failing tests in `crates/engine/tests/grabber.rs`.**
  - `removed_link_is_not_checked`: a prober that counts calls and blocks the first `PROBE_SLOTS` calls on a gate; intake `PROBE_SLOTS + 1` links; remove the last one; open the gate; after the checks settle, the counter is `PROBE_SLOTS`.
  - `newer_recheck_wins`: a prober whose first call for a URL sleeps 300 ms and returns Offline, and whose second returns Online immediately; intake, then `recheck` at once → final status Online.
  - FAIL. Implement: after the permit, skip when `store.link_exists(id)` is false (add `Store::link_exists(&self, id: i64) -> Result<bool, EngineError>`); `spawn_probe` bumps `probe_gen[id]` and saves the result only if the generation is unchanged. PASS. Commit `fix(collector): …`.
- [ ] **Step 5 (6e): failing extension test** in `cookies.test.ts`: `first_party_isolation_retries_with_domain`: a fake `getAll` that throws `Error('First-Party Isolation is enabled, but the required \'firstPartyDomain\' attribute was not set.')` unless `firstPartyDomain` is given, and returns `[{name:'sid', value:'1'}]` when `firstPartyDomain == 'example.com'`; `cookieHeader(api, 'https://dl.example.com/f.bin', { cookieStoreId: 'firefox-default', referrer: 'https://www.example.com/page' })` → `'sid=1'`. FAIL. Implement: on an error whose message mentions `firstPartyDomain`, retry once with `{ url, storeId, firstPartyDomain: site }` where `site` is the referrer's host with a leading `www.` removed and at most its last two labels kept (the URL's host when there's no referrer). PASS. `npm test`, `npm run check`, `npm run build` in `extension/`. Commit `fix(firefox): cookies under first-party isolation`.
- [ ] **Step 6: Hand checks with the owner** (ask before each):
  - 5h: in the owner's terminal, with dev Haul quit: rename `%APPDATA%\dev.haul.desktop.dev\haul.db` to `haul.db.bak` and create an empty folder named `haul.db`; start `npx tauri dev`; expect the "Haul couldn't start" box. Then remove the empty folder and rename `haul.db.bak` back (nothing is deleted).
  - 6c: owner copies a password from their password manager with Watch clipboard on; nothing appears in the Collector.
  - Edge real test: owner quits the installed Haul; run `npx playwright test -c playwright.real.config.ts` (both specs) with the range server and `tauri dev` on port 9222.
- [ ] **Step 7: Full gate** per `docs/release-checklist.md` §1 (extension build; `cargo test --workspace --no-fail-fast`; clippy `-D warnings`; `cargo fmt --check`; in `app/`: `npx vitest run`, `npm run check`, `npx playwright test`; in `extension/`: `npm test`, `npm run check`; the real-browser run from Step 6). All green, flakes rerun alone.
- [ ] **Step 8: Fresh whole-branch review** (one reviewer on the whole `master..feat/cleanup` diff). Fix Critical/Important findings test-first; list the rest as Plan 8 deferred minors.
- [ ] **Step 9: `handoff.md`.** Add a "Plan 8: clean-up pass (9 Nov 2026)" section (docs, what was fixed, the dropped list with reasons, review result, test counts); remove the closed items from the old deferred lists or mark each "fixed in Plan 8"; update "Where things stand". Commit `docs: Plan 8 handoff`.
- [ ] **Step 10: Merge** only after the owner agrees: fast-forward `master` to `feat/cleanup` from the main checkout, delete the branch and the worktree.
