# Haul plan 6a: settings, tray, "when done", Finished and first run

Design agreed with the owner on 8 Nov 2026. It refines sections 4, 7 and 10 of `2026-11-05-haul-design.md`; where the two differ, this file wins. Plan 6 is split: 6a (this file) makes Haul an everyday app in the dev build; 6b covers the installer, the update check and the release checklist.

## 1. Goal and scope

Haul can be left running: you set your folder, close the window and downloads carry on in the tray, finished files are found in Finished, and Haul can sleep or shut the PC down when the queue is done.

- **Success:**
  - Changing the download folder in Settings applies to the next download, with no restart.
  - Closing the window with a download running keeps it downloading; the tray shows Haul is busy.
  - A completed download leaves Queue and appears in Finished, where it can be opened, shown in its folder, removed from the list or sent to the Recycle Bin.
  - "Shut down" fires 60 s after the queue and post-processing are idle, and can be cancelled.
- **Out of scope:** the installer, update check, store listing of the extension, code signing (all 6b); themes (Haul follows Windows), proxy settings, languages.

## 2. Owner decisions

| Question | Decision |
|---|---|
| Plan size | Split into 6a and 6b. |
| The window's X button | Hides to the tray; a Settings switch makes it quit instead. A one-time notification says Haul is still running. |
| Start with Windows | Off by default; a Settings switch turns it on. Started this way, Haul opens hidden in the tray. |
| Completed downloads | Move from Queue to Finished. |
| How settings are saved | Approach A: one `Settings` record, one save command, every change applied live. Rejected: one command per setting (B); apply on restart (C). |

## 3. Settings

### 3.1 The record (Rust, `app/src-tauri`)
One `Settings` struct, serialised as camelCase, stored key by key in the existing `settings` table (keys `download_dir`, `max_parallel`, `speed_limit` and `watch_clipboard` keep their names and values, so existing data carries over).

| Field | Type, default | Range or rule | Applied |
|---|---|---|---|
| `downloadDir` | path, Windows Downloads | must exist and be writable (a probe file is created and deleted) | new downloads and new Collector starts; queued downloads keep their folder |
| `maxParallel` | 1–20, 3 | clamped | `Queue::set_max_parallel` |
| `connections` | 1–16, 8 | clamped | new downloads (replaces `DEFAULT_CONNECTIONS`) |
| `speedLimit` | `{enabled, bps}` | bps ≥ 1 KiB/s when enabled | global limiter, as today |
| `browserStart` | `"now"` or `"collector"`, `"now"` | | captures from the extension |
| `watchClipboard` | bool, false | | clipboard watcher, as today |
| `schedule` | `null` or `{start, stop, days}` | start ≠ stop; at least one day | `Queue::set_schedule` |
| `closeToTray` | bool, true | | the window's close handler |
| `autostart` | bool, false | | the autostart plugin's enable/disable |
| `notifyFinished` | bool, true | | notifications (section 6) |
| `trayHintShown` | bool, false | internal | set after the one-time tray notification |
| `firstDownloadSeen` | bool, false (true when the `downloads` table already has rows) | internal | set when the first download is added; hides the welcome (section 7) |

- `AppCore` keeps the record behind a `RwLock`; `download_dir` and the connection count are read from it at the moment they are needed, not copied at start.
- Commands: `settings() -> Settings` and `set_settings(patch) -> Settings`. A patch holds only changed fields. Haul validates, saves, applies, and returns the full record. An invalid field fails the whole call with a message naming the field; nothing is saved.
- The status bar's speed-limit and clipboard toggles call `set_settings`, so they and the Settings page never disagree. `set_speed_limit` and `set_watch_clipboard` are removed.
- Collector filters, rules, categories and passwords keep their own commands.
- If `autostart` fails to apply (the plugin errors), the switch stays off and the error is shown.

### 3.2 The page (`SettingsView.svelte`)
One scrolling column with headed groups, in the style of Rules. Each change saves on its own; a failure shows inline under the control and the control returns to the saved value.

- **Downloads:** download folder (path, **Change…** opens the Windows folder picker via `tauri-plugin-dialog`, **Open**); downloads at once; connections per download; speed limit value.
- **Adding links:** links from the browser (Start at once / Wait in the Collector); watch the clipboard.
- **Schedule:** Off, or start time, stop time and weekday toggles.
- **App:** close to tray; start with Windows; notify when a download finishes.
- **Browsers:** the paired browsers list with Remove, moved here from Rules.

## 4. Tray, close and single instance

- **Tray icon** (Tauri tray API), always present while Haul runs.
  - Tooltip: `Haul · 3 downloading · 7.0 MB/s`, or `Haul · idle`; updated at most once a second.
  - Icon: the Haul mark; a variant with a dot while anything is running.
  - Left-click shows and focuses the window.
  - Menu: Open Haul · Pause all / Resume all · Speed limit (check) · When done ▸ Nothing / Sleep / Shut down (radio) · Quit.
- **Close:** with `closeToTray` on, the close request is prevented and the window hidden; everything keeps running (queue, post-processing, the local API, the clipboard watcher). The first time, a notification says "Haul is still running in the tray" and `trayHintShown` is set. With it off, close does what it does today: pause, save, quit.
- **Quit** (tray menu, or close with the switch off) runs the existing shutdown path.
- **Single instance** (`tauri-plugin-single-instance`): a second launch exits at once and the running copy shows and focuses its window.
- **Autostart** (`tauri-plugin-autostart`): the entry passes `--hidden`; a launch with `--hidden` creates the window hidden.

## 5. "When done"

- Values: Nothing, Sleep, Shut down (the engine's `WhenDone`). Set from the status bar selector or the tray submenu. Not saved: it resets to Nothing after it fires, when cancelled, and when Haul starts.
- **Fires when** both are true: the queue has nothing running or queued, and post-processing has no run in progress. Checked on `Event::AllDone` and when a post-processing run ends.
  - Does not fire while downloads are held by the schedule, or a post-processing run waits on the user (archive password, checksum mismatch).
  - Failed downloads don't block it.
  - Choosing an option while everything is already idle does not fire it; something must run and finish first.
- **Countdown:** Haul shows its window and a dialog: "Shutting down in 60 s" (or "Going to sleep…"), buttons **Cancel** (default; Esc and Enter) and **Do it now**. Cancel resets to Nothing. Anything starting during the countdown cancels it.
- **Action:** Sleep calls `SetSuspendState(FALSE, FALSE, FALSE)` and Haul keeps running. Shut down runs the shutdown path (pause and save), then `shutdown /s /t 0`.
- The decision is a pure function (`when_done::should_fire(state) -> bool` plus a small countdown state machine) with unit tests. The power call goes through a `PowerAction` trait; tests use a recording fake.

## 6. Finished view and notifications

### 6.1 Data
- Migration 6 adds `downloads.finished_at TEXT`, set when a download becomes Completed (and cleared if it is restarted). Older completed rows keep NULL and sort by `created_at`.
- The snapshot gains `finishedAt` per download.

### 6.2 What is in Finished
- A completed download with no package moves to Finished when it completes.
- A package moves as a whole when every file is completed and its post-processing has ended (done or skipped). Until then it stays in Queue.
- Failed downloads stay in Queue.
- Queue's "active" count and the status bar ignore Finished rows, as today.

### 6.3 The view (`FinishedView.svelte`)
- Newest first, grouped by package like Queue, packages collapsed by default. Columns: Name, Size, Finished (`14:32` today, a date before today), Location. Rail count as in the mockup. The filter box matches names.
- Actions (drawer, keyboard, right-click menu):
  - **Open** (Enter): opens the file with its default app.
  - **Open folder:** Explorer with the file selected.
  - **Remove from list** (Delete): deletes the rows only, with the existing Undo toast.
  - **Delete file…** (Shift+Delete): confirmed; the file (or an extracted package folder) goes to the Recycle Bin via the `trash` crate; then the rows are removed. A file already gone is not an error.
  - **Download again:** adds the same URL as a new download.
- A file no longer on disk shows "Missing" (checked when Finished opens and when a row is selected); Open and Open folder are disabled for it.

### 6.4 Notifications
- Sent when a download with no package completes, and when a package's post-processing ends, if `notifyFinished` is on. Text: `<name> finished` (or the post-processing result, as today).
- Windows toast with an **Open folder** button; clicking the toast shows Haul on Finished. Built with a Windows toast library that supports buttons and activation (`tauri-winrt-notification`), because `tauri-plugin-notification` has no actions on Windows.
- Fallback if activation proves unreliable: the toast has no button and clicking it opens Haul on Finished. The build notes say which shipped.

## 7. First run and the speed readout

- **First run:** while the Queue is empty and `firstDownloadSeen` is false, the empty state is a welcome with three steps, each ticked when done:
  1. Add a link: "Copy a link or drop one here" (Ctrl+V).
  2. Watch the clipboard: the switch and one line, "Haul notices download links you copy; nothing is sent anywhere."
  3. Pair your browser: how to load the extension unpacked in Edge or Chrome, a button that opens the extension folder, and "Paired with <browser>" once paired.
  After the first download is added, `firstDownloadSeen` is set and the plain empty state returns for good.
- **Speed readout:** the engine's speed average (`emit_progress`) uses a 5 s window instead of 2 s. "Left" uses the same speed. Transfer behaviour is unchanged.

## 8. Testing

- **Rust:** settings validation, clamping, live apply, unwritable folder refused, existing keys read back; `should_fire` and the countdown with a fake `PowerAction`; migration 6 and `finished_at` set and cleared; Recycle Bin delete against a temp folder; the 5 s speed window.
- **Front end:** Vitest for settings, Finished and when-done state; Playwright e2e on the mock in light and dark for the Settings page, Finished (open, remove + undo, delete confirm, missing), the welcome steps and the countdown dialog.
- **Real app smoke:** close to tray keeps a download running and hides the window; a second launch brings the first window back.
- **Owner by hand:** the tray menu, autostart after a real sign-in, one real Sleep. A real Shut down is optional.

## 9. Order of work

1. Settings record, commands and page.
2. Tray, close to tray, single instance, autostart.
3. Migration 6, `finished_at`, and the Finished view.
4. When done.
5. Notifications.
6. First run and the speed readout.
