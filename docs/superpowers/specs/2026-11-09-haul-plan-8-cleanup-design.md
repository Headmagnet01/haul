# Haul Plan 8: the clean-up pass (design)

Date: 9 Nov 2026. Branch `feat/cleanup` in the worktree `.claude/worktrees/plan8`, from `master` d98ae5e.

## Purpose

Plan 8 adds no features. It fixes the deferred minor issues from Plans 1–7 that are worth fixing now, records why the rest are dropped, and repairs two tests that give wrong signals (one flaky, one stale). Afterwards `handoff.md` should hold no open "deferred minors" list, only the dropped list below with its reasons.

**Owner decisions (9 Nov):** the cut below ("Yes, this cut"): six tasks, the drops as listed.

**Done when:** every fix below is merged into `master` with a test that failed first (or, where a test can't fail first, the reason is recorded in the ledger), the full gate in `docs/release-checklist.md` §1 is green, and `handoff.md` has a Plan 8 section.

## Rules for every fix

- Test-first: write the test, watch it fail for the right reason, then fix.
- One commit per fix or per small group of related fixes. Ledger at `.superpowers/sdd/2026-11-09-haul-plan-8-cleanup/progress.md` (git-ignored), with each deviation as a `Ruling:` line.
- No new dependencies unless a fix below names one.
- Error text shown to the user is plain English and says what to do.

---

## Task 1: tests that give wrong signals

### 1a. The flaky `capture::tests::collector_capture_starts_with_cookies`

**Cause (found 9 Nov, before writing this spec).** The test waits until the download row says Completed and then reads that row's `target_path`. Completing a download starts a post-processing run, and its sort step moves the file into a category folder and saves the new path (`postproc/mod.rs`, `sort` → `store.set_completed(id, &new)`). The test holds the old path, so when the sort wins the race, `metadata(path)` fails with "file not found". The app is fine; the test reads a stale path.

**Fix.** Prove the cause first: put a short sleep between reading the row and checking the file, which should make it fail every time. Then change the test to wait until the download's post-processing run has finished (state `done`, `failed` or `skipped`), re-read the download, and check the file at the path the row now holds. The cookie check (the point of the test) stays the same.

### 1b. The stale Edge real test `app/e2e-real/extension.spec.ts`

Since Plan 6a a completed download leaves Queue for Finished, but this test still waits for "Complete" in Queue. Change it to the pattern in `firefox.spec.ts`: open the Finished view and find the row in the "Finished downloads" tree grid. It runs only with the installed Haul closed (it would pair with the installed copy on 17654), so the owner is asked to quit it first.

---

## Task 2: "When done" safety

"When done" can sleep or shut down the PC, so it must fire only when the work it was waiting for really finished.

### 2a. It waits for the downloads that ran, not just for an idle queue

**Problem.** Today "armed" means "something started since the mode was chosen". Pause all (or pausing the last running download by hand) makes the queue idle, so the countdown starts and the PC shuts down with downloads half done. This is also the Plan 1 minor "`AllDone` fires when the user pauses every download".

**Fix.** `WhenDone` remembers the ids of the downloads seen starting since the mode was chosen (it already gets `StateChanged { id, Running }`). `Idle` gains `stopped_by_hand: bool`: one of those downloads is now Paused (and the schedule isn't the one holding it). `should_fire` refuses while it is true. A removed download simply drops out. Choosing a mode again clears the list.

**Result.** Pause all → no countdown; resuming them and letting them finish → countdown as before. Failed downloads still don't block it (spec 6a §5).

### 2b. A checksum mismatch waits for you

**Problem.** Spec 6a §5 says When done must not fire while a post-processing run waits on the user, naming a checksum mismatch. Only "needs password" blocks today; a mismatch ends as a `failed` run and doesn't.

**Fix.** `Idle.postproc_waiting_on_user` is also true when a run for one of the downloads remembered in 2a has failed at its verify step. Older failed runs from before the mode was chosen don't count, so a mismatch from yesterday can't block tonight's shutdown.

### 2c. Do it now only acts while the countdown is showing

**Problem.** If a download starts in the moment between the countdown showing and the click, Do it now still sleeps or shuts down.

**Fix.** `when_done_now` carries out the mode only while the countdown is running. If new work stopped the countdown first, the click does nothing and the window shows the plain status again (the mode stays chosen, so the countdown starts once the new work is done).

### 2d. No work every second when nothing is chosen

**Problem.** `when_done_step` reads every download and every run once a second, even with "Nothing" chosen.

**Fix.** Check the mode first; with Nothing it returns at once without reading the database.

---

## Task 3: engine correctness

### 3a. A crash while finishing doesn't download the file again

**Problem.** A crash between the final rename (`.haulpart` → name) and `set_completed` leaves the row Running with no part file, so recovery downloads it again as `name (1)`.

**Fix.** Save the chosen target path on the row before the rename. When a transfer starts and the part file is missing, but the row has a saved target path to a file of the expected size, mark the row Completed at that path instead of downloading again. If the size is unknown or doesn't match, download as today.

### 3b. A hand pause as the window closes stays paused

**Problem.** If you pause a download just as the schedule's window closes, a dispatch tick can still see its transfer (cancelled, not yet stopped) and add it to `auto_paused`, so the schedule restarts it later.

**Fix.** The dispatcher skips transfers whose cancel token is already cancelled when it fills `auto_paused`.

### 3c. Network errors never keep a password from the URL

**Problem.** `EngineError::Network` keeps reqwest's message, which includes the full URL with any `user:password@`. It reaches the debug log and the row's error text.

**Fix.** Strip the username and password from the URL inside the reqwest error before it becomes text (reqwest's `url_mut`), so the message keeps the host and path.

### 3d. Deleted secrets don't linger in the database file

**Problem.** SQLite leaves deleted data in free pages until they are reused, so wiped headers and removed passwords can stay in the file.

**Fix.** Open the database with `PRAGMA secure_delete = ON`, so deleted content is overwritten with zeros. Also add the missing test from Plan 5: a schema-4 database with plain-text headers migrates to the sealed form. (No one-off VACUUM: only the owner's dev database ever held plain-text headers, and their free pages get reused over time.)

### 3e. "In use" is recognised for the copy engine's wording too

**Problem.** Delete files says "<name> is in use" only for Windows' file-system wording ("being used by another process", errors 32 and 33). The Recycle Bin's copy engine uses its own wording ("in use by another process" / "open in another program"), which shows as raw OS text.

**Fix.** Add a test that holds a real temp file open without sharing and recycles it, see which message comes back, and recognise it. Keep the existing wordings.

---

## Task 4: update check, release scripts and autostart

### 4a. A failed check keeps an update already found

`check_now` replaces the whole status. When a check fails after an earlier one found an update, keep `latest` and `url` and add the error.

### 4b. A found update survives a restart

Save the found update (`latest`, `url`) with the check time. At start, load it and show it if it is still newer than the running version, so a restart doesn't hide it for up to 24 h. Installing that version (current ≥ saved) clears it.

### 4c. Open release says when it fails

If opening the release page fails (from the status bar or Settings › App), show a toast "Couldn't open the release page: <reason>" instead of nothing.

### 4d. Release URLs can't climb out of Haul's releases

`safe_release_url` checks a prefix today, so `https://github.com/meraaki410/haul/releases/../../other` passes. Parse the URL, and accept it only when the scheme is https, the host is exactly `github.com`, and the normalised path is under `/meraaki410/haul/releases/`.

### 4e. A failed sign leaves nothing to bundle

The installer ships the whole `extension/firefox/` folder. If `sign:firefox` fails, its scratch folder `.signed/` stays and would be bundled. Remove it on every exit path, and make `check-release` refuse to bundle while `.signed/` exists.

### 4f. `sign:firefox` says what it is waiting for

Print "Uploaded to Mozilla; waiting for review (this can take a few minutes)…" before web-ext runs, and pass its output through as it comes instead of after it ends.

### 4g. `check-release` spots an add-on built from older code

`sign:firefox` writes a SHA-256 of the `dist-firefox` files it signed beside the version. `check-release` (run after the release build rebuilt the extension) compares it with the current `dist-firefox` and refuses with "The Firefox code changed since it was signed. Raise the version and run npm run sign:firefox." Needs the build to be repeatable; a test builds twice and compares.

### 4h. The open-folder error fits installed users

"The Firefox add-on wasn't found" tells installed users to run an npm command. Installed builds say "The Firefox add-on is missing from Haul's folder. Reinstall Haul to get it back."; dev builds keep the npm hint.

### 4i. A wrong-typed Task Manager mark doesn't break autostart

`enable` reads `StartupApproved\Run\Haul` as binary; any other type fails the whole switch. Treat a value of the wrong type as no valid mark: delete it and carry on.

---

## Task 5: front-end polish

### 5a. Failed actions say so

Today remove, reorder/priority, add links, the speed limit and Collector remove fail silently. Each shows a toast "Couldn't <action>: <reason>" and puts things back: removed rows reappear, a moved row reloads to its saved place, the speed limit returns to the saved value, Collector links reappear.

### 5b. Settings number fields check what you typed

Empty, non-whole or out-of-range values in "Downloads at once" (1–20) and "Connections per download" (1–16) show "Enter a whole number from 1 to 20." under the field and go back to the saved value, without calling the backend. The speed limit accepts 0.01 or more.

### 5c. A finished toast with auto post-processing off

With auto post-processing off no run happens, so no "<name> is ready" toast is ever sent. Then a completed download sends "<name> finished" (with Open folder), under the same "notify when finished" switch.

### 5d. Toasts are filtered by kind, not wording

`should_show` checks the title ends with "needs attention". The engine's `Notice` gets a `kind` (`Ready`, `Attention`), and the filter uses it.

### 5e. Small display fixes

- Rows waiting in the 6 s Undo window don't count toward the total speed.
- `lastSteal` drops entries for downloads that are gone.
- A file whose size is known to be 0 bytes shows 0% until it completes, not 100%.
- With auto post-processing off, a finished row says "Not processed" (with its Process button), not "Waiting to process".

### 5f. Keyboard and selection

- Enter in a `<select>` or editable element in the Collector no longer starts the selection (check `isContentEditable` and `SELECT` too).
- Removing an expanded Collector package selects the next package (or the previous one at the end).

### 5g. The window has no permission it doesn't use

Remove `opener:default` from `capabilities/default.json` and the `@tauri-apps/plugin-opener` JS package. Rust keeps the plugin, which is what opens folders and pages.

### 5h. A startup failure shows a message

If Haul can't start (the database can't open, Tauri fails to build), show a Windows message box "Haul couldn't start: <reason>. The log is in <folder>." and exit, instead of a silent panic.

---

## Task 6: small privacy and tidiness items

### 6a. Pairing

- The browser name an extension sends has control characters removed and is cut to 64 characters before it is shown or logged.
- Pairing again from the same origin (the same extension) replaces that row and its key instead of adding a second one.

### 6b. Tidy-ups

- The `api-port` file is deleted when Haul quits.
- The stray run of spaces in the store SQL string is fixed.

### 6c. The clipboard watcher respects password managers

When the clipboard holds the `ExcludeClipboardContentFromMonitorProcessing` format (set by password managers), or `CanIncludeInClipboardHistory` is 0, the watcher ignores that copy.

### 6d. Collector checks

- A link removed while its check waits in line is not checked: the check task looks the link up after it gets its slot and stops if it is gone.
- Recheck: each check carries a number per link, and only the newest one's result is saved, so an older check can't overwrite a newer one.

### 6e. Firefox first-party isolation

With `privacy.firstparty.isolate` on, `cookies.getAll` without `firstPartyDomain` throws, so the hand-over sends no cookies. On that error, ask again with `firstPartyDomain` set to the referrer's site (or the URL's site when there is no referrer).

---

## Dropped, with reasons

- **Harmless or accepted trade-offs:** stalls under a speed limit wait for the 30 s read timeout (Plan 1); the pause also waits for the cookie read (Plan 5); a capture over 1.5 s that was still queued downloads twice (Plan 5, never-lose trade-off); a server slower than 1.5 s to send headers leaves a download in Firefox (Plan 7, same trade-off); the extension rescans only on a dead port (Plan 5); the 1 MB clipboard limit is checked after reading (Plan 3).
- **Rare and costly to fix:** a non-archive package started one link at a time can split; `queue.add` failing partway leaves links in both places; a package's key changes when a check learns the name; a re-capture can race the running probe (6b); the Firefox size wait reuses onCreated's name, state and Alt-clicks; "partitioned wins" can send an iframe-partition cookie for a top-level link (Plan 7).
- **Needs a design of its own:** the archive password reaches 7-Zip on its command line (visible only to the same user's programs); a run that failed for a missing part needs Retry once the part arrives.
- **Moot:** downloads finished before Plan 4 show Process instead of a run.
- **Already fixed:** autostart drift and `--hidden` (6b), `allow-close` in production (6b), shared dev/release data folder (6b), schedule vs. hand pause (7).

---

## Testing

- Rust unit tests for every Rust fix (Tasks 1a, 2, 3, 4a–d, 4h–i, 5c–d, 5h's message text, 6a–d).
- Vitest for the front-end logic (5a, 5b, 5e, 5f), Playwright e2e where a view changes (5a toasts, 5b inline error, 5e labels, 5f selection), in both themes as today.
- Node tests for the scripts (`check-release.test.ts`, a new sign test for 4e–g).
- Extension Vitest for 6e with a fake cookies API.
- Hand checks the tests can't cover, done once near the end: 5h (start Haul with a locked database and see the box), 6c (copy from a password manager with Watch clipboard on), 1b (the Edge real test, with the installed Haul quit).
- Then the full gate in `docs/release-checklist.md` §1, including both real-browser tests.

## Order

Task 1 first (the flaky test makes every later gate noisy), then 2, 3, 4, 5, 6. One fresh whole-branch review at the end, its fixes test-first, then the merge with the owner's agreement.
