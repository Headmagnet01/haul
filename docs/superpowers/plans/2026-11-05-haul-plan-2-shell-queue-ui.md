# Haul Plan 2: Tauri Shell + Queue UI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A Windows desktop app, `haul`, that runs the plan-1 engine behind a Svelte Queue view. The view has live segment strips, a detail drawer and a status bar, and you add downloads by pasting or dropping links.

**Architecture:**
- **App crate:** a second workspace member, `app/src-tauri` (`haul-app`), depends on `haul-engine`.
- **No Tauri types in the logic:** all app logic lives in plain Rust modules: `core` (commands as methods), `dto` (what the UI sees) and `bridge` (batches engine events into one update every 250 ms). Tests run on them without Tauri; `commands.rs` and `lib.rs` are thin Tauri glue.
- **UI:** Svelte 5 + TypeScript under `app/src`. It talks to a `Backend` interface, with a Tauri implementation and an in-memory mock, so Vitest and Playwright run without the native app.
- **Pure UI logic:** formatting, status words, strip geometry, list building, connection speeds and key mapping are pure TS in `app/src/lib`, unit-tested with Vitest.

**Tech Stack:**
- Rust: Tauri 2, `tauri-plugin-opener`, `serde`, `tracing-subscriber`, `tracing-appender`.
- Front end: Svelte 5, Vite, TypeScript, `@tauri-apps/api` 2, `@tauri-apps/cli` 2 (dev dependency, run via `npx tauri`), `@fontsource/ibm-plex-sans` and `@fontsource/ibm-plex-mono`.
- Tests: Vitest, `@playwright/test` with `channel: 'msedge'` (the installed Edge, so no browser download), `@axe-core/playwright`.

**Spec:** `docs/superpowers/specs/2026-11-05-haul-design.md`: section 7 (user interface) and milestone 3, plus `DESIGN.md` and the mockup at `docs/mockup/index.html`.
- The plan-1 handoff said "sections 5–6"; that was wrong. Sections 5–6 are plans 3 and 5.
- Later plans: 3 = Collector + clipboard, 4 = post-processing + rules, 5 = local API + extension, 6 = settings, tray, Finished view, "when done", packaging.

**Not in this plan (on purpose):**
- **Later plans' views:** Collector, Finished, Rules and Settings show a one-line placeholder in the rail's views.
- **Status-bar controls:** no clipboard toggle (plan 3), no "When done" selector and no tray (plan 6). The status bar shows only controls that work.
- **Drawer and rail extras:** no "After download" checklist (plan 4) and no categories in the rail (plan 4).
- **Window close:** closing the window quits Haul cleanly. Minimising to the tray arrives in plan 6.
- **Browser-related pieces:** the pairing status in the rail foot, the first-run "Pair your browser" step and the "Refresh from browser" action for expired links all arrive in plan 5.
- **Deleting finished files:** moving a finished file to the Recycle Bin is part of the Finished view (plan 6). Delete in the Queue only removes the row.

## Global Constraints

- Layout: rail **208px**, drawer **340px**, status bar **40px**, minimum window **1080×640**. Default window size is 1280×800, titled `Haul`.
- Rows: file rows **40px**, package rows **44px**, **20px** horizontal padding.
- Columns: Name, Size, Progress, Speed, Left, Status, using the mockup grid `minmax(220px,1.6fr) 92px minmax(180px,1.3fr) 86px 64px 118px`.
- Fonts: IBM Plex Sans and Mono, weights 400/500/600, **bundled** through `@fontsource` (never loaded from the web). Every figure uses `font-variant-numeric: tabular-nums`.
- Colour tokens: as in `DESIGN.md`, with the extra mockup tokens (`--signal-ink`, `--signal-soft`, `--ok-soft`, `--err-soft`, `--focus`), **except** `--ink-3`, which becomes `#6B6559` light and `#8F8B82` dark (see the rulings).
- **Status words are `--ink-2`.** The status icon carries `--signal`/`--ok`/`--idle`/`--err`, and only error words use `--err`. Every status is an icon plus a word.
- Segment strip: **10px** tall with a **2px** gap. Cell width is proportional to the range, and the fill is `--signal` active, `--ok` complete, `--idle` paused, `--err` error. Adjacent fully written ranges merge into one cell.
- Drawer segment map: **34px**-tall cells with a vertical fill.
- Live updates: the backend batches engine events into **one `haul:update` event every 250 ms**. Fills do not animate between updates.
- Motion: hover and toggle transitions are 150ms. `prefers-reduced-motion` turns every animation and transition off.
- Theme: follows Windows (`prefers-color-scheme`). The manual override is plan 6.
- Defaults: speed limit `{ enabled: false, bps: 10_000_000 }` (shown as "10 MB/s"); 8 connections per download; 3 parallel downloads; the save folder is Windows' Downloads folder.
- Settings keys in the engine's `settings` table: `speed_limit` → `{"enabled":bool,"bps":u64}`, `download_dir` → string, `max_parallel` → integer.
- Several pasted links become one package named **`Pasted list · <d Mon HH:MM>`** (local time, e.g. `Pasted list · 5 Nov 18:40`). A single link has no package.
- Units are decimal (1 MB = 1,000,000 bytes), as in the mockup.
- Never destroy data: Delete removes the row (the engine keeps completed files and deletes only the unfinished `.haulpart`), and only after the 6-second Undo window.
- Tauri identifier `dev.haul.desktop`; product name `Haul`; the binary is `haul`.

## Review Focus

1. **Event bursts beyond the engine's 1024-event broadcast buffer** (20 downloads × 4 progress events/s plus steals). Expected: the bridge notices the lag, the UI reloads a full snapshot, and no row stays stale. Tests: `lagged_receiver_requests_resync` (Task 3) and `unknown_id_requests_resync` (Task 5).
2. **Typing in the filter or the add-links box.** Expected: Space types a space and Delete deletes a character; neither pauses nor removes a download. Tests: `keys_ignored_in_text_fields` (Task 5) and the e2e `typing_in_filter_does_not_pause` (Task 7).
3. **Messy pasted text**: links inside quotes, angle brackets, markdown or parentheses, trailing punctuation, duplicates, non-HTTP schemes, or no links at all. Expected: exactly the HTTP(S) links, deduplicated, in order, and "No links found" when there are none. Tests: `extract_*` (Task 1) and `paste_without_links_says_so` (Task 7).
4. **Downloads with no known size.** Expected: a single empty strip cell, a written-bytes figure instead of a percentage, "—" for Size and Left, and no `NaN` or `Infinity` anywhere. Tests: `strip_unknown_size` and `format_handles_unknown` (Task 5).
5. **Closing the window mid-download.** Expected: the queue shuts down cleanly, segments are saved, and the next launch shows the download Paused and resumes it without restarting. Test: the close-and-relaunch step of the smoke run (Task 10).

---

### Task 1: Engine additions: link extraction, reorder, packages, created_at

**Files:**
- Create: `crates/engine/src/links.rs`
- Modify: `crates/engine/src/lib.rs` (export `links::extract_links`, `store::Package`)
- Modify: `crates/engine/src/store/mod.rs` (`Download.created_at`, `Package`, `list_packages`, `reorder`)
- Modify: `crates/engine/src/queue.rs` (`Queue::reorder`)
- Test: unit tests inside `links.rs` and `store/mod.rs`; `crates/engine/tests/queue.rs`

**Interfaces:**
- Produces:
  - `pub fn extract_links(text: &str) -> Vec<String>`
  - `pub struct Package { pub id: PackageId, pub name: String, pub source: String, pub created_at: String }`
  - `Store::list_packages(&self) -> Result<Vec<Package>, EngineError>`
  - `Store::reorder(&self, ids: &[DownloadId]) -> Result<(), EngineError>`
  - `Download.created_at: String` (the SQLite `created_at` text, UTC `YYYY-MM-DDTHH:MM:SSZ`)
  - `Queue::reorder(&self, ids: &[DownloadId])` (calls `Store::reorder`, then wakes the dispatcher)

- [ ] **Step 1: Write the failing tests** in `links.rs`:

```rust
#[test] fn extract_plain_and_trailing_punctuation() {
    assert_eq!(extract_links("get https://a.org/x.zip and http://b.net/y.iso."),
               vec!["https://a.org/x.zip", "http://b.net/y.iso"]);
}
#[test] fn extract_wrapped() {
    assert_eq!(extract_links("<https://a.org/x> \"https://a.org/q?x=1&y=2\", [f](https://a.org/f.bin)"),
               vec!["https://a.org/x", "https://a.org/q?x=1&y=2", "https://a.org/f.bin"]);
}
#[test] fn extract_keeps_balanced_parens() {
    assert_eq!(extract_links("(see https://en.wikipedia.org/wiki/Foo_(bar))"),
               vec!["https://en.wikipedia.org/wiki/Foo_(bar)"]);
}
#[test] fn extract_dedupes_and_normalises() {
    assert_eq!(extract_links("HTTPS://A.ORG/Up https://a.org/Up\nhttps://a.org/Up"), vec!["https://a.org/Up"]);
}
#[test] fn extract_ignores_other_schemes_and_junk() {
    assert!(extract_links("ftp://a.org/x mailto:x@y.z www.a.org https:// http://").is_empty());
    assert!(extract_links("").is_empty());
}
#[test] fn extract_many() {
    let text: String = (0..500).map(|i| format!("https://a.org/{i}.bin\r\n")).collect();
    assert_eq!(extract_links(&text).len(), 500);
}
```

In `store/mod.rs` tests:
- `reorder_renumbers`: insert a, b, c, d (Normal). `reorder(&[c, a])`, then `list_downloads` ids == `[c, a, b, d]`, positions == `[1, 2, 3, 4]`.
- `packages_listed`: `create_package("p", "paste")`, then `list_packages()` has one row with name `"p"`, source `"paste"` and a `created_at` ending in `Z`.
- `created_at_filled`: `get_download(id).created_at.len() == 20`.

In `tests/queue.rs`:
- `reorder_changes_start_order`: set `max_parallel` 1 and a never-open schedule (`weekdays: vec![]`). Add a, b. Call `q.reorder(&[b, a])`, then `set_schedule(None)`. `b` reaches Running first.

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p haul-engine links reorder packages created_at`
Expected: compile errors for the missing items.

- [ ] **Step 3: Implement**
- `extract_links`:
  - Find every case-insensitive `http://` or `https://`. Take characters until whitespace or one of `"'<>` and backtick.
  - Then repeatedly strip a trailing `.,;:!?`, or a trailing `)`/`]`/`}` that has no matching opener inside the candidate.
  - Keep it if `url::Url::parse` succeeds with a non-empty host. Push `url.to_string()`, skipping duplicates and keeping first-seen order.
- `reorder`, in one transaction: give `ids` positions `1..=k` in order, then every other row `k+1..` in its current `(priority, position, id)` order.
- `created_at`: append it to `DOWNLOAD_COLS` as index 20.

- [ ] **Step 4: Run the engine suite**

Run: `cargo test -p haul-engine` and `cargo clippy -p haul-engine --all-targets -- -D warnings`
Expected: all green, clippy clean.

- [ ] **Step 5: Commit**: `feat(engine): link extraction, reorder, package listing`

---

### Task 2: App scaffold: workspace member, Vite + Svelte, tokens, fonts, test runners

**Files:**
- Modify: `Cargo.toml` (members += `"app/src-tauri"`)
- Create: `app/package.json`, `app/vite.config.ts`, `app/tsconfig.json`, `app/svelte.config.js`, `app/index.html`, `app/playwright.config.ts`, `app/.env.mock` (`VITE_BACKEND=mock`)
- Create: `app/src/main.ts`, `app/src/App.svelte`, `app/src/app.css`, `app/src/components/Rail.svelte`, `app/src/components/Placeholder.svelte`, `app/src/components/Icon.svelte`
- Create: `app/src-tauri/Cargo.toml`, `build.rs`, `tauri.conf.json`, `capabilities/default.json`, `src/main.rs`, `src/lib.rs`, `icons/*` (generated)
- Create: `app/app-icon.svg` (the mockup mark, an orange `#D9520B` right-pointing triangle on `#1C1B19`)
- Create: `app/e2e/shell.spec.ts`, `app/src/lib/sanity.test.ts`
- Modify: `DESIGN.md` (the `--ink-3` values and the status-word rule)

**Interfaces:**
- Produces:
  - npm scripts, run in `app/`:
    - `dev` (vite, port 5173);
    - `dev:mock` (`vite --mode mock --port 5174`);
    - `build` (`vite build`);
    - `check` (`svelte-check`);
    - `test` (`vitest run`);
    - `e2e` (`playwright test`);
    - `tauri` (`tauri`).
  - The Rust crate is `haul-app`, with lib name `haul_app_lib` and binary `haul`. `main.rs` calls `haul_app_lib::run()`.
  - `Icon.svelte` takes `name`, one of `queue | collector | finished | rules | settings | dl | ok | err | pause | play | wait | chev | plus | search | folder | link | x`. Each is a 16px viewBox with a 1.5px stroke in `currentColor`. Use the paths from the mockup where it has them, and draw the rest in the same style.

- [ ] **Step 1: Install**
  - In `app/`: `npm init -y`, then `npm install` the stack above (dev dependencies for everything except `@tauri-apps/api` and `@fontsource/*`).
  - Create the Rust crate with `tauri = "2"`, `tauri-build = "2"`, `serde`, `serde_json`, and `haul-engine = { path = "../../crates/engine" }`.
  - Set `tauri.conf.json`:
    - `build.devUrl`: `http://localhost:5173`; `build.frontendDist`: `../dist`; `beforeDevCommand`: `npm run dev`; `beforeBuildCommand`: `npm run build`.
    - The window: `minWidth` 1080, `minHeight` 640, 1280×800.
    - CSP: `default-src 'self'; style-src 'self' 'unsafe-inline'; font-src 'self' data:; img-src 'self' data:`.
  - Generate icons with `npx tauri icon app-icon.svg`. If the CLI rejects SVG, render a 1024×1024 PNG with Playwright's `page.screenshot` and pass that instead.

- [ ] **Step 2: Write the failing tests**
  - `sanity.test.ts`: `expect(1 + 1).toBe(2)` (this proves Vitest runs).
  - `e2e/shell.spec.ts` → `shell_renders`:
    - against `dev:mock`, the page has `role=navigation[name="Views"]` with the buttons Queue, Collector, Finished, Rules and Settings;
    - Queue has `aria-current="page"`;
    - clicking Rules shows a heading `Rules` and the text `Arrives in a later version.`;
    - `getComputedStyle(document.body).fontFamily` starts with `"IBM Plex Sans"`.
  - `playwright.config.ts`:
    - `webServer: { command: 'npm run dev:mock', port: 5174, reuseExistingServer: true }`;
    - `use: { baseURL: 'http://localhost:5174', channel: 'msedge' }`;
    - projects `light` and `dark` (`colorScheme`).

- [ ] **Step 3: Run them to see them fail**: `npm run e2e`. Expected: FAIL (no page yet).

- [ ] **Step 4: Implement the shell**
  - `app.css`: the tokens from the mockup with the fixed `--ink-3`, dark values under `@media (prefers-color-scheme: dark)`, the reduced-motion rule, and the `:focus-visible` ring.
  - `main.ts` imports the 400/500/600 weights of both fonts.
  - `App.svelte`: the grid `208px 1fr 340px` / `1fr 40px`, with the drawer column only on the Queue view.
  - `Rail.svelte`: the mark, the views nav and an empty foot.
  - `Placeholder.svelte`: an `h1` with the view name plus `Arrives in a later version.`
  - `lib.rs`: `tauri::Builder::default().run(tauri::generate_context!())`.
  - `DESIGN.md`: update both `--ink-3` cells, and add to "Colour rules": "Status words are `--ink-2`; the icon carries the colour; only error words use `--err` (AA contrast)."

- [ ] **Step 5: Verify**
  - `npm test`, `npm run e2e` and `npm run check` pass.
  - `cargo check -p haul-app` passes.
  - `npx tauri dev` opens a 1280×800 window showing the rail. Close it by hand.

- [ ] **Step 6: Commit**: `feat(app): Tauri + Svelte shell with tokens, fonts and test runners`

---

### Task 3: App core, DTOs, settings, event bridge (plain Rust)

**Files:**
- Create: `app/src-tauri/src/dto.rs`, `app/src-tauri/src/core.rs`, `app/src-tauri/src/bridge.rs`
- Modify: `app/src-tauri/src/lib.rs` (`mod` lines only)
- Modify: `app/src-tauri/Cargo.toml` (`chrono`, `url`, `tokio` with `full`; dev: `tempfile`)
- Test: unit tests in each file

**Interfaces:**
- Consumes: `Queue`, `Store`, `TransferCtx`, `extract_links`, `Store::list_packages`, `Queue::reorder` (Task 1)
- Produces (every DTO is `#[serde(rename_all = "camelCase")]`):

```rust
// dto.rs
pub struct DownloadView { pub id: i64, pub package_id: Option<i64>, pub name: String, pub url: String, pub host: String,
    pub size: Option<u64>, pub written: u64, pub state: String /* DownloadState::as_str */, pub error: Option<String>,
    pub priority: String /* "high"|"normal"|"low" */, pub position: i64, pub connections: u32,
    pub resumable: Option<bool>, pub target_dir: String, pub target_path: Option<String>, pub created_at: String,
    pub segments: Vec<[u64; 3]> /* start, pos, end */ }
pub struct PackageView { pub id: i64, pub name: String, pub created_at: String }
pub struct SpeedLimit { pub enabled: bool, pub bps: u64 }
pub struct Snapshot { pub packages: Vec<PackageView>, pub downloads: Vec<DownloadView>, pub speed_limit: SpeedLimit }
pub struct ProgressView { pub id: i64, pub written: u64, pub size: Option<u64>, pub speed_bps: u64, pub segments: Vec<[u64; 3]> }
pub struct StateView { pub id: i64, pub state: String, pub error: Option<String> }
pub struct StealView { pub id: i64, pub bytes: u64 }
pub struct Update { pub progress: Vec<ProgressView>, pub states: Vec<StateView>, pub steals: Vec<StealView>, pub resync: bool }
pub struct AddResult { pub added: usize, pub package_id: Option<i64> }

// core.rs: must be created inside a Tokio runtime (Queue::start spawns)
impl AppCore {
    pub fn start(db_path: &Path, default_download_dir: PathBuf) -> Result<AppCore, EngineError>;
    pub fn start_with(store: Store, default_download_dir: PathBuf, schedule: Option<Schedule>) -> AppCore; // tests pass a never-open schedule
    pub fn events(&self) -> EventBus;
    pub fn snapshot(&self) -> Result<Snapshot, EngineError>;
    pub fn add_links(&self, text: &str) -> Result<AddResult, EngineError>;
    pub fn pause(&self, ids: &[i64]); pub fn resume(&self, ids: &[i64]); pub fn remove(&self, ids: &[i64]);
    pub fn pause_all(&self);
    pub fn set_priority(&self, id: i64, priority: &str) -> Result<(), EngineError>; // "high" | "normal" | "low", else Err
    pub fn reorder(&self, ids: &[i64]);
    pub fn set_speed_limit(&self, limit: SpeedLimit) -> Result<(), EngineError>; // persists + applies
    pub fn folder_target(&self, id: i64) -> Result<FolderTarget, EngineError>;
    pub async fn shutdown(&self);
}
pub enum FolderTarget { RevealFile(PathBuf), OpenDir(PathBuf) }

// bridge.rs
pub struct Batcher { /* latest progress per id, states in order, steals in order, resync flag */ }
impl Batcher { pub fn push(&mut self, e: &Event); pub fn mark_lagged(&mut self); pub fn take(&mut self) -> Option<Update>; }
pub fn spawn_forwarder(events: EventBus, period: Duration, emit: impl Fn(Update) + Send + 'static) -> tokio::task::JoinHandle<()>;
```

Behaviour that tests pin:
- **Download name:** `file_name`, else the probe's `file_name`, else `naming::filename_from(None, url)`.
- **Written:** completed → `size.unwrap_or(sum)`, else the sum of `pos - start` over the saved segments.
- **Settings:** `start` reads `speed_limit`, `download_dir` and `max_parallel` from settings, with the defaults from Global Constraints, and applies the global limit when it's enabled.
- **Package name:** `add_links` with 2 or more links creates the package `Pasted list · {Local::now().format("%-d %b %H:%M")}` with source `"paste"`.

- [ ] **Step 1: Write the failing tests**
  - In `core.rs`, use `start_with(Store::open_in_memory()?, tmpdir, Some(Schedule { start: 00:00, stop: 00:00, weekdays: vec![] }))` inside `#[tokio::test]`, so nothing ever starts downloading:
    - `add_one_link_no_package`: `add_links("x https://a.org/f.bin y")` → `added == 1`, `package_id == None`. The snapshot has 1 download named `f.bin`, state `queued`, priority `normal`, `segments == []`, and `written == 0`.
    - `add_many_links_one_package`: 3 links → one package whose name starts with `Pasted list · `. All 3 downloads have that `package_id`.
    - `add_no_links`: `add_links("nothing here")` → `added == 0`, and no package is created.
    - `speed_limit_persists`: `set_speed_limit({enabled: true, bps: 5_000_000})`. A fresh `AppCore` on the same store has `snapshot().speed_limit == {true, 5_000_000}`, and `ctx.global_limit.rate() == Some(5_000_000)`.
    - `set_priority_rejects_unknown`: `"urgent"` is an `Err`, and `"high"` makes the snapshot priority `"high"`.
    - `folder_target_for_unfinished_is_dir`: a queued row gives `OpenDir(target_dir)`.
  - In `bridge.rs`:
    - `progress_coalesces`: 3 `Progress` events for id 1 and 1 for id 2 → `take()` returns 2 progress views, and id 1's is the last pushed.
    - `states_and_steals_keep_order`: all of them are kept, in order.
    - `empty_take_is_none`.
    - `lagged_receiver_requests_resync`: a bus with 1100 events sent before the forwarder reads them → the first emitted `Update` has `resync == true`. Use a 20 ms period and collect into an `Arc<Mutex<Vec<Update>>>`.

- [ ] **Step 2: Run them to see them fail**: `cargo test -p haul-app`. Expected: compile errors.

- [ ] **Step 3: Implement**
  - `spawn_forwarder` loops on `select!` over `rx.recv()` (push; on `RecvError::Lagged`, `mark_lagged`; on `Closed`, break) and an interval tick (`if let Some(u) = take() { emit(u) }`).
  - An invalid priority returns `EngineError::Io(io::Error::new(InvalidInput, "unknown priority"))`.

- [ ] **Step 4: Run tests and clippy**: `cargo test -p haul-app` and `cargo clippy -p haul-app --all-targets -- -D warnings`. Expected: green.

- [ ] **Step 5: Commit**: `feat(app): core commands, DTOs, settings and event bridge`

---

### Task 4: Tauri wiring: commands, startup, logging, clean exit

**Files:**
- Create: `app/src-tauri/src/commands.rs`
- Modify: `app/src-tauri/src/lib.rs`
- Modify: `app/src-tauri/Cargo.toml` (`tauri-plugin-opener = "2"`, `tracing-subscriber` with `env-filter`, `tracing-appender`)
- Modify: `app/src-tauri/capabilities/default.json` (`core:default`, `opener:default`)

**Interfaces:**
- Consumes: `AppCore`, `spawn_forwarder` (Task 3)
- Produces Tauri commands (the names the UI invokes; arguments are camelCase on the JS side), each returning `Result<T, String>`:
  - `snapshot() -> Snapshot`
  - `add_links(text: String) -> AddResult`
  - `pause(ids: Vec<i64>)`, `resume(ids)`, `remove(ids)`, `pause_all()`
  - `set_priority(id: i64, priority: String)`
  - `reorder(ids: Vec<i64>)`
  - `set_speed_limit(enabled: bool, bps: u64)`
  - `open_folder(id: i64)`: `RevealFile` → `app.opener().reveal_item_in_dir(path)`; `OpenDir` → `app.opener().open_path(dir, None::<&str>)`
- Produces the event `haul:update` with an `Update` payload.

- [ ] **Step 1: Implement `lib.rs::run()`**
  - **Logging:** `tracing_appender::rolling::daily(<app_log_dir>, "haul.log")` with the env filter `info`. Keep the guard in managed state.
  - **Startup, in `setup`:**
    - `let core = tauri::async_runtime::block_on(async { AppCore::start(&app_data_dir.join("haul.db"), download_dir) })?`;
    - `download_dir` is `app.path().download_dir()`, falling back to `home_dir()/Downloads`;
    - `app.manage(core)`;
    - inside `async_runtime::block_on` (it needs the runtime), call `spawn_forwarder(core.events(), 250ms, move |u| { let _ = handle.emit("haul:update", u); })`.
  - **Clean exit:** `.build(..)?.run(|app, e| if let RunEvent::Exit = e { tauri::async_runtime::block_on(app.state::<AppCore>().shutdown()) })`.

- [ ] **Step 2: Implement `commands.rs`**: each command is a one-line call on `State<'_, AppCore>`, mapping errors with `e.to_string()`.

- [ ] **Step 3: Verify**
  - `cargo clippy -p haul-app --all-targets -- -D warnings` is clean, and `cargo test -p haul-app` is green.
  - `npx tauri dev` starts with no panic.
  - `%APPDATA%\dev.haul.desktop\haul.db` exists afterwards, and `%LOCALAPPDATA%\dev.haul.desktop\logs\haul.log*` contains a line.

- [ ] **Step 4: Commit**: `feat(app): Tauri commands, event forwarding and clean shutdown`

---

### Task 5: UI pure logic (format, status, strip, list model, speeds, keys, sparkline)

**Files:**
- Create: `app/src/lib/types.ts`, `format.ts`, `status.ts`, `strip.ts`, `model.ts`, `speeds.ts`, `keys.ts`, `sparkline.ts`, plus a `*.test.ts` file next to each

**Interfaces:**
- Produces:
  - **`types.ts`:** the TS mirrors of the Task 3 DTOs: `DownloadView`, `PackageView`, `Snapshot`, `SpeedLimit`, `Update`, `AddResult`, and `State = 'queued'|'running'|'paused'|'completed'|'failed'|'link_expired'|'offline'`.
  - **`format.ts`:**
    - `formatBytes(n: number|null): string`;
    - `formatSpeed(bps: number): string`;
    - `formatEta(seconds: number|null): string`;
    - `formatProgress(written: number, size: number|null): string`: `"38.0%"`, or `formatBytes(written)` when the size is unknown.
  - **`status.ts`:** `statusOf(d: {state: State, error: string|null}): { icon: IconName, word: string, tone: 'signal'|'ok'|'idle'|'err'|'none' }`.
  - **`strip.ts`:**
    - `type Cell = { flex: number, fill: number }`;
    - `stripCells(segments: [number,number,number][], size: number|null, state: State): Cell[]`;
    - `packageCells(children: DownloadView[]): Cell[]`;
    - `stripKind(state: State, error: string|null): 'active'|'done'|'paused'|'error'|'queued'`.
  - **`model.ts`:**
    - `type ListItem = { kind: 'package', pkg: PackageView, children: DownloadView[], open: boolean } | { kind: 'file', d: DownloadView, child: boolean }`;
    - `buildList(snap: Snapshot, opts: { collapsed: Set<number>, filter: string, hidden: Set<number> }): ListItem[]`;
    - `applyUpdate(byId: Map<number, DownloadView>, speeds: Map<number, number>, u: Update): { resync: boolean }`;
    - `summary(ds: DownloadView[], speeds: Map<number, number>): { downloading: number, waiting: number, attention: number, totalBps: number }`;
    - `moveIds(order: number[], id: number, delta: -1|1): number[]`.
  - **`speeds.ts`:** `class RangeSpeeds { update(id: number, segs: [number,number,number][], nowMs: number): void; get(id: number): { start: number, bps: number, done: boolean }[]; forget(id: number): void }`.
  - **`keys.ts`:**
    - `type Action = 'up'|'down'|'toggle'|'remove'|'expand'|'collapse'|'move-up'|'move-down'|'focus-filter'`;
    - `keyAction(e: { key: string, altKey: boolean, ctrlKey: boolean, shiftKey: boolean, target: { tagName: string, isContentEditable?: boolean } | null }): Action | null`.
  - **`sparkline.ts`:** `sparkPoints(samples: number[], width: number, height: number): string` (SVG polyline points).

- [ ] **Step 1: Write the failing tests** (the exact values to pin):

```ts
// format.test.ts
expect(formatBytes(999)).toBe('999 B'); expect(formatBytes(15_400)).toBe('15 KB');
expect(formatBytes(4_560_000)).toBe('4.6 MB'); expect(formatBytes(412_000_000)).toBe('412 MB');
expect(formatBytes(6_100_000_000)).toBe('6.10 GB'); expect(formatBytes(null)).toBe('—');
expect(formatSpeed(0)).toBe('—'); expect(formatSpeed(512_000)).toBe('512 KB/s'); expect(formatSpeed(41_230_000)).toBe('41.2 MB/s');
expect(formatEta(null)).toBe('—'); expect(formatEta(42)).toBe('42s'); expect(formatEta(125)).toBe('2m 5s'); expect(formatEta(7380)).toBe('2h 3m');
// format_handles_unknown
expect(formatProgress(3_000_000, null)).toBe('3.0 MB'); expect(formatProgress(0, 0)).toBe('100.0%');
expect(formatEta(Infinity)).toBe('—'); expect(formatEta(NaN)).toBe('—');

// status.test.ts: the full table
const s = (state, error = null) => statusOf({ state, error });
expect(s('queued')).toEqual({ icon: 'wait', word: 'Waiting', tone: 'none' });
expect(s('running')).toEqual({ icon: 'dl', word: 'Downloading', tone: 'signal' });
expect(s('paused')).toEqual({ icon: 'pause', word: 'Paused', tone: 'idle' });
expect(s('paused', 'disk full')).toEqual({ icon: 'pause', word: 'Disk full', tone: 'err' });
expect(s('completed')).toEqual({ icon: 'ok', word: 'Complete', tone: 'ok' });
expect(s('failed', 'http status 500')).toEqual({ icon: 'err', word: 'Failed', tone: 'err' });
expect(s('link_expired')).toEqual({ icon: 'err', word: 'Link expired', tone: 'err' });
expect(s('offline')).toEqual({ icon: 'err', word: 'Offline', tone: 'err' });

// strip.test.ts
expect(stripCells([[0,50,100],[100,100,200]], 200, 'running')).toEqual([{flex:100,fill:0.5},{flex:100,fill:0}]);
expect(stripCells([[0,100,100],[100,200,200],[200,250,300]], 300, 'running')).toEqual([{flex:200,fill:1},{flex:100,fill:0.5}]); // done ranges merge
expect(stripCells([], 500, 'queued')).toEqual([{flex:1,fill:0}]);
expect(stripCells([], 500, 'completed')).toEqual([{flex:1,fill:1}]);
// strip_unknown_size
expect(stripCells([[0,300,Number.MAX_SAFE_INTEGER]], null, 'running')).toEqual([{flex:1,fill:0}]);
expect(stripKind('paused','disk full')).toBe('error'); expect(stripKind('completed',null)).toBe('done');

// model.test.ts
// - buildList: package rows come before their children; a package sits at its first child's list position;
//   collapsed packages have no child items; filter 'iso' (case-insensitive, by name) keeps a package only if a
//   child matches; hidden ids are left out.
// - applyUpdate: progress sets written/segments and speeds.set(id, speedBps); a state to non-running deletes the speed.
// - unknown_id_requests_resync: an update for an id not in byId → { resync: true }; u.resync → { resync: true }.
// - summary: downloading = running; waiting = queued; attention = failed + link_expired + offline + paused-with-error;
//   totalBps = sum of the speeds.
// - moveIds([1,2,3], 3, -1) → [1,3,2]; moveIds([1,2,3], 1, -1) → [1,2,3].

// speeds.test.ts: update(1, [[0,0,100]], 0); update(1, [[0,40,100]], 1000) → get(1) = [{start:0, bps:40, done:false}];
// a range that finishes reports done:true and bps 0; a new start (a steal) appears with bps 0 until its second sample.

// keys.test.ts
const k = (key, mods = {}, tag = 'DIV') => keyAction({ key, altKey: false, ctrlKey: false, shiftKey: false, ...mods, target: { tagName: tag } });
expect(k(' ')).toBe('toggle'); expect(k('Delete')).toBe('remove'); expect(k('ArrowDown')).toBe('down');
expect(k('ArrowUp', { altKey: true })).toBe('move-up'); expect(k('ArrowRight')).toBe('expand'); expect(k('f', { ctrlKey: true })).toBe('focus-filter');
// keys_ignored_in_text_fields
expect(k(' ', {}, 'INPUT')).toBeNull(); expect(k('Delete', {}, 'TEXTAREA')).toBeNull();
expect(keyAction({ key: ' ', altKey: false, ctrlKey: false, shiftKey: false, target: { tagName: 'DIV', isContentEditable: true } })).toBeNull();

// sparkline.test.ts
expect(sparkPoints([0, 10], 84, 20)).toBe('0.0,20.0 84.0,2.0'); expect(sparkPoints([], 84, 20)).toBe('');
expect(sparkPoints([0, 0], 84, 20)).toBe('0.0,20.0 84.0,20.0');
```

The sparkline rule is `y = height - v / (max * 1.1) * height`, so the peak sits 10% below the top. When the maximum is 0, every point is at `y = height`.

Unknown sizes: the engine sends `end = u64::MAX` for an unknown size, which reaches TS as a number above 2^53. Treat any `size === null` as unknown and never divide by it.

- [ ] **Step 2: Run them to see them fail**: `npm test`. Expected: FAIL (modules missing).
- [ ] **Step 3: Implement the modules** to the signatures above.
- [ ] **Step 4: Run** `npm test` and `npm run check`. Expected: PASS.
- [ ] **Step 5: Commit**: `feat(ui): pure logic for formatting, status, strips, list model and keys`

---

### Task 6: Backend interface, Tauri backend, mock backend

**Files:**
- Create: `app/src/lib/backend.ts`, `app/src/lib/backend-tauri.ts`, `app/src/lib/backend-mock.ts`, `app/src/lib/backend-mock.test.ts`

**Interfaces:**
- Consumes: `types.ts` (Task 5); the Task 4 command names
- Produces:

```ts
export interface Backend {
  snapshot(): Promise<Snapshot>;
  addLinks(text: string): Promise<AddResult>;
  pause(ids: number[]): Promise<void>; resume(ids: number[]): Promise<void>; remove(ids: number[]): Promise<void>;
  pauseAll(): Promise<void>; setPriority(id: number, p: 'high'|'normal'|'low'): Promise<void>;
  reorder(ids: number[]): Promise<void>; setSpeedLimit(l: SpeedLimit): Promise<void>; openFolder(id: number): Promise<void>;
  onUpdate(cb: (u: Update) => void): () => void; // returns unsubscribe
}
export function getBackend(): Backend; // import.meta.env.VITE_BACKEND === 'mock' ? createMockBackend(...) : tauriBackend
export function createMockBackend(opts: { tickMs: number, demo: boolean }): Backend & { calls: string[] };
```

- **The mock** is a small simulated engine.
  - `demo` comes from the `?demo=1` URL parameter in `getBackend()` and seeds the mockup's items: one running ISO, a 5-file package, a link-expired row and a paused row.
  - Each tick advances running downloads (4 ranges each) and does one steal when a range finishes, emitting `Update`s in the real shape.
  - `addLinks` uses its own regex. A URL containing `404` becomes `offline` and one containing `expired` becomes `link_expired`; everything else gets a size of 50–500 MB derived from the URL, so it's deterministic.
  - Each mock download runs about 6 s at `tickMs` 250.
  - `calls` records method names. `getBackend()` also exposes it as `window.__haulMockCalls`, so the e2e tests can assert on it.
  - The mock keeps the speed-limit setting in `sessionStorage`, so it survives a page reload.

- [ ] **Step 1: Write the failing tests** in `backend-mock.test.ts`, with fake timers:
  - `addLinks('https://a.org/f.bin https://a.org/404.bin')` → `added: 2`, and a package with 2 downloads in the snapshot.
  - After 30 ticks an `Update` has arrived with `progress.length >= 1`.
  - After `pause([id])`, the next update has a state `paused` for that id.
  - The demo snapshot has 1 package and 4 or more downloads.
- [ ] **Step 2: Run them to see them fail**: `npm test`.
- [ ] **Step 3: Implement**. `backend-tauri.ts` wraps `invoke` and `listen('haul:update')`, passing camelCase argument names.
- [ ] **Step 4: Run** `npm test` and `npm run check`. Expected: PASS.
- [ ] **Step 5: Commit**: `feat(ui): backend interface with Tauri and mock implementations`

---

### Task 7: Queue view: rows, package rows, strips, filter, empty state, adding links, keyboard

**Files:**
- Create: `app/src/lib/app-state.svelte.ts`, `app/src/components/QueueView.svelte`, `ListRow.svelte`, `Strip.svelte`, `AddLinks.svelte`, `EmptyState.svelte`
- Modify: `app/src/App.svelte`
- Test: `app/e2e/queue.spec.ts`

**Interfaces:**
- Consumes: `Backend` (Task 6); `buildList`, `applyUpdate`, `summary`, `stripCells`, `packageCells`, `statusOf`, `keyAction`, `format*` (Task 5)
- Produces `app-state.svelte.ts`: `createAppState(backend: Backend)` returning a rune-based object:
  - **Fields:** `snapshot`, `byId`, `speeds`, `selected: number|null`, `collapsed`, `filter`, `hidden`;
  - **Getters:** `list: ListItem[]`, `totals`;
  - **Methods:** `load(): Promise<void>`, `select(id)`, `toggle(id)`, `addText(text): Promise<AddResult>`.
  - On an update with `resync`, it calls `load()` again.

Behaviour:
- **Top bar:** `h1` "Queue". The subtitle is `"{downloading} downloading · {waiting} waiting · {attention} needs attention"`, dropping zero parts, or `"Nothing queued"`. Then the filter box (placeholder `Filter downloads`), a `Pause all` button, and an `Add links` primary button with `<kbd>Ctrl V</kbd>`, as in the mockup.
- **Add links:**
  - The button opens an inline panel under the top bar (not a modal): a textarea, an `Add` button and a `Cancel` button.
  - **Ctrl+V anywhere outside a text field**, and **dropping text, links or `.txt` files anywhere**, go straight to `addText`.
  - After adding, a toast says `Added {n} download(s)` or `No links found`.
- **List:**
  - It uses `role="treegrid"`. Package rows have `aria-level=1` and `aria-expanded`, with child rows at `aria-level=2`.
  - Each row has Name (ellipsis; a `Can't resume` tag when `resumable === false`), Size, Progress (strip + `formatProgress` · `"{n} conn"` while running), Speed, Left (`(size-written)/speed`), and Status (icon + word).
  - A package row shows a strip with one cell per file, `"{done} of {total}"` bytes, the summed speed, and a status of `"{k} of {n} done"`.
- **Selection:** a click or the arrows select a row, which is highlighted with `--sel`. Space pauses or resumes the selection; on a package row it applies to all its files. Right and Left expand and collapse. Ctrl+F focuses the filter.
- **Empty state** (no downloads): `Copy a link or drop one here` plus the line `Press Ctrl+V anywhere to add links.`

- [ ] **Step 1: Write the failing e2e tests** in `queue.spec.ts`:
  - `empty_state_shown`: `/` shows `Copy a link or drop one here`.
  - `paste_adds_downloads`: dispatch a `paste` event with `clipboardData` text `"https://a.org/one.bin https://a.org/two.bin"` on `document.body` → a package row `Pasted list · …` with 2 child rows, and the toast `Added 2 downloads`.
  - `paste_without_links_says_so`: pasting `"hello"` shows the toast `No links found`, and no rows appear.
  - `rows_progress_live`: `/?demo=1`; the running ISO row's progress text changes within 2 s, and its status reads `Downloading`.
  - `space_pauses_and_resumes`: select the running row, press Space, and the status becomes `Paused`; press Space again and it becomes `Downloading` or `Waiting`.
  - `typing_in_filter_does_not_pause`: select the running row, focus the filter, type `"iso "` → the row still reads `Downloading`, and the filter value is `"iso "`.
  - `filter_narrows`: type `sprite` → only the package and its children are visible.
  - `collapse_package`: ArrowLeft on the package row hides its children, and ArrowRight shows them again.
  - `long_name_no_overflow`: add `https://a.org/` + `a`×300 + `.bin` → `document.documentElement.scrollWidth <= innerWidth`.
- [ ] **Step 2: Run them to see them fail**: `npm run e2e -- queue`.
- [ ] **Step 3: Implement** the components. DOM updates come only from `applyUpdate` on each `haul:update`; there are no per-row timers.
- [ ] **Step 4: Run** `npm run e2e`, `npm test` and `npm run check`. Expected: PASS in the light and dark projects.
- [ ] **Step 5: Commit**: `feat(ui): Queue view with live strips, packages, paste/drop and keyboard`

---

### Task 8: Detail drawer and status bar

**Files:**
- Create: `app/src/components/Drawer.svelte`, `SegmentMap.svelte`, `StatusBar.svelte`, `Sparkline.svelte`
- Modify: `app/src/App.svelte`, `app/src/lib/app-state.svelte.ts` (`speedHistory: number[]`: 240 samples, one per update, i.e. 60 s at 4/s; `lastSteal: Map<number, number>`)
- Test: `app/e2e/drawer.spec.ts`

**Interfaces:**
- Consumes: `RangeSpeeds`, `sparkPoints`, `formatSpeed`, `Backend.openFolder/setPriority/setSpeedLimit/pause/resume` (Tasks 5–6)

Behaviour:
- **Drawer, for the selected file:**
  - **Header:** a `Selected` label, the name (`h2`), and `"{written} / {size}"` plus the speed.
  - **"Segment map":** the section label shows `"{n} connections"` with n = unfinished ranges while running. The map has one 34px vertical-fill cell per strip cell. Under it is one line for the latest steal: `A connection finished early and took over half of the largest remaining range ({formatBytes(bytes)}).` It is empty until a steal arrives for this id.
  - **Per-range rows:** `C{i}`, a bar, and `formatSpeed` or `done`.
  - **"Details":**
    - Host;
    - Resumable: `Yes (byte ranges, ETag checked)`, `No (can't resume)`, or `Not checked yet`;
    - Added: local time `HH:MM, d Mon`;
    - Save to: the target path if completed, else the target dir;
    - Priority: a `<select>` with High, Normal and Low.
  - **Actions:** `Pause`/`Resume` `<kbd>Space</kbd>`, `Open folder`, and `Copy link`. Copy link uses `navigator.clipboard.writeText`, then the toast `Link copied`.
- **Drawer, for a package row:** the name, the file count, the summed bytes and the Pause all/Resume all buttons. **Nothing selected:** `Select a download to see its connections.`
- **Status bar:**
  - A down-arrow icon, the total speed and an 84×20 sparkline of the last 60 s.
  - `"{active} active · {waiting} waiting"`.
  - The speed-limit toggle (`button` with `aria-pressed`), labelled `Speed limit` with `formatSpeed(bps)`. Toggling calls `setSpeedLimit({enabled, bps})` and survives a reload.

- [ ] **Step 1: Write the failing e2e tests** in `drawer.spec.ts` (`/?demo=1`):
  - `drawer_shows_selected`: clicking the ISO row puts its name in the drawer `h2`. The segment map has ≥ 1 cell and the label matches `/\d+ connections/`.
  - `steal_note_appears`: within 10 s the drawer shows text starting `A connection finished early`.
  - `priority_change_calls_backend`: choose High → `window.__haulMockCalls` (the mock exposes `calls`) includes `setPriority`.
  - `copy_link`: grant `clipboard-read`/`clipboard-write`, click `Copy link`, and the clipboard text equals the row's URL.
  - `speed_limit_toggle_persists`: click the toggle → `aria-pressed="true"`; after a reload it is still `true` (the mock keeps the setting in `sessionStorage`).
  - `sparkline_draws`: after 2 s the `polyline` `points` attribute is non-empty.
- [ ] **Step 2: Run them to see them fail**: `npm run e2e -- drawer`.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run all checks**: `npm run e2e`, `npm test` and `npm run check`. Expected: PASS.
- [ ] **Step 5: Commit**: `feat(ui): detail drawer with segment map, status bar with sparkline and speed limit`

---

### Task 9: Remove with Undo, reorder, accessibility pass

**Files:**
- Create: `app/src/components/Toast.svelte`
- Modify: `app/src/lib/app-state.svelte.ts` (`removeWithUndo(ids)`, `undo()`, `move(id, delta)`, drag reorder), `ListRow.svelte`, `QueueView.svelte`
- Test: `app/e2e/remove-reorder.spec.ts`, `app/e2e/a11y.spec.ts`

Behaviour:
- **Delete:**
  - Delete (or a `Remove` action in the drawer) adds the ids to `hidden` and shows a toast, `Removed {name}` or `Removed {n} downloads`, with an `Undo` button, for **6 s**. When it expires it calls `backend.remove(ids)`.
  - A second removal commits the pending one first. Undo clears `hidden`.
  - The toast region is `role="status"` `aria-live="polite"`.
- **Reorder:**
  - Alt+↑/↓ moves the selected file one place, or the whole package as a block when a package row is selected. It calls `backend.reorder(fullOrder)`.
  - Rows can be dragged (HTML5 drag-and-drop) to reorder the same way.
  - A row dropped among rows of another priority takes that row's priority (`setPriority` before `reorder`).
- **Accessibility:**
  - Every control is reachable with Tab.
  - The focus ring is the `--focus` 2px outline.
  - Icons are `aria-hidden`, and words carry the meaning.

- [ ] **Step 1: Write the failing e2e tests**
  - `delete_then_undo_restores`: select a row, press Delete → the row is gone and the toast shows `Undo`; click Undo → the row is back; after 7 s `calls` has no `remove`.
  - `delete_commits_after_6s`: press Delete, wait 7 s → `calls` includes `remove`.
  - `alt_arrow_reorders`: with 3 loose downloads, Alt+↑ on the third makes it second, and `calls` includes `reorder`.
  - `a11y.spec.ts`:
    - `contrast_aa_both_themes`: `/?demo=1` in both projects; `new AxeBuilder({ page }).withRules(['color-contrast']).analyze()` → `violations` is empty.
    - `reduced_motion_stops_animation`: `page.emulateMedia({ reducedMotion: 'reduce' })` → the computed `animation-name` of any `.spin` or `.pulse` element is `none`, and `transition-duration` on `.sw` is `0s`.
    - `keyboard_only_flow`: Tab to the list, ArrowDown to a row, Space pauses it, and Tab reaches `Open folder` in the drawer.
- [ ] **Step 2: Run them to see them fail**: `npm run e2e -- remove-reorder a11y`.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run all checks**: `npm run e2e`, `npm test` and `npm run check`. Expected: PASS.
- [ ] **Step 5: Commit**: `feat(ui): undoable remove, reorder, accessibility checks`

---

### Task 10: Real-app smoke run and handoff

**Files:**
- Create: `app/scripts/range-server.mjs`: Node `http` with no dependencies. It serves `/file.bin`, 64 MiB of deterministic bytes (`(i * 31 + 7) % 251`, as in the engine tests), with `Accept-Ranges`, `206` + `Content-Range`, an `ETag`, a throttle of 8 MB/s per response, and the port from argv.
- Create: `app/e2e-real/smoke.spec.ts` and `app/playwright.real.config.ts` (no webServer; `connectOverCDP('http://127.0.0.1:9222')`).
- Modify: `app/package.json` (`"smoke": "playwright test -c playwright.real.config.ts"`)
- Modify: `handoff.md`

- [ ] **Step 1: Write the smoke test**, which drives the real app over WebView2's DevTools port:
  - Paste `http://127.0.0.1:<port>/file.bin` → a row `file.bin`, which reaches `Downloading` within 5 s.
  - At ≥ 30% (from the progress text), press Space → `Paused`. Press Space again → `Downloading`.
  - Wait for `Complete` (60 s timeout). The SHA-256 of `<Downloads>/file.bin` equals the hash of the generated bytes.
- [ ] **Step 2: Run it**
  - Start the server: `node scripts/range-server.mjs 8765`.
  - Start the app in another shell: `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222 npx tauri dev`.
  - Run `npm run smoke`. Expected: PASS.
- [ ] **Step 3: Close-and-relaunch check (Review Focus 5)**
  - Start another download of `/file.bin?2`, close the window at about 40%, and relaunch with `npx tauri dev`.
  - Expected: the row shows `Paused`. Space resumes it, and the server log shows ranged requests starting above 0 (not a restart).
  - Record the result in the handoff.
- [ ] **Step 4: Look at it yourself**
  - Screenshot the running app in both Windows themes, with demo-like real data.
  - Compare against `docs/mockup/index.html`: the rail, the columns, the strip colours, the drawer and the status bar. Fix visible mismatches before going on.
- [ ] **Step 5: Run every check**
  - `cargo test --workspace`
  - `cargo clippy --workspace --all-targets -- -D warnings`
  - `npm test`, `npm run check`, `npm run e2e`
- [ ] **Step 6: Update `handoff.md`**
  - What exists now and how to run it (`npx tauri dev`, the mock with `npm run dev:mock` → `http://localhost:5174/?demo=1`).
  - The rulings from the ledger.
  - Next: plan 3 (Collector + clipboard, spec section 5).
- [ ] **Step 7: Commit**: `test(app): real-app smoke run; docs: handoff after plan 2`

---

## Rulings made while writing this plan

- **Contrast beats the token table.** `--ink-3` fails WCAG AA (2.8:1 light and 3.3:1 dark against the row backgrounds), and orange or green text fails in light mode. So `--ink-3` becomes `#6B6559` / `#8F8B82` (≥ 4.5:1 on every surface), and status words use `--ink-2` with the colour on the icon. Cost: labels are a little darker than in the mockup.
- **Adding links skips the Collector for now.** Pasted links queue immediately, so plan 2 is usable. Plan 3 routes them through the Collector, as the spec says. Cost: no duplicate check or online check until plan 3.
- **Package names:** always `Pasted list · <date time>`. The spec's "common file-name prefix" needs probe results, which arrive with the plan-3 grabber. Cost: generic names for now.
- **Per-connection speeds are per-range speeds.** They're measured in the UI from successive `segments` (keyed by range start); the engine doesn't report per connection. Cost: right after a steal a range shows 0 until its second sample.
- **The status bar shows working controls only.** The clipboard toggle and the "When done" selector come with the plans that make them work. Cost: the status bar looks emptier than the mockup until then.
- **Closing the window quits** (clean `Queue::shutdown`) until the tray exists in plan 6. Cost: none.
- **`extract_links` lives in the engine** (`links.rs`), where plan 3's grabber can use it. Cost: none.
- **E2E runs on the installed Edge** (`channel: 'msedge'`), so Playwright downloads no browsers. Cost: needs Edge, which Windows 11 has.
