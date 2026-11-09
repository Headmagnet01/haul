# Haul release checklist

Do these in order for every release. The installer is not code-signed yet, so Windows SmartScreen warns on first run ("More info", then "Run anyway").

## 1. The gate

Build the extension first. The config tests check the bundled extension against `extension/dist`, which is not in Git, so a stale or missing build would test the wrong files. On a fresh clone the `haul-app` crate doesn't compile at all until `extension/dist` exists.

```bash
npm --prefix extension run build
```

Then, from the repository root (stop `tauri dev` first, or set `CARGO_TARGET_DIR=_tmp/target-alt`):

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

The build also makes `extension/dist-firefox` (the Firefox add-on, same code, Firefox manifest).

Real browsers (in `app/`, with `node scripts/range-server.mjs 8765` and `tauri dev` on DevTools port 9222, as the spec files' headers say): `npx playwright test -c playwright.real.config.ts` runs Edge (`extension.spec.ts`) and Firefox (`firefox.spec.ts`).

## 1b. Sign the Firefox add-on

Firefox only keeps add-ons Mozilla has signed. Once: on addons.mozilla.org, Developer Hub › Manage API Keys, create a key, then set the user environment variables `WEB_EXT_API_KEY` (the JWT issuer) and `WEB_EXT_API_SECRET` (the JWT secret). Never put them in a file in the repository.

Every release, when the extension changed: raise `version` in `extension/static/manifest.json` (Mozilla refuses a version it has already signed), then in `extension/`:

```bash
npm run sign:firefox
```

It builds, uploads the Firefox build as an unlisted add-on, and saves `extension/firefox/haul-firefox.xpi` and `haul-firefox.version`. The installer build refuses to bundle a missing `.xpi` or one whose version differs from the manifest (`check-release.mjs`, run as Tauri's `beforeBundleCommand`). The extension didn't change? Keep the signed `.xpi` you have. Since Plan 8 the `.version` file must carry a hash of the build it was signed from; the 0.1.0 one has none, so the first release after Plan 8 must raise the version and sign again.

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
7. Settings › Browsers: Open the Firefox add-on folder opens the installed `firefox` folder. Drag `haul-firefox.xpi` onto a normal Firefox window and choose Add. Click the Haul toolbar button, Pair with Haul, Allow in Haul: Settings › Browsers lists Firefox.
8. Restart Firefox: the add-on is still installed and still paired (its popup says Paired with Haul).
9. In Firefox, a 1 MB+ download from a site you are signed in to lands in Haul and completes (the cookies came along). Repeat it from a Firefox container tab if you use containers.
10. With Haul closed, the same click downloads in Firefox as usual.

## 6. Uninstall

Settings › Apps › Installed apps › Haul › Uninstall. Afterwards Haul is gone from the Start menu, and `reg query HKCU\Software\Microsoft\Windows\CurrentVersion\Run /v Haul` finds nothing.

## 7. Publish

Create a GitHub release on `meraaki410/haul` tagged `v<version>` (for example `v0.2.0`; a tag with `-beta` is ignored by the update check). Attach the installer and put its SHA-256 in the notes. Installed copies of Haul notice the new release within a day.
