# Haul

A free, open-source download manager for Windows. It downloads fast over several connections at once, takes over downloads from your browser, collects links into packages, and checks and extracts archives when they finish.

## Features

- **Fast, resumable downloads.** Each file is split across several connections. A crash or a dropped connection never loses the work already done.
- **Browser hand-over.** The Haul extension for Chrome, Edge and Firefox passes big downloads to Haul, cookies included. If Haul isn't running, the browser simply keeps the download.
- **Collector.** Paste or copy a batch of links (or turn on Watch clipboard). Haul checks them, groups them into packages, and starts them when you say so.
- **Post-processing.** Checksums, then automatic extraction of zip, 7z and rar (multi-part too) with the bundled 7-Zip, and Rules that sort finished files into folders.
- **Quiet by default.** No pop-up per download. A tray icon, a toast when something finishes, speed limits, a schedule, and "when done" (sleep or shut down).

## Install

1. Download `Haul_<version>_x64-setup.exe` from [Releases](https://github.com/Headmagnet01/haul/releases) and check its SHA-256 against the release notes.
2. Run it. It installs for your user only, with no admin prompt. The installer isn't code-signed yet, so Windows SmartScreen warns the first time: choose **More info**, then **Run anyway**.
3. Add the browser extension from Haul's **Settings › Browsers**:
   - **Chrome or Edge:** Open the extension folder, then load it on `chrome://extensions` (or `edge://extensions`) with Developer mode on and **Load unpacked**.
   - **Firefox:** Open the Firefox add-on folder, then drag `haul-firefox.xpi` onto a Firefox window.
4. Click the Haul toolbar button, choose **Pair with Haul**, and allow it in Haul.

Haul checks this repository for a newer release once a day (you can turn that off in Settings).

## Build from source

You need Rust (stable), a current Node.js LTS and Windows 10 or 11.

```bash
npm --prefix extension install
npm --prefix extension run build
npm --prefix app install
cd app
npx tauri dev
```

Tests: `cargo test --workspace` at the root, `npx vitest run` and `npx playwright test` in `app/`, and `npm test` in `extension/`. The full release steps are in [docs/release-checklist.md](docs/release-checklist.md).

## Layout

- `crates/engine`: the download engine, queue, store and post-processing (a plain Rust library).
- `app`: the desktop app (Tauri 2 and Svelte).
- `extension`: the browser extension (one source, Chromium and Firefox builds).

## Licence

Dual-licensed under either [MIT](LICENSE-MIT) or [Apache 2.0](LICENSE-APACHE), at your option. The bundled 7-Zip is under its own licence ([License.txt](app/src-tauri/resources/7zip/License.txt): LGPL, with the unRAR restriction).
