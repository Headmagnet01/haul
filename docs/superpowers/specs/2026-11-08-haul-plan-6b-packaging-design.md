# Haul plan 6b: installer, update check, Collector cookies and quoted autostart

Design agreed with the owner on 8 Nov 2026. It finishes Plan 6 of `2026-11-05-haul-design.md` (sections 6, 7 and 10) after `2026-11-08-haul-plan-6a-app-shell-design.md`. Where this file and the main spec differ, this file wins.

## 1. Goal and scope

Haul installs like a normal Windows app and keeps working the same way installed: the browser extension and 7-Zip come with it, Start with Windows is safe, browser captures sent to the Collector keep their cookies, and Haul says when a newer release exists.

- **Success:**
  - A per-user installer installs Haul with no admin prompt; the uninstaller removes it and its Start with Windows entry.
  - In the installed build, "Open the extension folder" opens the bundled extension, and a split archive extracts with the bundled 7-Zip.
  - Start with Windows writes `"<path to haul.exe>" --hidden`, with the quotes, and an installed Haul starts hidden in the tray after a real sign-in.
  - A login-only download captured with "Wait in the Collector" shows Online in the Collector and downloads after Start.
  - With a newer release on GitHub, Haul shows "Update available" within a day of it appearing.
  - Dev runs and smoke tests never touch the installed Haul's data.
- **Out of scope:** code signing (SmartScreen will warn), installing updates, Firefox, an extension store listing, an MSI, the other 6a deferred minors (handoff, "Plan 6a deferred minors") except the three named in sections 3.4 and 4.4.

## 2. Owner decisions

| Question | Decision |
|---|---|
| Update check | Build it now, against `meraaki410/haul`. |
| Update check default | On, with a Settings switch. |
| Dev and installed data | Separate: dev runs use their own identifier and data folder. |
| NSIS tools | Tauri's bundler may download them (NSIS 3 and its plugin DLL) on the first build. |

## 3. Collector captures keep their cookies (I5)

### 3.1 Data
- Migration 7 adds `collector_links.headers_blob BLOB`: the capture's request headers (`Cookie`, `Referer`, `User-Agent`, as `capture.rs` builds them today), DPAPI-sealed with `secret::protect`, the same as `downloads.headers_blob`.
- Only browser captures fill it. Pasted, dropped, clipboard and "Download all links…" links keep NULL.
- The headers never leave the engine: `CollectorLink` as the UI sees it has no headers field. The store returns them only through a separate `link_headers(id) -> Option<String>` read.

### 3.2 Flow
- **Capture with "Wait in the Collector":** `Grabber::intake_capture(url, headers_json)` adds a one-link batch labelled as today, storing the sealed headers. If the same URL is already waiting, its headers are replaced with the new ones and it is checked again; nothing is added. The capture returns 0 as today.
- **Online check and Recheck:** the prober takes the link's headers (`ProbeFn` gains an `Option<HeaderMap>` argument). A link with headers is probed with them.
- **Start:** `Grabber::start` passes the link's headers to `NewDownload.headers_json`. From there the download's existing rules apply (sealed in `downloads`, wiped on Completed and Failed). The link row, and its blob, are deleted by the existing start path.
- **Remove:** deleting the row deletes the blob.

### 3.3 Errors
- A blob that cannot be unsealed (another Windows user, a damaged row) is treated as no headers, logged once per link; the link is still checked and can still start.

### 3.4 Tests (Rust, written first)
- A capture to the Collector stores sealed headers (the raw column holds no `sid=` text), and the UI's link has none.
- The probe of a captured link receives its cookie; a pasted link's probe receives none.
- Starting a captured link queues a download whose headers contain the cookie.
- A second capture of a waiting URL refreshes its headers and adds nothing.
- Removing a link removes its blob.
- Migration 7 on a schema-6 DB keeps existing links.
- App level: with `browserStart = collector`, a capture against the test server's cookie-only route ends Online, and after Start the download completes.

## 4. Start with Windows, quoted

### 4.1 Why
`tauri-plugin-autostart` (through `auto-launch` 0.6) writes the Run value as `format!("{} {}", path, args)`, with no option for quotes. With a path containing spaces, Windows first tries the shorter paths (`C:\tools\claude.exe`, …), so a program planted there would run at sign-in.

### 4.2 Design
- Remove `tauri-plugin-autostart` (crate, `lib.rs` plugin, the `autostart:default` permission).
- New `app/src-tauri/src/autostart.rs`:
  - `run_value(exe: &Path) -> String`, pure: `"<exe>" --hidden`. The exe is always quoted; the `\\?\` prefix is stripped.
  - `Autostart { key: String }` with `enable(exe)`, `disable()` and `is_enabled() -> bool`, on `HKCU\<key>` value `Haul`. The real key is `Software\Microsoft\Windows\CurrentVersion\Run`; tests use a scratch key under `HKCU\Software\Haul-test-<random>`, deleted afterwards. Registry calls use `windows-sys` (`Win32_System_Registry`), already a dependency.
  - `disable` on a missing value is not an error.
  - `enable` also clears a "disabled" mark Windows keeps under `…\Explorer\StartupApproved\Run` for `Haul` if present, so turning the switch on after disabling Haul in Task Manager works. It never writes that key otherwise.
- **At start:** if the `autostart` setting is on, Haul calls `enable(current_exe)` again. This repairs today's unquoted value and follows the exe if it moved. If the setting is on but the value is gone (removed outside Haul), the setting is turned off instead (it follows the registry).
- **Settings:** `set_settings` with `autostart` calls `enable`/`disable` first, as today; a failure keeps the switch as it was and shows the error.

### 4.3 `--hidden` while running
A second launch with `--hidden` (the single-instance callback receives its arguments) does nothing; any other second launch shows the window as today.

### 4.4 Tests
- `run_value` quotes a path with spaces, strips `\\?\`, and adds `--hidden` (written first).
- On the scratch key: enable writes the quoted value; disable removes it; disable twice is fine; is_enabled follows both.
- Start-up repair: an unquoted value is rewritten quoted; a missing value turns the setting off.
- `second_launch_args_hidden_keeps_window_hidden` for the single-instance decision (a pure function `should_show(args)`).

## 5. Installer, bundled resources and data folders

### 5.1 Identifiers and data folders
- `tauri.conf.json` (the base, used by `tauri dev` and every test) gets identifier `dev.haul.desktop.dev`, so dev runs keep their data in `%APPDATA%\dev.haul.desktop.dev` and logs in `%LOCALAPPDATA%\dev.haul.desktop.dev\logs`. Dev runs and the installed Haul can then run side by side (single instance is per identifier). The local API's port fallback (17654–17664) gives each its own port; the extension pairs with whichever it finds.
- New `app/src-tauri/tauri.release.conf.json` overrides the identifier to `dev.haul.desktop` and the capability list (5.4). The installed Haul uses `%APPDATA%\dev.haul.desktop`, which holds today's (nearly empty) dev data: two old test pairings show in Settings › Browsers and can be removed.
- `app/package.json` gains `"release": "tauri build --config src-tauri/tauri.release.conf.json"`. The release checklist says to build only through it.
- The real-app smoke test reads `api-port` from `dev.haul.desktop.dev`.

### 5.2 Installer (NSIS)
- `bundle.targets`: `["nsis"]`. `bundle.windows.nsis`: `installMode: "currentUser"` (installs to `%LOCALAPPDATA%\Haul`, no admin), `languages: ["English"]`, `displayLanguageSelector: false`, and the Haul icon. WebView2: the default download bootstrapper (Windows 11 has WebView2 already).
- `bundle.license`: `LICENSE-MIT`; `bundle.publisher`, `copyright`, `shortDescription` filled.
- An NSIS hook file (`app/src-tauri/installer-hooks.nsh`, `NSIS_HOOK_POSTUNINSTALL`) deletes `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` value `Haul` and the matching `StartupApproved\Run` value. App data is left unless the user ticks the uninstaller's own "delete app data" box.
- The Start menu shortcut carries the identifier as its AUMID (the Tauri NSIS default), so installed toasts are labelled Haul (`toast.rs` already uses the identifier outside `target\`).

### 5.3 Bundled resources
- `bundle.resources` adds `"../../extension/dist/": "extension/"`, next to 7-Zip.
- `build.beforeBuildCommand` builds the extension first: `npm --prefix ../extension run build && npm run build`.
- `open_extension_folder` already opens `<resource dir>\extension`; a test checks that the resource map covers every file the extension's `manifest.json` names (icons, pages, scripts), so a missing file fails the build checks, not the install.
- `LICENSE-MIT`, `LICENSE-APACHE` (Haul: MIT OR Apache-2.0, "The Haul contributors") are added at the repository root and bundled with 7-Zip's `License.txt` as today.

### 5.4 Capabilities
- The test-only `core:window:allow-close` moves from `capabilities/default.json` to `capabilities/dev-test.json`. The base config lists both capabilities; the release config lists only `default`. `autostart:default` is removed (4.2).

### 5.5 Release checklist
New `docs/release-checklist.md`: the full gate, `npm run release`, the installer's path and SHA-256, install, the owner checks (section 7), uninstall. It replaces the main spec's "manual release checklist" lines for v1.

## 6. Update check

### 6.1 Behaviour
- `app/src-tauri/src/update.rs`. While `checkUpdates` is on, Haul checks 30 s after start, then every 24 h while running, skipping a check when the last one was under 24 h ago (`lastUpdateCheck`, an internal settings key holding a UTC time).
- One request: `GET https://api.github.com/repos/meraaki410/haul/releases/latest`, headers `User-Agent: Haul/<version>` and `Accept: application/vnd.github+json`, 15 s timeout. Nothing else is sent.
- The reply's `tag_name` (`v0.2.0` or `0.2.0`) is compared with the app's version as three numbers; a tag with a pre-release part (`-beta`) or anything unparsable is ignored. `html_url` is kept only if it starts with `https://github.com/meraaki410/haul/releases/`; otherwise the releases page is used.
- Result kept in memory: `UpdateStatus { latest: Option<String>, url: Option<String>, checkedAt, error: Option<String> }`. A 404 (no release yet) counts as up to date. Network and parse errors are logged and kept in `error`; nothing pops up.
- A newer version emits `haul:update-available {version, url}`.

### 6.2 UI
- **Status bar:** an "Update available 0.2.0" button at the right, shown only when newer; it opens the URL in the default browser (through the opener plugin, with the URL checked again in Rust).
- **Settings › App:** a "Check for updates" switch (on by default, key `checkUpdates`) and a **Check now** button that shows "You have the latest version (0.1.0)", "Version 0.2.0 is available" with the link, or "Couldn't check: <reason>".
- Commands: `update_status() -> UpdateStatus`, `check_updates_now() -> UpdateStatus`. The feed URL is a constant in `update.rs`; tests inject a local URL.

### 6.3 Tests
- `is_newer` table: `v0.2.0` > `0.1.0`, equal is not newer, `0.10.0` > `0.9.9`, `v1.0.0-beta` ignored, junk ignored (written first).
- Against a local axum server: newer, same, 404, 500 and bad JSON give the right status; the request carries the user agent; an off switch sends nothing; a check within 24 h of the last is skipped.
- URL guard: a foreign `html_url` is replaced with the releases page.
- Vitest and e2e on the mock: the status-bar notice appears for a newer version and opens the link; the Settings switch and Check now show each result, in light and dark, with axe.

## 7. Owner checks on the installed build
1. The installer runs with no admin prompt and Haul opens.
2. Settings › Browsers or the welcome: Open the extension folder opens `%LOCALAPPDATA%\Haul\extension` (or the resource folder Tauri chose), and Edge loads it unpacked.
3. A split 7z package downloads and extracts (bundled 7-Zip).
4. A finished-download toast is labelled Haul, not Windows PowerShell.
5. Start with Windows on: `HKCU\…\Run` `Haul` shows the path in quotes with `--hidden`. Sign out and in: Haul is in the tray, window hidden.
6. Check now in Settings says "You have the latest version" (no release exists).
7. Uninstall: Haul is gone from the Start menu and the Run value is gone.

## 8. Order of work
1. Dev identifier, release config, capabilities split (so every later test runs on dev data).
2. Quoted autostart (`autostart.rs`), the start-up repair, `--hidden` while running.
3. Migration 7 and Collector headers (engine), then the capture path (app).
4. Update check (Rust), then its UI.
5. Bundling: extension resource, licences, NSIS config and hook, `npm run release`, the checklist.
6. Build the installer, install, the automated checks that can run against it, and the owner checks.
