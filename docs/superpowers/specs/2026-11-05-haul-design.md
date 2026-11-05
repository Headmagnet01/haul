# Haul: design spec

Date: 2026-11-05 · Status: draft for owner review · Working name: **Haul** (check the name is free before a public release)

## 1. Goal

A free, open-source Windows download manager that takes the best of **IDM** (a fast, light, reliable engine and browser capture) and **JDownloader 2** (link collecting, packages, auto-extract, rules) and drops what makes each one painful: IDM's paywall, dated UI and pop-up windows; JD2's heavy Java runtime, cluttered settings and fragile per-site plugins.

It starts as the owner's personal tool and is built so it can later be released publicly under an open-source licence.

### Success criteria

- A 1 GB+ file from a range-capable server downloads at least as fast as a single-connection browser download, and recovers from a dropped connection with no user action.
- Killing the app mid-download and restarting it resumes every download with no corrupt files.
- A pasted list of 20 links becomes correctly grouped packages, with offline links flagged, in under 10 seconds.
- A finished multi-part archive package is verified, extracted and sorted with no clicks when auto mode is on.
- Idle memory use is under 150 MB with 100 finished items in history.
- A download clicked in Chrome, Edge or Firefox lands in Haul with its cookies, so logged-in downloads work.

## 2. Feature comparison and scope

| Area | IDM | JDownloader 2 | Haul v1 |
|---|---|---|---|
| Multi-connection download | Dynamic segmentation, very strong | Chunks, weaker on a single file | **Split-and-steal segmentation (IDM-style)** |
| Resume after crash | Yes | Yes | Yes, with ETag/Last-Modified/size check |
| Browser capture | Excellent | Clunky | **Extension + local API, cookies passed** |
| Link collecting | Weak | Link Grabber, strong | **Collector with grouping, online check, duplicates, filters** |
| Packages / multi-part archives | No | Yes | Yes |
| Auto-extract with password list | No | Yes | Yes, bundled 7-Zip, passwords encrypted with DPAPI |
| Checksum verification | No | Partial | Yes (sfv/md5/sha1/sha256 files, pasted hash, server digest) |
| Categories and sort rules | Categories | Packagizer rules | Categories + ordered rules |
| Scheduler, sleep/shutdown when done | Yes | Partial | Yes |
| Speed limits | Yes | Yes | Global and per-download |
| Clipboard watching | No | Yes | Yes, toggle in the status bar |
| Virus scan after download | Yes | No | Optional, Windows Defender, off by default |
| Remote control | No | My.JDownloader | v2 (the local API is built to allow it) |
| Video stream grabber | Yes | Plugins | v2 (direct media links work in v1) |
| Site crawler | Site Grabber | Deep decrypt | v2 |
| FTP | Yes | Yes | v2 |
| Per-site hoster plugins, captcha | No | Yes | **Never** (maintenance treadmill, main source of JD2 bloat) |
| Router reconnect, DLC containers | No | Yes | **Never** |
| Torrents | No | No | **Never** |
| Price, platform | Paid, Windows | Free, Java | Free, open source, Windows first |

## 3. Architecture

Three parts in one repository (the repository root is the project folder):

```
<repo root>/
  app/            Tauri 2 desktop app
    src-tauri/    Rust: engine + Tauri shell + local API
    src/          Svelte 5 + TypeScript UI
  extension/      Manifest V3 browser extension (Chrome, Edge, Firefox)
  docs/
```

- **Engine (Rust, inside `src-tauri`).** All download logic lives here as plain Rust modules with no Tauri types, so they can be tested without the UI.
- **UI (Svelte 5 + TypeScript + Vite).** Calls the engine through Tauri commands; the engine pushes state through Tauri events.
- **Browser extension (Manifest V3, TypeScript).** Talks to the app over the local HTTP API.

### Main crates and tools

| Need | Choice |
|---|---|
| Async runtime | `tokio` |
| HTTP client (HTTP/1.1 + HTTP/2, rustls) | `reqwest` |
| Local API server | `axum` |
| Storage | SQLite via `rusqlite` (bundled), WAL mode |
| Clipboard watching | `clipboard-master` + `arboard` |
| Encryption of secrets | Windows DPAPI via the `windows` crate |
| Notifications, tray, autostart | Tauri plugins (`notification`, tray API, `autostart`) |
| Extraction | Bundled **7-Zip** `7z.exe` + `7z.dll` (needed for RAR), run as a child process |
| Logging | `tracing` with a rolling file in the app data folder |

### Engine modules

Each module has one job and a small public interface.

| Module | Job | Depends on |
|---|---|---|
| `probe` | Learn name, size, range support, ETag/Last-Modified, final URL for a link | `reqwest` |
| `segmenter` | Pure logic: plan segments, decide steals and splits | nothing (pure, unit-tested) |
| `transfer` | Run one download: connections, writing to disk, retries, speed limit | `probe`, `segmenter`, `store` |
| `queue` | Decide what runs: parallel limit, priorities, per-host limits, scheduler | `transfer`, `store` |
| `grabber` | Collector: intake, grouping, duplicates, filters, auto-start decision | `probe`, `store` |
| `postproc` | Verify, extract, sort/rename, virus scan, notify | `store`, 7-Zip |
| `rules` | Pure logic: categories and user rules → folder and file name | nothing (pure, unit-tested) |
| `store` | SQLite access and migrations | `rusqlite` |
| `api` | Local HTTP API, pairing, auth | `grabber`, `queue` |
| `events` | One event bus the UI and API subscribe to | `tokio::sync::broadcast` |

### Link lifecycle

1. A link enters from the clipboard, paste/drop, or the extension.
2. `grabber` probes it (8 probes in parallel), groups it into a package, flags duplicates and applies filters.
3. Auto mode decides: links sent from the browser start immediately; clipboard and pasted links wait in the Collector. Both behaviours can be changed in settings.
4. A started package moves to `queue`, which hands files to `transfer` as slots free up.
5. When every file in a package is finished, `postproc` runs verify → extract → sort → (optional) scan → notify.

## 4. Download engine

### Probe
`HEAD` request, falling back to `GET` with `Range: bytes=0-0` when `HEAD` is refused. It records: final URL after redirects, size, `Accept-Ranges` or a `206` reply, file name (`Content-Disposition`, then URL path), `ETag`, `Last-Modified`, and `Content-Type`.

### Segmentation: split and steal
- The file is pre-allocated as `<name>.haulpart` in the target folder.
- Start with `min(8, size / 4 MiB)` connections (at least 1). The user can set 1–32 per download; the default is 8.
- Each connection owns a byte range and writes at its offset.
- **Steal:** when a connection finishes its range, it takes the **second half of the largest remaining range**, provided that half is at least 1 MiB. This repeats until nothing is worth splitting.
- **Slow connection:** a connection running below 25% of the median connection speed for 10 seconds is closed and its range returned to the pool.
- **Per-host cap:** at most 16 connections to one host across all downloads (configurable).
- Without range support: one connection, and the UI shows "Can't resume".
- Writes go through a buffered writer per connection (256 KiB) and `fsync` on pause, exit and completion.

### Persistence and resume
- Segment progress is saved to SQLite every 2 seconds and on pause, exit and error.
- On resume, re-probe and compare ETag, then Last-Modified, then size. Any mismatch restarts the file from zero and logs why.
- On completion, rename `.haulpart` to the final name. If that name exists, use `name (1).ext`, `name (2).ext` and so on. Never overwrite.

### Errors and retries
- Network errors and 5xx: retry with backoff of 2, 4, 8, 16, 32 seconds, then 60-second steps, up to 10 attempts (configurable), then mark the download failed.
- `403`, `410` or a signed URL that has expired: state **Link expired**. The UI offers "Refresh from browser". When a new capture arrives whose file name and size (and ETag, when known) match an expired download, Haul attaches the new URL and cookies to it and resumes instead of creating a duplicate.
- `404`: state **Offline**.
- Disk full or the folder is not writable: pause the download with a clear message. Free space is checked before starting.
- `416` on resume: re-probe and restart that file.

### Queue and scheduler
- Default 3 downloads running at once, adjustable from 1 to 20.
- Priorities: high, normal, low, plus drag-to-reorder.
- Speed limit: a global token bucket plus an optional per-download limit. The status-bar "turtle" toggle switches the global limit on and off.
- Scheduler: optional start time and stop time on chosen weekdays.
- "When done": nothing, sleep, or shut down, with a 60-second cancellable countdown dialog.

## 5. Collector and post-processing

### Intake
- **Clipboard watcher:** picks up `http`/`https` links when the clipboard changes; ignores text Haul itself put on the clipboard. Toggle in the status bar.
- **Paste:** Ctrl+V anywhere in the window extracts every link from the pasted text.
- **Drop:** links, `.txt` files, or text dropped anywhere on the window.
- **Extension:** single link, selection, page links through a filter, or a captured download.

### Grouping into packages
Recognised multi-part patterns (by base name):
`name.partN.rar`, `name.rar` + `name.rNN`, `name.7z.NNN`, `name.zip.NNN`, `name.zip` + `name.zNN`.
Everything else from one paste, one drop or one extension send becomes one package, named after the common file-name prefix or, failing that, "Pasted list · <date time>" or the page title.

### Duplicates and filters
- Duplicate if the same final URL, or the same name and size, is already queued or finished. Duplicates are flagged, not dropped.
- Filter chips: hide `.html` pages, hide images, hide under 1 MB, hide duplicates. Users can add extension filters.

### Post-processing (runs when a whole package has finished)
1. **Verify.**
   - Hash sources, in order: a checksum file in the package (`.sfv`, `.md5`, `.sha1`, `.sha256`, `SHA256SUMS`), a hash pasted by the user, a server `Digest`/`Content-MD5` header.
   - A mismatch re-downloads **only the failing file**, once; a second failure marks it failed.
   - No hash available: the step shows "No checksum" and passes.
2. **Extract.**
   - Waits for all parts, then runs `7z x`.
   - Tries an empty password, then each saved password in order, then asks the user.
   - Extracts into a folder named after the package, then extracts nested archives one level deep.
   - Optional: move the source archives to the Recycle Bin after a successful extraction. Haul never permanently deletes.
3. **Sort and rename.**
   - Default categories by extension: Video, Music, Documents, Programs, Compressed, Other, each with a folder.
   - User rules match host, file name (glob or regex), extension and size range. Their actions are a target folder and a rename pattern with `{name}`, `{ext}`, `{package}`, `{host}`, `{date}`.
   - Rules run top to bottom and the first match wins.
4. **Virus scan (optional, off by default).** Runs `MpCmdRun.exe -Scan -ScanType 3 -File <path>`. A detected threat shows a warning and leaves the file where it is.
5. **Notify.** A Windows notification with an "Open folder" action.

Every step's state (pending, running, done, skipped, failed) is stored and shown in the detail drawer.

## 6. Browser extension and local API

### Extension
- One Manifest V3 codebase built for Chromium (Chrome, Edge) and Firefox.
- **Capture:**
  - `downloads.onCreated`: when a download matches the capture rules (default: at least 1 MB, or one of the listed file types), cancel and erase the browser download and send it to Haul.
  - Holding **Alt** while clicking lets the browser handle that download instead.
  - If Haul does not answer within 1.5 seconds, the browser keeps the download. A download is never lost.
- **Context menu:** "Download with Haul" on links, and "Download all links…", which opens a picker with type filters and the current selection.
- **Cookies and headers:**
  - Sends the cookies for the download URL only (`cookies.getAll({url})`), plus the referrer and user agent.
  - Haul stores them encrypted with DPAPI and deletes them when the download finishes.
- **Popup:** paired or not paired status, plus a short list of active downloads.

### Local API
- HTTP on `127.0.0.1:17654`, falling back to the next free port in `17654–17664`. The chosen port is written to the app data folder so the extension can find it.
- **Pairing:**
  1. The extension calls `POST /v1/pair`.
  2. Haul shows "Allow <browser> to send downloads?".
  3. On approval it returns a random 256-bit token, which the extension stores.
  4. Paired clients are listed in Settings and can be revoked.
- **Every other request must:**
  - carry `Authorization: Bearer <token>`;
  - have a `Host` header of `127.0.0.1:<port>` (this blocks DNS-rebinding attacks);
  - have an `Origin` that is a browser-extension origin bound at pairing.
- **Endpoints (v1):**
  - `POST /v1/pair`: request pairing.
  - `POST /v1/links`: send `{ links: [...], source, page_title?, auto_start? }`.
  - `POST /v1/capture`: send one captured download `{ url, filename?, size?, referrer, user_agent, cookies }`.
  - `GET /v1/status`: version, paired, active count.
- Native messaging was considered and rejected for v1: it needs a registry entry per browser, and the same HTTP API serves the v2 remote control. Token plus Host and Origin checks give comparable protection.

## 7. User interface

Mode: **Operate**. The visual system is recorded in `DESIGN.md`, and the reference is the mockup at `docs/mockup/index.html`.

- **Layout:**
  - Left rail: Queue, Collector (with count badge), Finished, Rules, Settings; then Categories; at the bottom, extension pairing status.
  - Main list: collapsible package rows with file rows. Columns are Name, Size, Progress (segment strip), Speed, Left, Status.
  - Right detail drawer for the selected row: segment map, per-connection speeds, after-download checklist, details, actions.
  - Status bar: total speed with a 60-second sparkline, active and waiting counts, speed-limit toggle, clipboard toggle, "When done" selector.
- **Signature element:** the segment strip in every row shows each connection's range filling. The drawer's segment map and a one-line note show steals as they happen.
- **Interaction:**
  - Ctrl+V anywhere adds links; drop anywhere adds links.
  - Space pauses or resumes the selection. Delete removes it from the list, with an Undo toast; deleting the file on disk is a separate, confirmed action that moves it to the Recycle Bin.
  - Closing the window minimises Haul to the tray, which shows live speed.
- **No pop-up window per download**, unlike IDM.
- **First run:**
  - An empty state saying "Copy a link or drop one here".
  - The clipboard switch, explained in one line.
  - A "Pair your browser" step with links to load the extension.
- **Accessibility:**
  - Every action works from the keyboard.
  - Text meets WCAG AA contrast in both themes.
  - Status is shown with icon and word, never colour alone.
  - Motion stops when Windows "reduce motion" (prefers-reduced-motion) is on.
- **Live updates:** progress events are throttled to 4 per second per download, and the UI batches DOM updates.

## 8. Data model (SQLite)

| Table | Key columns |
|---|---|
| `packages` | id, name, source, created_at, state, target_dir, auto_extract, postproc_state (JSON) |
| `downloads` | id, package_id, url, final_url, file_name, size, etag, last_modified, resumable, state, priority, position, connections, speed_limit, target_path, error, created_at, finished_at |
| `segments` | download_id, start, end, written |
| `collector_links` | id, group_key, url, probe_result (JSON), source, duplicate_of, created_at |
| `secrets` | download_id, kind (cookies or headers), blob (DPAPI) |
| `passwords` | id, position, blob (DPAPI) |
| `rules` | id, position, enabled, match (JSON), action (JSON) |
| `paired_clients` | id, name, origin, token_hash, created_at, last_seen |
| `settings` | key, value (JSON) |
| `history_meta` | schema version, migrations |

Tokens are stored as SHA-256 hashes, never in plain text.

## 9. Testing

- **Unit tests:** `segmenter` (steal and split decisions), `rules`, the grouping patterns in `grabber` (a table of 50+ real-world file names), URL extraction from pasted text, and `api` auth checks.
- **Integration tests:** a test HTTP server in Rust (an `axum` dev-dependency) that can throttle per connection, drop connections mid-range, refuse ranges, change the ETag mid-download, return 403/404/416, and serve multi-part archives. Each engine error path in section 4 has a test against it.
- **Post-processing tests:** fixture archives (zip, 7z, split 7z, RAR, password-protected, corrupted) plus checksum files.
- **UI checks:** Playwright against the Vite dev build with a mocked engine for the main flows (add links, collector start, pause/resume, theme).
- **Manual release checklist:** a real 1 GB+ download, killing the process mid-download, the extension in Chrome, Edge and Firefox, and sleep/shutdown when done.

## 10. Packaging and licence

- **Installer:** Tauri's NSIS installer, per-user install with no admin rights needed. WebView2 comes with Windows 11.
- **Code signing:** v1 is unsigned, so Windows SmartScreen will warn. A signing certificate is a later decision for the public release.
- **Licence:** MIT OR Apache-2.0 (the Rust convention). The bundled 7-Zip keeps its own LGPL and unRAR licence files in the installer.
- **Updates:** v1 checks GitHub Releases and shows "Update available". v1 does not auto-install updates.

## 11. Milestones (the order the implementation plan follows)

1. Repository scaffold, Rust engine `probe` + `segmenter` + `transfer` + `store`, with the test server and tests.
2. `queue`, the scheduler and speed limits.
3. Tauri shell and the Queue UI with segment strips and the detail drawer.
4. `grabber`, the clipboard watcher and the Collector UI.
5. `postproc` (verify, extract, sort, scan, notify) and the Rules UI.
6. Local API, pairing and the browser extension.
7. Settings, tray, first run, "when done", the Finished view, and packaging.

## 12. Out of scope for v1

Video stream grabbing (HLS/DASH), site crawler, remote control UI, FTP, folder watch, RSS, macOS and Linux builds, auto-install updates, per-site plugins, captcha solving, router reconnect, DLC containers, torrents.
