# Haul Plan 5: Local API, Pairing and the Browser Extension Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Downloads clicked in Chrome or Edge go to Haul and start at once, carrying the site's cookies; "Download all links…" sends a page's links to the Collector; a one-time Allow/Deny pairing keeps everything else out.

**Architecture:**
- **Engine:**
  - A download's request headers (`{Cookie, Referer, User-Agent}`) are stored DPAPI-encrypted in a new `downloads.headers_blob`. `headers_json` stays the in-memory field, so every existing caller and the transfer code (which already sends `headers_json` on the probe and on every connection) keep working.
  - The headers are wiped on Completed and Failed.
  - `paired_clients` is a new table in migration 5.
- **App crate:** a new `api/` module.
  - `guard.rs`: the three locks, as pure functions.
  - `pairing.rs`: `PairGate`, one waiting request at a time, answered from the window.
  - `server.rs`: an `axum` router, port fallback and the port file.
  - `AppCore` gains `capture`, `add_browser_links`, `active_count` and the paired-client methods. The routes call those, and so do the Tauri commands.
- **UI:**
  - A pairing dialog driven by a `haul:pair-request` event.
  - A Browsers section in the Rules view.
  - The mock simulates both.
- **Extension** (`extension/`, new):
  - TypeScript, bundled by esbuild into `extension/dist/` (MV3).
  - Pure modules (`decide`, `port`, `handover`, `picker`) hold all the logic and take the browser and network as parameters.
  - Thin entry files (`background`, `content`, `popup`, `options`, `picker-page`) wire them to `chrome.*`.

**Tech Stack:** as in plan 4, plus:
- App: `axum 0.8` (moves from engine dev-dependency to an app dependency), `getrandom 0.3`, `sha2 0.11`. Dev: `reqwest 0.13` with the same features as the engine.
- Extension: `typescript 5`, `esbuild 0.25`, `vitest 5`, `@types/chrome 0.1`. Its own `extension/package.json`; nothing is added to `app/package.json`.

**Spec:** `docs/superpowers/specs/2026-11-08-haul-browser-extension-design.md`. It refines section 6 of `docs/superpowers/specs/2026-11-05-haul-design.md`; where they differ, the plan-5 spec wins. One correction to that spec: the transfer code already sends `headers_json`, so Task 1 adds storage, encryption and wiping, not sending.

**Not in this plan (on purpose):** Firefox; cookies for "Download all links…"; live progress in the popup; a Settings page; store publishing.

## Global Constraints

- Everything in plans 2–4's Global Constraints still holds (tokens, fonts, motion, icon plus word, never destroy data, test-first).
- **Port:** `127.0.0.1` only, first free port in `17654..=17664`. The chosen port is written as plain decimal text to `<app data dir>\api-port`.
- **Routes:**
  - `POST /v1/status` (a GET carries no Origin from an extension) → `{"app":"haul","version":"<CARGO_PKG_VERSION>","paired":bool,"active":n}`
  - `POST /v1/pair` body `{"browser":string}` → `{"token":"<64 lowercase hex>"}`
  - `POST /v1/capture` body `{"url","filename"?,"size"?,"referrer"?,"user_agent"?,"cookies"?}` → `{"id":n}`
  - `POST /v1/links` body `{"links":[string],"page_title"?}` → `{"added":n,"skipped":n}`
- **Locks, checked in this order:**
  1. `Host` must equal `127.0.0.1:<port>` exactly, else `421`.
  2. `Origin` must start with `chrome-extension://`, else `403`.
  3. Bearer token (every route but `/v1/pair` and `/v1/status`): its SHA-256 must match a `paired_clients` row whose `origin` equals the request's `Origin`, else `401`.
  - No CORS headers on any reply.
- **Limits:** body ≤ 2 MiB (`413`); `links.len()` ≤ 5000 (`413`); pairing waits ≤ 60 s; a second waiting pair request gets `429`; Deny or timeout gets `403`; while shutting down, every route gets `503`.
- **Duplicates:** the same `url` captured within 5 s returns the first id. A capture whose `url` matches a download in `link_expired` or `paused` replaces its headers and resumes it.
- **Header JSON keys:** exactly `Cookie`, `Referer`, `User-Agent`, and only the keys present.
- **Logs** never contain cookie values, header values or tokens. A capture logs `host`, `file` and `cookies=<count>`.
- **Link source:** `browser` joins `clipboard`, `paste` and `drop`. The UI's `Source` type gains `'browser'`, shown as "Browser".
- **Extension defaults:** minimum size 1 MiB (1048576 bytes). Types: `zip rar 7z exe msi iso mp4 mkv mp3 flac pdf`. Hand-over timeout 1500 ms. An Alt+click counts for 2000 ms.
- **Extension permissions:** `downloads`, `contextMenus`, `cookies`, `storage`, `activeTab`, `scripting`, `notifications`. Host permissions: `<all_urls>`, `http://127.0.0.1/*`. `minimum_chrome_version` is `"116"`.
- **Browser name** sent on pairing: `"Microsoft Edge"` if `navigator.userAgentData.brands` includes it, else `"Google Chrome"`, else `"Chromium"`.

## Review Focus

1. **A capture racing the browser:** a small file can finish in the browser before `pause` lands. Expected: if `pause` fails because the download is already complete, the extension leaves it alone and sends nothing. (Task 8: `handover_leaves_finished_download_alone`.)
2. **A URL with credentials or odd characters** (`https://u:p@host/f.zip`, spaces, unicode): Expected: Haul stores the URL as given and a capture still succeeds, while the log shows the host only, never `u:p`. (Task 5: `capture_log_has_no_userinfo`.)
3. **Haul restarted on a different port** while the extension holds the old one: Expected: the next capture rescans once and succeeds. (Task 8: `port_rescans_after_failure`.)
4. **A cookie header too large or invalid for HTTP** (a newline in a value): Expected: the capture is still accepted, and invalid header values are dropped rather than failing the download. (Task 1: `invalid_header_values_are_dropped`.)
5. **Pairing answered after the 60 s timeout** (a late click on Allow): Expected: the late answer does nothing, and no token is created. (Task 4: `late_answer_is_ignored`.)

---

### Task 1: Engine, encrypted request headers

**Files:**
- Create: `crates/engine/src/store/migration_5.sql`
- Modify: `crates/engine/src/store/mod.rs` (migration list near line 135, `insert`/row mapping around lines 68–95 and 173–180, `set_completed` line 247, `set_state` line 231)
- Modify: `crates/engine/src/error.rs:41` (401)
- Modify: `crates/engine/tests/support/mod.rs` (`Spec.require_header`)
- Test: `crates/engine/src/store/mod.rs` tests; `crates/engine/tests/transfer.rs`; `crates/engine/tests/resume.rs`

**Interfaces:**
- Produces:
  - `Store::refresh_headers(&self, id: DownloadId, headers_json: Option<&str>) -> Result<(), EngineError>`
  - `Store::find_by_url(&self, url: &str) -> Result<Vec<Download>, EngineError>`, newest first
  - `EngineError::from_status(401)` returns `LinkExpired(401)`
  - Migration 5 also creates `paired_clients` (used by Task 2)
- `NewDownload.headers_json` and `Download.headers_json` keep their meaning (plain JSON in memory). Only their storage changes.

- [ ] **Step 1: Write `migration_5.sql`**

```sql
ALTER TABLE downloads ADD COLUMN headers_blob BLOB;
UPDATE downloads SET headers_json = NULL;
CREATE TABLE paired_clients (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    browser     TEXT NOT NULL,
    origin      TEXT NOT NULL,
    token_hash  TEXT NOT NULL UNIQUE,
    created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    last_seen   TEXT
);
```

- [ ] **Step 2: Write the failing store tests**
  - `headers_are_stored_encrypted`: insert with `headers_json: Some(r#"{"Cookie":"sid=abc"}"#)`. A raw `SELECT headers_json, headers_blob` gives `NULL` and a blob that does not contain the bytes `sid=abc`. `get_download` returns the original JSON.
  - `headers_wiped_on_complete`: after `set_completed`, `get_download(id).headers_json == None`.
  - `headers_wiped_on_failed_kept_on_pause`: `set_state(Failed)` wipes them. `set_state(Paused)`, `LinkExpired` and `Offline` keep them.
  - `refresh_headers_replaces`: `refresh_headers(id, Some(r#"{"Cookie":"new"}"#))`, then `get_download` returns the new JSON.
  - `find_by_url_newest_first`: two downloads of the same URL come back with the later id first.
  - `schema_version_is_5`.
  - Update the existing round-trip test at line ~369/400, which now passes through encryption.
- [ ] **Step 3: Run** `cargo test -p haul-engine store::` and expect these tests to FAIL.
- [ ] **Step 4: Implement:**
  - Add `migration_5.sql` to the migration list.
  - Insert writes `secret::protect(json)` into `headers_blob` and `NULL` into `headers_json`.
  - Row mapping reads `headers_blob` through `secret::unprotect`. On a failed unprotect (another user's DB), log a warning and treat it as `None`.
  - `set_completed` and `set_state(Failed, …)` set `headers_blob = NULL`.
  - Add `refresh_headers` and `find_by_url`.
  - Add `401` to the `LinkExpired` arm of `from_status`.
- [ ] **Step 5: Run** `cargo test -p haul-engine store::` and expect PASS.
- [ ] **Step 6: Write the failing transfer tests.** First add `require_header: Option<(String, String)>` to `Spec`: when set, any request without that exact header value gets `401`.
  - `transfer.rs::cookie_sent_on_probe_and_connections`: `Spec::new(4 MiB)` with `require_header = ("cookie", "sid=abc")` and the download's `headers_json` set to that cookie. The download completes and the file matches.
  - `transfer.rs::missing_cookie_gives_link_expired`: the same server, no headers. The final state is `LinkExpired`.
  - `resume.rs::cookie_sent_on_resume`: pause at >0 bytes, then resume. It completes and the file matches.
  - `transfer.rs::invalid_header_values_are_dropped`: `headers_json` with `{"Cookie":"a=1\nb=2","Referer":"https://x.test/"}`, against a server with no requirement. It completes, with no error.
- [ ] **Step 7: Run** `cargo test -p haul-engine --test transfer --test resume` and expect PASS. These tests may pass the first time they run: the transfer code already sends headers, and Step 4 already mapped 401. That's expected, and they stay as guards. Record it as a `Ruling:` in the ledger.
- [ ] **Step 8: Run** `cargo test --workspace` and `cargo clippy --workspace --all-targets -- -D warnings`. Both should be green.
- [ ] **Step 9: Commit** `feat(engine): request headers stored encrypted, wiped when done; 401 is link expired`

### Task 2: Engine, paired clients store

**Files:**
- Create: `crates/engine/src/store/clients.rs`
- Modify: `crates/engine/src/store/mod.rs` (`mod clients; pub use clients::PairedClient;`), `crates/engine/src/lib.rs` (re-export)

**Interfaces:**
- Produces:
  - `pub struct PairedClient { pub id: i64, pub browser: String, pub origin: String, pub created_at: String, pub last_seen: Option<String> }`
  - `Store::add_paired_client(&self, browser: &str, origin: &str, token_hash: &str) -> Result<i64, EngineError>`
  - `Store::paired_client_by_hash(&self, token_hash: &str) -> Result<Option<PairedClient>, EngineError>`
  - `Store::touch_paired_client(&self, id: i64) -> Result<(), EngineError>` (sets `last_seen` to now)
  - `Store::list_paired_clients(&self) -> Result<Vec<PairedClient>, EngineError>` (oldest first)
  - `Store::remove_paired_client(&self, id: i64) -> Result<(), EngineError>`

- [ ] **Step 1: Write the failing tests** in `clients.rs`:
  - `add_then_find_by_hash`
  - `unknown_hash_is_none`
  - `touch_sets_last_seen`
  - `remove_forgets_client`: `paired_client_by_hash` gives `None` after removal
  - `list_is_oldest_first`
- [ ] **Step 2: Run** `cargo test -p haul-engine clients` and expect FAIL (does not compile).
- [ ] **Step 3: Implement** the five methods.
- [ ] **Step 4: Run** the tests again and expect PASS.
- [ ] **Step 5: Commit** `feat(engine): paired browser clients`

### Task 3: App, capture and browser links in `AppCore`

**Files:**
- Modify: `app/src-tauri/src/core.rs` (`SOURCES` line 26, new methods, tests module)
- Modify: `crates/engine/src/grabber/mod.rs:188` (a label parameter)
- Modify: `app/src-tauri/src/dto.rs` (`CaptureRequest`, `PairedClientView`)
- Modify: `app/src/lib/types.ts:71` (`Source` gains `'browser'`) and the place the UI words a source (grep `Clipboard'`), so it shows "Browser"

**Interfaces:**
- Consumes: `Store::find_by_url`, `Store::refresh_headers` (Task 1).
- Produces:
  - `#[derive(Deserialize)] pub struct CaptureRequest { pub url: String, pub filename: Option<String>, pub size: Option<u64>, pub referrer: Option<String>, pub user_agent: Option<String>, pub cookies: Option<String> }`
  - `AppCore::capture(&self, req: CaptureRequest) -> Result<i64, EngineError>`
  - `AppCore::add_browser_links(&self, links: Vec<String>, page_title: Option<&str>) -> Result<IntakeResult, EngineError>`
  - `AppCore::active_count(&self) -> usize` (downloads in `running` or `queued`)
  - `Grabber::intake_labelled(&self, links: Vec<String>, source: &str, label: Option<&str>) -> Result<IntakeResult, EngineError>`. The existing `intake` calls it with `None`.

- [ ] **Step 1: Write the failing tests** in `core.rs` (use `never()` so nothing starts):
  - `capture_queues_with_encrypted_headers`: capture `https://a.org/f.zip` with cookies `sid=1`, referrer and user agent. A download exists whose `headers_json` parses to exactly the keys `Cookie`, `Referer` and `User-Agent`, and its `file_name` is the given filename.
  - `capture_without_extras_has_no_headers`: `headers_json == None`.
  - `capture_twice_within_5s_returns_same_id`: both calls return the same id, and `list_downloads().len() == 1`.
  - `capture_after_5s_adds_new`: use an injectable clock. `AppCore` holds `recent: Arc<Mutex<HashMap<String, (i64, Instant)>>>`, and the test inserts a stale `Instant`. Two downloads result.
  - `recapture_refreshes_expired_download`: set a download's state to `LinkExpired`, then capture the same URL with cookies `new`. It returns the same id, the headers carry `new`, and the state is `queued`.
  - `browser_links_go_to_collector_with_label`: `add_browser_links(vec![…2 urls], Some("My page"))` gives `added == 2`, and the Collector batch label is `My page`.
  - `add_links_accepts_browser_source`.
  - `capture_rejects_non_http_url`: `ftp://x` and `blob:…` give `Err`.
- [ ] **Step 2: Run** `cargo test -p haul-app core::` and expect FAIL.
- [ ] **Step 3: Implement:**
  - `capture` builds `headers_json` from the present fields only (serde_json map).
  - Duplicates:
    - First check `recent`, keyed by URL with a 5 s window.
    - Then `find_by_url` for a `LinkExpired` or `Paused` download: `refresh_headers` plus `queue.resume`.
    - Otherwise `queue.add(NewDownload { package_id: None, priority: Normal, connections: DEFAULT_CONNECTIONS, target_dir: download_dir, … })`.
  - Logging: `tracing::info!(host, file, cookies = n, "captured")`, where `n` counts `;`-separated pairs.
  - Add `"browser"` to `SOURCES` (make it length 4).
- [ ] **Step 4: Run** `cargo test -p haul-app` and expect PASS. Then run `npm test` and `npm run check` in `app/`.
- [ ] **Step 5: Commit** `feat(app): capture downloads and browser links`

### Task 4: App, the locks and pairing gate (pure parts)

**Files:**
- Create: `app/src-tauri/src/api/mod.rs`, `app/src-tauri/src/api/guard.rs`, `app/src-tauri/src/api/pairing.rs`
- Modify: `app/src-tauri/src/lib.rs` (`pub mod api;`), `app/src-tauri/Cargo.toml` (`axum = "0.8.9"`, `getrandom = "0.3"`, `sha2 = "0.11.1"`)

**Interfaces:**
- Produces (`guard.rs`):
  - `pub fn check_host(host: Option<&str>, port: u16) -> bool`
  - `pub fn check_origin(origin: Option<&str>) -> Option<&str>`: returns the origin when it starts with `chrome-extension://` and has a non-empty id.
  - `pub fn bearer(auth: Option<&str>) -> Option<&str>`: the token after `Bearer `.
  - `pub fn hash_token(token: &str) -> String`: SHA-256 as lowercase hex.
  - `pub fn new_token() -> String`: 32 bytes from `getrandom`, as 64 lowercase hex.
- Produces (`pairing.rs`):
  - `pub struct PairGate`, `Clone`, holding `Arc<Mutex<Option<(u64, oneshot::Sender<bool>)>>>` plus a counter
  - `PairGate::new(timeout: Duration) -> PairGate`
  - `PairGate::request(&self) -> Result<PairTicket, PairBusy>`
  - `PairTicket::wait(self) -> impl Future<Output = bool>`: `false` on Deny or timeout
  - `PairGate::answer(&self, allow: bool)`
  - Dropping a ticket (the client went away) frees the gate.

- [ ] **Step 1: Write the failing tests:**
  - `guard.rs`:
    - `host_must_match_exactly`: true only for `127.0.0.1:17654` with port 17654. False for `localhost:17654`, `127.0.0.1:17655`, `evil.test:17654`, `127.0.0.1`, and `None`.
    - `origin_must_be_extension`: accepts `chrome-extension://abcdef`. Rejects `https://a.org`, `null`, `chrome-extension://`, and `None`.
    - `bearer_parses`
    - `hash_is_sha256_hex`: `hash_token("abc")` equals `ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad`.
    - `new_token_is_64_hex_and_unique`
  - `pairing.rs`, with Tokio `start_paused`:
    - `allow_resolves_true`
    - `deny_resolves_false`
    - `timeout_resolves_false`: advance 61 s.
    - `second_request_is_busy`
    - `gate_frees_after_answer`
    - `dropped_ticket_frees_gate`
    - `late_answer_is_ignored`: after a timeout, `answer(true)` does nothing, and a new request is not pre-approved.
- [ ] **Step 2: Run** `cargo test -p haul-app api::` and expect FAIL.
- [ ] **Step 3: Implement** `guard.rs` and `pairing.rs`. The ticket carries the request number, so `answer` only resolves the waiting request.
- [ ] **Step 4: Run** the tests again and expect PASS.
- [ ] **Step 5: Commit** `feat(app): API locks and the pairing gate`

### Task 5: App, the local server

**Files:**
- Create: `app/src-tauri/src/api/server.rs`
- Create: `app/src-tauri/tests/api.rs` (integration tests; they start the server on a free port)
- Modify: `app/src-tauri/Cargo.toml` dev-dependencies (`reqwest = { version = "0.13.5", default-features = false, features = ["rustls-no-provider", "json"] }`, plus `rustls` as in the engine, and `tracing-test = "0.2"` for the log check)

**Interfaces:**
- Consumes: `AppCore::{capture, add_browser_links, active_count}` (Task 3); the guard and `PairGate` (Task 4); the `Store` paired-client methods (Task 2), through new `AppCore` wrappers `pair_client(browser, origin) -> String` (makes a token, stores its hash, returns the token), `client_for(token, origin) -> Option<i64>` (looks up the hash, checks the origin, touches `last_seen`), `paired_clients() -> Vec<PairedClientView>` and `remove_paired_client(id)`.
- Produces:
  - `pub struct ApiHandle { pub port: u16, shutdown: CancellationToken, … }` with `ApiHandle::stop(&self)`
  - `pub async fn start_api(core: AppCore, gate: PairGate, on_pair: Arc<dyn Fn(String) + Send + Sync>, ports: RangeInclusive<u16>, port_file: Option<PathBuf>) -> Result<ApiHandle, EngineError>`
  - `on_pair(browser)` is called when a pair request starts waiting. The Tauri glue (Task 6) emits the event and focuses the window.

- [ ] **Step 1: Write the failing integration tests** in `tests/api.rs`. Each starts `start_api` on the range `p..=p+2` around a free port, with a `never()`-schedule core in a temp dir. Helpers send requests with `Host: 127.0.0.1:<port>` and `Origin: chrome-extension://testid`.
  - `status_needs_no_token`: `200` with `app == "haul"` and `paired == false`.
  - `wrong_host_is_421`: `Host: evil.test:<port>`.
  - `web_origin_is_403` and `missing_origin_is_403`.
  - `capture_without_token_is_401`.
  - `pair_allow_returns_token_then_capture_works`: the `on_pair` callback calls `gate.answer(true)`. The token is 64 hex. Capture with `Authorization: Bearer <token>` returns `200 {id}`, and `status` now says `paired == true`.
  - `pair_deny_is_403`.
  - `second_pair_while_waiting_is_429`.
  - `token_from_other_extension_is_401`: pair as `chrome-extension://a`, use the token from `chrome-extension://b`.
  - `removed_client_token_is_401`.
  - `links_land_in_collector`: `200 {added: 2}`.
  - `too_many_links_is_413`: 5001 links.
  - `body_over_2mib_is_413`.
  - `port_falls_back_when_taken`: hold a `TcpListener` on `p`, and the server binds `p+1`. With `port_file` set, the file contains `p+1`.
  - `all_ports_taken_is_error`.
  - `no_cors_headers`: no `access-control-allow-origin` on any reply.
  - `stopped_server_answers_503_or_refuses`: after `stop()`, a request either fails to connect or gets `503`.
  - `capture_log_has_no_secrets` (`#[traced_test]`): capture `https://u:p@a.org/f.zip` with cookies `sid=topsecret`. The logs contain `a.org`, and contain neither `topsecret` nor `u:p`.
- [ ] **Step 2: Run** `cargo test -p haul-app --test api` and expect FAIL.
- [ ] **Step 3: Implement** `server.rs`:
  - The router has the four routes, plus a `middleware::from_fn_with_state` lock layer and `DefaultBodyLimit::max(2 * 1024 * 1024)`.
  - The layer applies the locks in the order of Global Constraints and puts the `client id` into request extensions for authed routes. `/v1/status` calls `client_for` only when a bearer is present, to set `paired`.
  - `/v1/pair` takes a `gate.request()` ticket (`PairBusy` → `429`), calls `on_pair`, and awaits `wait()`: `false` → `403`, `true` → `pair_client`.
  - `axum::serve(...).with_graceful_shutdown(token.cancelled())`.
  - Call blocking `AppCore` methods via `tokio::task::spawn_blocking`.
  - Error mapping: `EngineError` from `capture` → `400` with `{error}`.
- [ ] **Step 4: Run** the tests again and expect PASS. Then run `cargo clippy --workspace --all-targets -- -D warnings`.
- [ ] **Step 5: Commit** `feat(app): local API server with pairing`

### Task 6: App, Tauri glue and the window

**Files:**
- Modify: `app/src-tauri/src/lib.rs` (start the API in `setup`, stop it on `RunEvent::Exit` before `shutdown`, manage `PairGate`), `app/src-tauri/src/commands.rs`
- Modify: `app/src/lib/types.ts`, `backend.ts`, `backend-tauri.ts`, `backend-mock.ts`
- Create: `app/src/components/PairDialog.svelte`, `app/src/components/BrowserList.svelte`
- Modify: `app/src/App.svelte` (mount `PairDialog`), `app/src/components/RulesView.svelte` (a Browsers `<section aria-labelledby="h-browsers">` after Saved passwords)
- Test: `app/src/lib/backend-mock.test.ts`, `app/e2e/pairing.spec.ts`; add the dialog and section to `app/e2e/a11y.spec.ts`

**Interfaces:**
- Consumes: `start_api`, `PairGate` and the `AppCore` paired-client methods (Task 5).
- Produces:
  - Commands: `pair_answer(allow: bool)`, `paired_clients() -> Vec<PairedClientView>`, `remove_paired_client(id: i64)`
  - Event `haul:pair-request` with payload `{ browser: string }`
  - TS: `PairedClient { id: number; browser: string; createdAt: string; lastSeen: string | null }`
  - `Backend` gains `pairAnswer(allow: boolean): Promise<void>`, `pairedClients(): Promise<PairedClient[]>`, `removePairedClient(id: number): Promise<void>` and `onPairRequest(cb: (browser: string) => void): () => void`
  - The mock exposes `window.__haulMock.pairRequest(browser)` for e2e.

- [ ] **Step 1: Write the failing mock tests:**
  - `pair_request_then_allow_adds_client`: after `pairRequest('Microsoft Edge')` and `pairAnswer(true)`, `pairedClients()` has one entry, `browser: 'Microsoft Edge'`.
  - `deny_adds_nothing`
  - `remove_paired_client_forgets`
- [ ] **Step 2: Write the failing e2e tests** (`pairing.spec.ts`, light and dark like the others):
  - `pair_dialog_allow`: trigger `pairRequest`. A dialog with role `alertdialog` and name `Allow Microsoft Edge to send downloads to Haul?` shows Allow focused. Clicking Allow closes it, and the Rules view's Browsers section lists "Microsoft Edge".
  - `pair_dialog_escape_denies`: Escape closes the dialog, and nothing is listed.
  - `browser_remove`: Remove takes the row away, and the section shows `No browsers paired. Install the Haul extension and press Pair in its popup.`
  - Add both to the axe pass.
- [ ] **Step 3: Run** `npm test` and `npm run e2e -- pairing` in `app/`, and expect FAIL.
- [ ] **Step 4: Implement:**
  - Rust glue: `setup` spawns `start_api` with `on_pair` emitting `haul:pair-request` and calling `unminimize()`, `show()` and `set_focus()` on the main window. The port file is `app_data_dir/api-port`. A failure is logged with `warn!`, and the app runs without the API.
  - `RunEvent::Exit` calls `ApiHandle::stop()` first.
  - The UI uses `DESIGN.md` tokens. The dialog copy is title `Allow <browser> to send downloads to Haul?`, body `It can then add downloads and links. You can remove it later in Rules → Browsers.`, and the buttons are `Allow` and `Deny`.
  - Each list row shows the browser, `Paired <relative date>` and `Last used <relative date or "never">`, plus Remove (no Undo; removing just means pairing again).
- [ ] **Step 5: Run** `npm test`, `npm run check`, `npm run e2e` and `cargo test --workspace`, and expect all green.
- [ ] **Step 6: Commit** `feat(ui): pairing dialog and Browsers list`

### Task 7: Extension scaffold and the catch rule

**Files:**
- Create: `extension/package.json` (scripts: `build` = `node build.mjs`, `test` = `vitest run`, `check` = `tsc --noEmit`), `extension/tsconfig.json`, `extension/build.mjs`, `extension/static/manifest.json`, `extension/src/types.ts`, `extension/src/decide.ts`, `extension/src/decide.test.ts`
- Modify: `.gitignore` (`extension/dist/`, `extension/node_modules/`)

**Interfaces:**
- Produces:
  - `types.ts`:
    - `Options { minSize: number; types: string[] }` and `DEFAULT_OPTIONS`
    - `DownloadFacts { url: string; finalUrl: string; filename: string; size: number | null; incognito: boolean }`
  - `decide.ts`:
    - `decide(f: DownloadFacts, opts: Options, altClicks: { url: string; at: number }[], now: number): 'catch' | 'skip'`
    - `extOf(name: string): string` (lowercase, no dot)
- `build.mjs`:
  - Bundles `src/background.ts` (ESM, `"type": "module"` worker), and `src/content.ts`, `src/popup.ts`, `src/options.ts` and `src/picker-page.ts` (IIFE), into `dist/`.
  - Copies `static/*` (manifest and HTML).
  - Entries that don't exist yet are skipped, so this task builds.

- [ ] **Step 1: Write the failing tests** in `decide.test.ts`:
  - `catches_big_known_size`: 2 MiB `.bin` → `catch`.
  - `skips_small_unlisted`: 10 KiB `.bin` → `skip`.
  - `catches_small_listed_type`: 10 KiB `.zip` → `catch`.
  - `unknown_size_listed_catches`: `.pdf`.
  - `unknown_size_unlisted_skips`: `.bin`.
  - `skips_incognito`
  - `skips_blob_and_data`
  - `skips_non_http`: `file:` and `ftp:`.
  - `skips_alt_clicked_within_2s`: 1999 ms → `skip`, 2001 ms → `catch`. The match is on `url` or `finalUrl`.
  - `ext_from_finalurl_when_filename_empty`
  - `size_threshold_inclusive`: exactly 1048576 → `catch`.
- [ ] **Step 2: Run** `npm install`, then `npm test`, in `extension/`, and expect FAIL. Installing the npm packages listed under Tech Stack is pre-approved by this plan. Ask the owner before anything else.
- [ ] **Step 3: Implement** `decide` and `extOf`. Write the manifest: MV3, name `Haul`, version `0.1.0`, the permissions from Global Constraints, `background.service_worker: "background.js"`, `content_scripts` on `<all_urls>` at `document_start` with `content.js`, `action.default_popup: "popup.html"`, and `options_page: "options.html"`.
- [ ] **Step 4: Run** `npm test`, `npm run check` and `npm run build` in `extension/`, and expect PASS. `dist/manifest.json` should exist.
- [ ] **Step 5: Commit** `feat(extension): scaffold and the catch rule`

### Task 8: Extension, finding Haul and the hand-over

**Files:**
- Create: `extension/src/port.ts`, `extension/src/handover.ts`, `extension/src/api.ts`, plus `*.test.ts` for each

**Interfaces:**
- Produces:
  - `api.ts`:
    - `type Fetcher = (url: string, init: RequestInit & { timeoutMs: number }) => Promise<{ status: number; json(): Promise<unknown> }>`
    - `realFetcher` (uses `AbortController`)
  - `port.ts`:
    - `PORTS = [17654, …, 17664]`
    - `findPort(fetch: Fetcher, last: number | null): Promise<number | null>`: tries `last` first, then each port's `/v1/status`, and takes the first with `app === 'haul'`.
  - `handover.ts`:
    - `interface DownloadsApi { pause(id: number): Promise<void>; resume(id: number): Promise<void>; cancel(id: number): Promise<void>; erase(id: number): Promise<void> }`
    - `interface HaulClient { capture(body: CaptureBody): Promise<'ok' | 'unpaired' | 'failed'> }`
    - `handOver(id: number, body: CaptureBody, d: DownloadsApi, haul: HaulClient): Promise<'sent' | 'kept' | 'gone'>`
    - `makeClient(fetch: Fetcher, storage: { get(): Promise<{ port: number | null; token: string | null }>; set(v: Partial<{ port: number | null; token: string | null }>): Promise<void> }): HaulClient`. It does the 1500 ms timeout, one rescan on a network failure, and clears the token on `401`.
  - `CaptureBody` matches `/v1/capture` in Global Constraints (`cookies` is the joined `name=value; …` string).

- [ ] **Step 1: Write the failing tests:**
  - `port.test.ts`:
    - `uses_last_port_first`: only one fetch when `last` answers.
    - `scans_in_order`
    - `ignores_non_haul_answers`: `app: 'other'`.
    - `null_when_none_answer`
  - `handover.test.ts`:
    - `sent_cancels_and_erases_after_ok`: the call order is `capture`, `cancel`, `erase`, and `resume` is never called.
    - `failed_resumes`
    - `unpaired_resumes`
    - `cancel_never_before_ok`: `capture` is a deferred promise, and `cancel` isn't called until it resolves.
    - `handover_leaves_finished_download_alone`: `pause` rejects, so the result is `gone`, and `capture` is never called.
  - `api.test.ts`:
    - `timeout_after_1500ms_is_failed`: fake timers.
    - `401_clears_token_and_is_unpaired`
    - `port_rescans_after_failure`: the stored port refuses, the scan finds another, the retry succeeds, and the new port is stored.
    - `sends_bearer_and_json`
- [ ] **Step 2: Run** `npm test` in `extension/` and expect FAIL.
- [ ] **Step 3: Implement** `port.ts`, `handover.ts` and `api.ts`. `handOver` calls `pause` first and returns `gone` if it throws.
- [ ] **Step 4: Run** `npm test` and `npm run check`, and expect PASS.
- [ ] **Step 5: Commit** `feat(extension): finding Haul and the safe hand-over`

### Task 9: Extension, wiring, popup, options and link picker

**Files:**
- Create: `extension/src/background.ts`, `extension/src/content.ts`, `extension/src/popup.ts`, `extension/src/options.ts`, `extension/src/picker.ts`, `extension/src/picker.test.ts`, `extension/src/picker-page.ts`, `extension/static/popup.html`, `extension/static/options.html`, `extension/static/picker.html`, `extension/static/style.css`, `extension/static/icons/haul-{16,32,48,128}.png` (draw them from the app's icon in `app/src-tauri/icons/`)

**Interfaces:**
- Consumes: `decide` (Task 7); `findPort`, `makeClient` and `handOver` (Task 8).
- Produces:
  - `picker.ts`:
    - `type Kind = 'archives' | 'video' | 'audio' | 'images' | 'documents' | 'programs' | 'other'`
    - `kindOf(url: string): Kind`, using the category extension lists from plan 4's Global Constraints, with Compressed as archives, Music as audio, Programs as programs, and images `jpg jpeg png gif webp bmp svg avif`
    - `uniqueLinks(hrefs: string[]): string[]`: http(s) only, `#fragment` stripped, first-seen order
  - Messages (`chrome.runtime.sendMessage`):
    - `{ type: 'alt-click', url }` from the content script
    - `{ type: 'pair' }` and `{ type: 'status' }` from the popup
    - `{ type: 'send-links', links, pageTitle }` from the picker
  - `chrome.storage.local` keys: `port`, `token`, `options`, `lastResult` (`{ at, text }`), `pairState` (`'idle' | 'waiting' | 'paired' | 'denied'`)

- [ ] **Step 1: Write the failing tests** in `picker.test.ts`:
  - `kind_of_each_category`: one URL per kind, including a query string (`/a.zip?x=1` → archives).
  - `unique_links_filters_and_dedupes`
- [ ] **Step 2: Run** `npm test` and expect FAIL. **Step 3:** implement `picker.ts`. **Step 4:** run it and expect PASS.
- [ ] **Step 5: Implement the entry files:**
  - **`background.ts`:**
    - `downloads.onCreated`: build `DownloadFacts`. On `catch`, gather `cookies.getAll({ url: finalUrl })`, the referrer (`item.referrer`) and `navigator.userAgent`, then `handOver`.
    - Store `lastResult`: `Sent to Haul` / `Haul didn't answer, so your browser kept it` / `Not paired: press Pair` (wording fixed here).
    - Context menu `haul-link` ("Download with Haul", contexts `link`): capture without pause. On failure show a `chrome.notifications` toast with the `lastResult` text.
    - Context menu `haul-all` ("Download all links…", contexts `page`, `selection`, `link`): `scripting.executeScript` to collect `a[href]` (inside the selection if there is one), then open `picker.html?tab=<id>` with the links stored in `chrome.storage.session` under `picker:<tab>`.
    - `pair`: set `pairState: 'waiting'`, `findPort`, POST `/v1/pair` with no timeout. On 200 store the token and `'paired'`; on 403 or 429 store `'denied'`.
    - Keep the alt-click list in `chrome.storage.session` (the worker may sleep between the click and the download).
  - **`content.ts`:** a `click` listener (capture phase) that sends `alt-click` when `e.altKey` and the target is inside an `a[href]`.
  - **`popup.ts`:** shows `Paired with Haul` / `Not paired` / `Haul isn't running` from `status`, the active count, the `lastResult`, and Pair while not paired. It re-reads `pairState` whenever opened.
  - **`options.ts`:** a minimum size in MB (number input) and a types text field (space- or comma-separated), saved to `options`.
  - **`picker-page.ts`:** a checkbox list with kind filter chips and a select-all toggle. "Send to Haul" posts through the worker, then shows `Sent n links to Haul's Collector`.
  - **Styling:** in `style.css`, copy the colour and font tokens from `DESIGN.md` (light and dark through `prefers-color-scheme`), no frameworks.
- [ ] **Step 6: Run** `npm test`, `npm run check` and `npm run build` in `extension/`, and expect PASS. `dist/` should hold `manifest.json`, all five scripts, the three pages and the icons.
- [ ] **Step 7: Commit** `feat(extension): background wiring, popup, options and link picker`

### Task 10: Real end-to-end check and handoff

**Files:**
- Create: `app/e2e-real/extension.spec.ts`, `app/e2e-real/fixtures/download-page.html` (one link to `http://127.0.0.1:8765/big.bin`)
- Modify: `handoff.md`

**Interfaces:**
- Consumes: everything above; `scripts/range-server.mjs` (serves `/<name>.bin`, a 64 MiB file).

- [ ] **Step 1: Write the test.** `chromium.launchPersistentContext(tmpDir, { channel: 'msedge', headless: false, args: ['--disable-extensions-except=<abs extension/dist>', '--load-extension=<abs extension/dist>'] })`.
  1. Read the extension id from `context.serviceWorkers()[0].url()`.
  2. Open `chrome-extension://<id>/popup.html` and click Pair.
  3. In the Haul window (CDP on 9222, like `smoke.spec.ts`), click Allow.
  4. Open the fixture page served by the range server and click the link.
  5. Check that Haul's Queue shows `big.bin`, it completes, and the file's SHA-256 matches the server's file.
  6. Check that `chrome.downloads.search({})` in the worker finds nothing for that URL.
  - `kept_when_not_paired`: overwrite the stored `token` with `bogus`, then click the link again. Haul answers 401, the browser's download exists and completes, the popup shows `Not paired: press Pair`, and Haul's Queue gains no new row.
- [ ] **Step 2: Run it** (three shells as in the handoff, plus `npm run build` in `extension/`). **If Edge refuses `--load-extension`** (the extension's service worker never appears within 10 s), stop. Ask the owner before installing Playwright's Chromium (`npx playwright install chromium`, about 150 MB), then switch the test to `channel: undefined`.
- [ ] **Step 3: Load the extension by hand** in the owner's Edge (`edge://extensions`, Developer mode, Load unpacked, `extension/dist`). Pair it and capture one real download from a normal site. Screenshot the popup and the Haul row.
- [ ] **Step 4: Run all checks:** `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --check`; in `app/`: `npm test`, `npm run check`, `npm run e2e`, `npm run smoke`; in `extension/`: `npm test`, `npm run check`. All should be green.
- [ ] **Step 5: Update `handoff.md`.** Add a Plan 5 section in the style of Plan 4's: what happens, where the code is, the settings and storage keys, the rulings, the bugs found, and how to load the extension. Set Next to plan 6.
- [ ] **Step 6: Commit** `test: real browser-to-Haul check; docs: handoff after plan 5`
