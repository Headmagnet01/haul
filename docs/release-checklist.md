# Haul release checklist

Do these in order for every release. The installer is not code-signed yet, so Windows SmartScreen warns on first run ("More info", then "Run anyway").

## 1. The gate

From the repository root (stop `tauri dev` first, or set `CARGO_TARGET_DIR=_tmp/target-alt`):

```bash
cargo test --workspace --no-fail-fast
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check
```

In `app/`:

```bash
npx vitest run
npm run check
npx playwright test
```

In `extension/`: `npm test` and `npm run check`.

The known engine timing flakes (see `handoff.md`) are rerun alone before they are believed.

## 2. Build

Quit every running Haul first (the dev build too): the release build writes `target\release`.

In `app/`, build only through:

```bash
npm run release
```

This is `tauri build --config src-tauri/tauri.release.conf.json`. The overlay gives the installed app its own identifier, `dev.haul.desktop`, and leaves out the test-only window permission. A plain `tauri build` would ship the dev identifier: never use it for a release.

The build first builds the extension (`npm --prefix ../extension run build`) and the front end. The first build downloads NSIS into Tauri's tools folder.

## 3. The installer

- Path: `target\release\bundle\nsis\Haul_<version>_x64-setup.exe`
- SHA-256, for the release notes:

```bash
certutil -hashfile "target\release\bundle\nsis\Haul_0.1.0_x64-setup.exe" SHA256
```

## 4. Install

Run the installer. It installs for the current user only, into `%LOCALAPPDATA%\Haul`, with no admin prompt. Haul's data lives in `%APPDATA%\dev.haul.desktop`; dev runs use `%APPDATA%\dev.haul.desktop.dev` and never touch it.

## 5. Checks on the installed Haul

1. The installer runs with no admin prompt and Haul opens.
2. Settings › Browsers (or the welcome): Open the extension folder opens the installed `extension` folder, and Edge loads it unpacked (`edge://extensions`, Developer mode, Load unpacked).
3. A split 7z package downloads and extracts (the bundled 7-Zip).
4. A finished-download notification is labelled Haul, not Windows PowerShell.
5. Settings › App › Start Haul when I sign in to Windows, on. Then `reg query HKCU\Software\Microsoft\Windows\CurrentVersion\Run /v Haul` shows the path in quotes followed by `--hidden`. Sign out and back in: Haul is in the tray with its window hidden.
6. Settings › App › Check now gives "You have the latest version" (or names the newer release).

## 6. Uninstall

Settings › Apps › Installed apps › Haul › Uninstall. Afterwards Haul is gone from the Start menu, and `reg query HKCU\Software\Microsoft\Windows\CurrentVersion\Run /v Haul` finds nothing.

## 7. Publish

Create a GitHub release on `meraaki410/haul` tagged `v<version>` (for example `v0.2.0`; a tag with `-beta` is ignored by the update check). Attach the installer and put its SHA-256 in the notes. Installed copies of Haul notice the new release within a day.
