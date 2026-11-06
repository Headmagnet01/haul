# Haul: post-processing and Rules, design (plan 4)

Date: 2026-11-06 · Status: approved by the owner in chat ("you tell me" → designer's call) · Extends section 5 of `2026-11-05-haul-design.md`, which stays the source of truth where this file is silent.

## Owner decisions
- **Extractor:** bundle the official 7-Zip (`7z.exe` + `7z.dll`, LGPL + unRAR licence) inside Haul. WinRAR (paid, RAR-only on the command line) is not used by Haul; the owner's installed `Rar.exe` is used only to make RAR test fixtures.
- **Auto mode on by default:** a finished package is verified, extracted and sorted with no clicks.
- **Archives are kept** after a successful extraction. A switch sends them to the Recycle Bin instead (never a permanent delete).
- **Sort into category folders by default** (`<download folder>\Video`, `\Programs`, …).

## What runs, and when
A **run** belongs to one package. A download with no package gets a run of its own (keyed by the download). A run starts when every download in its package is `completed`. Runs execute one at a time, in the order they became ready, on a background task; a run left `running` by a crash restarts from its first unfinished step on next launch.

Each run stores five steps, in order: `verify`, `extract`, `sort`, `scan`, `notify`. Each step has a state (`pending`, `running`, `done`, `skipped`, `failed`, `needs_password`) and a one-line detail for the drawer ("SHA-256 from SHA256SUMS", "No checksum", "Not an archive", "→ D:\Downloads\Video"). A failed step stops the run; Retry restarts it from that step.

### 1. Verify
- Hash sources, first found wins per file: a checksum file in the package (`.sfv` CRC32, `.md5`, `.sha1`, `.sha256`, `SHA256SUMS`/`SHA1SUMS`/`MD5SUMS`), a hash pasted in the drawer (stored on the download), a server digest captured by the probe.
- Server digests: `Repr-Digest`/`Digest` (`sha-256`, `md5`) from any probe reply; `Content-MD5` only from a `HEAD` or a full `200` reply (on a `206` it describes only the range).
- A pasted hash's algorithm is told by its length: 8 hex = CRC32, 32 = MD5, 40 = SHA-1, 64 = SHA-256.
- A mismatch re-downloads only that file, once (its row goes back to the queue from zero; the run waits for it). A second mismatch marks the step failed with "Checksum mismatch".
- Checksum files themselves are not verified and are not extracted.
- No hash for any file: the step is `skipped` with "No checksum".

### 2. Extract
- Archive sets come from the Collector's existing `archive_part` rules; a lone `.zip`/`.rar`/`.7z`/`.tar`/`.gz`/`.tgz` counts as a one-volume set. Only the first volume is passed to 7-Zip.
- `7z x -y -bb0 -p<pw> -o<out> <first volume>`. Passwords are tried in order: empty, then each saved password. A wrong password is told apart from a corrupt archive by 7-Zip's exit code and its "Wrong password" message.
- No password works: the step is `needs_password`. The drawer shows a password box with a "Remember this password" tick (on by default). A working password that is ticked is added to the end of the saved list.
- Output folder: `<package folder>\<package name>`, made unique (`name (1)`) if it exists. Archives found directly inside the output are extracted one level deep, each into a folder next to it named after it.
- Afterwards, if "Recycle archives after extracting" is on, every volume goes to the Recycle Bin. Nested archives are kept, always.
- No archive in the package: `skipped`, "Not an archive".

### 3. Sort and rename
- What moves: the extracted folder (as one unit), every non-archive file, and the archives that were kept. Checksum files stay with the archives.
- Destination for a file: the first enabled rule that matches (top to bottom); else, if "Sort into category folders" is on, its category's folder; else it stays.
- An extracted folder is placed by the rule/category of its largest file, with the folder's name as `{name}`.
- Rules match on any of: host (exact or `*.example.com`), file name (glob, or regex when written `/…/`), extensions, size range. Unset fields match everything. Actions: a target folder (absolute, or relative to the download folder) and an optional rename pattern with `{name}`, `{ext}`, `{package}`, `{host}`, `{date}` (`YYYY-MM-DD`, the day it finished). A rename keeps the result a safe Windows name.
- Default categories (editable): Video, Music, Documents, Programs, Compressed, Other, each with an extension list and a folder `<download folder>\<Category>`.
- Never overwrite: a clash becomes `name (n).ext`. Same-drive moves are a rename; across drives it copies, checks the size, then removes the source. The download's stored path is updated.
- Nothing to move: `skipped`.

### 4. Scan (optional, off by default)
`%ProgramFiles%\Windows Defender\MpCmdRun.exe -Scan -ScanType 3 -File <path> -DisableRemediation`, once per moved item (a folder is scanned whole). Exit 2 means a threat: the step fails with "Threat found", the file stays put. Defender missing: `skipped`, "Windows Defender not found".

### 5. Notify
A Windows notification "‹package› is ready" (or "‹package› needs attention" when a step failed or needs a password). Clicking it does nothing: Tauri can't route a click reliably on Windows; the drawer has Open folder.

## Data
- Migration 4:
  - `postproc_runs` (id, package_id, download_id, state, steps_json, created_at, updated_at), one row per package or lone download.
  - `downloads` gains `expected_hash`, `server_digest` and `verify_retries`.
  - `passwords` (id, position, blob), the blob being DPAPI-encrypted for the current Windows user.
  - `rules` (id, position, enabled, match_json, action_json).
- Settings keys: `postproc` = `{"auto":true,"sortCategories":true,"recycleArchives":false,"virusScan":false}`, and `categories` (the six defaults until edited).

## Code layout
- `crates/engine/src/rules.rs`: pure. Categories, rule matching, rename patterns, `decide(file, rules, categories) -> Destination`.
- `crates/engine/src/postproc/`:
  - `checksum.rs`: parse checksum files, hash a file.
  - `extract.rs`: the 7-Zip runner behind a small `Extractor` trait, so tests can fake it.
  - `sort.rs`: moves with unique names.
  - `scan.rs`
  - `mod.rs`: the `PostProc` service that watches events and runs steps.
- `crates/engine/src/store/postproc.rs`: runs, passwords, rules.
- `crates/engine/src/secret.rs`: DPAPI protect/unprotect (Windows only).
- App: commands `postproc_runs`, `postproc_retry`, `postproc_password`, `set_expected_hash`, `rules_get`, `rules_set`, `passwords_list`, `passwords_set`, `set_postproc_settings`; `haul:update` gains `postproc: true` when a run changed. Notifications through `tauri-plugin-notification`; 7-Zip is shipped as a Tauri resource from `app/src-tauri/resources/7zip/`.
- UI:
  - The drawer's "After download" list.
  - Package rows show Verifying, Extracting or Sorting.
  - A Rules view with four sections: switches, categories, rules (editor, reorder, "Try a file name") and saved passwords. Saved passwords never go back to the UI: the list shows a masked entry per password, with its position.

## Out of this plan
The rail's category filter list and the Finished view (plan 7); an "Open folder" action on the notification.

## Testing
- Rust unit tests: `rules` (a table of matches, renames and categories) and the checksum parsers.
- Rust integration tests against committed fixture archives: zip, 7z, split 7z, multi-part RAR, password-protected, corrupted, nested, and checksum files. They cover the 7-Zip runner, the full run, the re-download on mismatch (local test server), crash recovery and DPAPI round-trips.
- The Vitest and Playwright tests use a mock post-processor, covering the drawer steps, the password prompt, Retry, the Rules view and the a11y checks in both themes.
- The real-app smoke test gets one more case: a two-part 7z downloads, then extracts and sorts.
