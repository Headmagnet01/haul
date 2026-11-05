# Haul Plan 1: Engine Core Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A tested Rust library crate, `haul-engine`, that probes links and downloads them with split-and-steal segmentation, resume, retries, speed limits, a priority queue and a scheduler. A command-line example proves it on real downloads.

**Architecture:** A Cargo workspace. The engine is its own crate (`crates/engine`) with no Tauri types, so plan 3 can add the Tauri app as a second workspace member that depends on it. Pure logic (`segmenter`, `naming`, `schedule`) is unit-tested. Network code is integration-tested against a local `axum` test server that can throttle, drop connections, refuse ranges and change ETags.

**Tech Stack:** Rust (stable, MSVC toolchain), `tokio`, `reqwest` (rustls, HTTP/2), `rusqlite` (bundled SQLite), `tokio-util` (CancellationToken), `thiserror`, `tracing`, `chrono`, `fs4`. Dev only: `axum`, `tempfile`, `sha2`.

**Spec:** `docs/superpowers/specs/2026-11-05-haul-design.md`, sections 3, 4 and 8. Later plans: 2 = Tauri shell + Queue UI, 3 = Collector + clipboard, 4 = post-processing + rules, 5 = local API + extension, 6 = settings, tray, packaging.

**Deviation from the spec, on purpose:** the spec puts the engine inside `app/src-tauri`. This plan puts it in `crates/engine` instead. It's the same "plain Rust, no Tauri types" rule with a stronger boundary, and the engine can be built and tested before any UI exists.

## Global Constraints

- Connections per download: default **8**, allowed **1–32**.
- Initial connection count: **`min(chosen, size / 4 MiB)`**, at least 1.
- Steal: a finished connection takes the **second half of the largest remaining range** if that half is **≥ 1 MiB**.
- Slow connection: below **25%** of the median connection speed for **10 s** → close it and return its range to the pool.
- Per-host cap: **16** connections to one host across all downloads.
- Write buffer: **256 KiB** per connection. `fsync` on pause, exit and completion.
- Save segment progress to SQLite every **2 s** and on pause, exit and error.
- Retry delays: **2, 4, 8, 16, 32 s, then 60 s** steps, up to **10 attempts**, then fail.
- Parallel downloads: default **3**, allowed **1–20**.
- Progress events: at most **4 per second** per download.
- Partial file name: **`<file_name>.<download_id>.haulpart`** in the target folder. The spec says `<name>.haulpart`; the id is added so two same-name downloads can't collide (see Review Focus).
- Never overwrite: final names that already exist become `name (1).ext`, `name (2).ext` and so on.
- HTTP status mapping: `404` → Offline; `403`/`410` → LinkExpired; `416` → restart that file once from zero.
- Resume fingerprint check order: ETag, then Last-Modified, then size. Any mismatch restarts from zero.
- All code is Windows-first. Paths use `std::path`, never string concatenation.

## Review Focus

1. **Server sends no size (no Content-Length, no Content-Range).** Expected: one connection, no pre-allocation, streams until the end, finishes correctly, marked not resumable. Test: `unknown_size_streams_to_eof` in Task 6.
2. **File names that are illegal on Windows**: `<>:"/\|?*`, control characters, trailing dots or spaces, reserved names like `CON` or `com1.txt`, names over 200 characters. Expected: a safe name that keeps its extension. Tests: `sanitize_*` in Task 2.
3. **Server says it supports ranges in the probe, but answers a ranged GET with `200` and the whole file.** Expected: detected, falls back to a single connection from zero, file still correct. Test: `range_ignored_mid_download_falls_back` in Task 7.
4. **Two downloads with the same file name into the same folder at the same time.** Expected: both finish, as `a.bin` and `a (1).bin`, with no corruption. Test: `same_name_concurrent_downloads` in Task 6.
5. **Target folder deleted while a download is paused.** Expected: resume recreates the folder and restarts that file from zero instead of crashing. Test: `resume_with_missing_part_file_restarts` in Task 7.

---

## File structure

```
Cargo.toml                         workspace root
crates/engine/
  Cargo.toml
  src/lib.rs                       pub mod list + re-exports
  src/error.rs                     EngineError
  src/types.rs                     ids, DownloadState, Priority, ProbeInfo, Event
  src/events.rs                    EventBus (broadcast)
  src/segmenter.rs                 pure range planning + steal decision
  src/naming.rs                    sanitize, filename from headers/url, unique path
  src/store/mod.rs                 Store (SQLite access)
  src/store/schema.sql             migration 1
  src/probe.rs                     probe()
  src/ratelimit.rs                 RateLimiter, HostLimiter
  src/transfer/mod.rs              run_transfer(), TransferJob, TransferCtx, TransferConfig
  src/transfer/conn.rs             one connection's loop
  src/transfer/state.rs            shared segment table + speed tracking
  src/queue.rs                     Queue
  src/schedule.rs                  Schedule, is_within_window()
  tests/support/mod.rs             test HTTP server
  tests/probe.rs  tests/transfer.rs  tests/resume.rs  tests/queue.rs
  examples/haul_get.rs             CLI for manual checks
```

---

### Task 0: Install the tools and scaffold the workspace

**Files:**
- Create: `Cargo.toml`, `crates/engine/Cargo.toml`, `crates/engine/src/lib.rs`, `rust-toolchain.toml`

**Interfaces:**
- Produces: a workspace where `cargo test -p haul-engine` runs.

- [ ] **Step 1: Install Rust and the MSVC C++ build tools.** The owner runs these, since they install software:

```bash
winget install --id Rustlang.Rustup -e
```
```bash
winget install --id Microsoft.VisualStudio.2022.BuildTools -e --override "--quiet --wait --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
```

Then open a new terminal. Run: `cargo --version`. Expected: `cargo 1.x.y`.

- [ ] **Step 2: Create the workspace.**
  - Root `Cargo.toml`: `[workspace] members = ["crates/engine"]` and `resolver = "2"`.
  - `rust-toolchain.toml`: `channel = "stable"`.
  - Create `crates/engine` with `cargo new --lib crates/engine --name haul-engine`.
  - Add dependencies with `cargo add` so the newest versions are picked:
    - `tokio --features full`
    - `reqwest --no-default-features --features rustls-tls,stream,http2`
    - `rusqlite --features bundled`
    - `tokio-util`, `futures`, `bytes`, `url`, `percent-encoding`
    - `thiserror`, `tracing`
    - `serde --features derive`, `serde_json`, `chrono`, `fs4`
  - Dev dependencies, with `cargo add --dev`: `axum tempfile sha2 tracing-subscriber`.
- [ ] **Step 3: Verify.** Run: `cargo test -p haul-engine`. Expected: `test result: ok. 0 passed` (the template test may show 1 passed; delete it).
- [ ] **Step 4: Commit.** `git add -A && git commit -m "chore: cargo workspace with haul-engine crate"`

---

### Task 1: Core types, errors, event bus

**Files:** Create `src/error.rs`, `src/types.rs`, `src/events.rs`; modify `src/lib.rs`. Tests sit inline in `types.rs`.

**Interfaces:**
- Produces:
  ```rust
  pub type DownloadId = i64; pub type PackageId = i64;
  #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
  pub enum DownloadState { Queued, Running, Paused, Completed, Failed, LinkExpired, Offline }
  impl DownloadState { pub fn as_str(&self) -> &'static str; pub fn parse(s: &str) -> Option<Self>; }
  #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
  pub enum Priority { High = 0, Normal = 1, Low = 2 }   // Ord: High sorts first
  #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
  pub struct ProbeInfo { pub final_url: String, pub size: Option<u64>, pub resumable: bool,
      pub file_name: String, pub etag: Option<String>, pub last_modified: Option<String>,
      pub content_type: Option<String> }
  #[derive(Clone, Debug, Serialize)]
  pub enum Event {
      Progress { id: DownloadId, written: u64, size: Option<u64>, speed_bps: u64, segments: Vec<(u64, u64, u64)> /* start, pos, end */ },
      StateChanged { id: DownloadId, state: DownloadState, error: Option<String> },
      Steal { id: DownloadId, bytes: u64 },
      AllDone,
  }
  #[derive(thiserror::Error, Debug)]
  pub enum EngineError { #[error("offline (404)")] Offline, #[error("link expired ({0})")] LinkExpired(u16),
      #[error("http status {0}")] Http(u16), #[error("range not satisfiable")] RangeNotSatisfiable,
      #[error("disk full")] DiskFull, #[error("network: {0}")] Network(String), #[error("io: {0}")] Io(#[from] std::io::Error),
      #[error("database: {0}")] Db(#[from] rusqlite::Error), #[error("cancelled")] Cancelled }
  impl EngineError { pub fn is_retryable(&self) -> bool }  // Network and Http(5xx) only
  pub struct EventBus;  impl EventBus { pub fn new() -> Self; pub fn send(&self, e: Event); pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<Event>; }  // Clone, capacity 1024
  ```

- [ ] **Step 1: Write the failing tests.**
  - `state_roundtrip`: every `DownloadState` survives `parse(as_str())`.
  - `priority_orders_high_first`: `[Low, High, Normal]` sorted equals `[High, Normal, Low]`.
  - `retryable_errors`: `Network`, `Http(503)` → true; `Http(404)`, `Offline`, `LinkExpired(403)`, `DiskFull` → false.
- [ ] **Step 2: Run.** `cargo test -p haul-engine types` → FAIL (types not defined).
- [ ] **Step 3: Implement the types above.** `EventBus::send` ignores "no receivers" errors.
- [ ] **Step 4: Run.** Same command → PASS.
- [ ] **Step 5: Commit.** `feat(engine): core types, errors, event bus`

---

### Task 2: File naming

**Files:** Create `src/naming.rs`, with tests inline.

**Interfaces:**
- Produces:
  ```rust
  pub fn sanitize_filename(raw: &str) -> String
  pub fn filename_from(content_disposition: Option<&str>, url: &url::Url) -> String
  pub fn unique_path(dir: &std::path::Path, file_name: &str) -> std::path::PathBuf
  pub fn part_path(dir: &std::path::Path, file_name: &str, id: DownloadId) -> std::path::PathBuf  // "<name>.<id>.haulpart"
  ```

- [ ] **Step 1: Write the failing tests.**
  - `sanitize_replaces_illegal`: `sanitize_filename("a<b>:c\"d/e\\f|g?h*.txt") == "a_b__c_d_e_f_g_h_.txt"`.
  - `sanitize_strips_trailing_dots_spaces`: `"report. . "` → `"report"`.
  - `sanitize_reserved_names`: `"CON"` → `"_CON"`, `"com1.txt"` → `"_com1.txt"`, `"console.txt"` unchanged.
  - `sanitize_long_keeps_extension`: 300 × `"a"` + `".mkv"` → length ≤ 200 and ends with `".mkv"`.
  - `sanitize_empty`: `""` and `"..."` → `"download"`.
  - `filename_rfc5987`: header `attachment; filename*=UTF-8''na%C3%AFve%20file.zip` → `"naïve file.zip"`.
  - `filename_quoted`: `attachment; filename="my file.iso"` → `"my file.iso"`.
  - `filename_from_url_path`: no header, `https://x.org/a/b/ubuntu%2024.iso?x=1` → `"ubuntu 24.iso"`.
  - `filename_fallback`: no header, `https://x.org/` → `"download"`.
  - `unique_path_counts_up`: in a temp dir holding `a.bin` and `a (1).bin`, the result is `a (2).bin`. A name with no extension, `README`, gives `README (1)`.
- [ ] **Step 2: Run.** `cargo test -p haul-engine naming` → FAIL.
- [ ] **Step 3: Implement.**
  - `filename*` wins over `filename`.
  - Percent-decode with `percent-encoding`.
  - Always pass the result through `sanitize_filename`.
  - Reserved stems (case-insensitive): `CON PRN AUX NUL COM1–COM9 LPT1–LPT9`.
- [ ] **Step 4: Run.** → PASS.
- [ ] **Step 5: Commit.** `feat(engine): safe file naming`

---

### Task 3: Segmenter (pure split-and-steal logic)

**Files:** Create `src/segmenter.rs`, with tests inline.

**Interfaces:**
- Produces:
  ```rust
  pub const MIB: u64 = 1024 * 1024;
  pub const MIN_STEAL: u64 = MIB;
  #[derive(Clone, Copy, Debug, PartialEq, Eq)]
  pub struct Range { pub start: u64, pub pos: u64, pub end: u64 }   // [start, end), bytes before pos are written
  impl Range { pub fn remaining(&self) -> u64; pub fn is_done(&self) -> bool; }
  pub fn initial_plan(size: u64, max_conns: u32) -> Vec<Range>
  /// Choose a range to split for an idle connection. Returns (victim index, split point).
  pub fn pick_steal(ranges: &[Range], min_steal: u64) -> Option<(usize, u64)>
  pub fn is_slow(speed_bps: f64, median_bps: f64, ratio: f64) -> bool
  ```

- [ ] **Step 1: Write the failing tests.**
  - `plan_100mib_8`: `initial_plan(100*MIB, 8)` → 8 ranges, each `12.5*MIB`. The first is `{0,0,13107200}` and the last ends at `104857600`.
  - `plan_small_file`: `initial_plan(10*MIB, 8)` → 2 ranges, because 10 MiB / 4 MiB = 2.
  - `plan_tiny_file`: `initial_plan(1000, 8)` → `[Range{0,0,1000}]`.
  - `plan_remainder_to_last`: `initial_plan(8*MIB + 3, 2)` → `[0..4MiB)`, `[4MiB..8MiB+3)`.
  - `steal_picks_largest`: remaining 3 MiB, 9 MiB and 5 MiB → `Some((1, pos + 4.5 MiB))`.
  - `steal_tie_picks_lowest_index`: two ranges each with 4 MiB remaining → index 0.
  - `steal_min_size`: largest remaining is `2*MIB - 1` → `None`. Exactly `2*MIB` → `Some` (half = 1 MiB).
  - `steal_ignores_done`: every range done → `None`.
  - `slow_detection`: `is_slow(0.24e6, 1e6, 0.25) == true`, `is_slow(0.25e6, 1e6, 0.25) == false`.
- [ ] **Step 2: Run.** `cargo test -p haul-engine segmenter` → FAIL.
- [ ] **Step 3: Implement.**
  - **Connection count:** `n = clamp(size / (4*MIB), 1, max_conns)`, with an equal split.
  - **Split point:** `pos + remaining/2`. The caller sets `victim.end = split` and pushes `Range{split, split, old_end}`.
- [ ] **Step 4: Run.** → PASS.
- [ ] **Step 5: Commit.** `feat(engine): segment planning and steal decisions`

---

### Task 4: SQLite store

**Files:** Create `src/store/mod.rs` and `src/store/schema.sql`, with tests inline using `tempfile`.

**Interfaces:**
- Consumes: Task 1 types, `segmenter::Range` (Task 3).
- Produces:
  ```rust
  #[derive(Clone)] pub struct Store { /* Arc<Mutex<rusqlite::Connection>> */ }
  pub struct NewDownload { pub package_id: Option<PackageId>, pub url: String, pub headers_json: Option<String>,
      pub target_dir: PathBuf, pub file_name: Option<String>, pub priority: Priority, pub connections: u32,
      pub speed_limit: Option<u64> }
  #[derive(Clone, Debug)] pub struct Download { pub id: DownloadId, pub package_id: Option<PackageId>, pub url: String,
      pub headers_json: Option<String>, pub probe: Option<ProbeInfo>, pub state: DownloadState, pub priority: Priority,
      pub position: i64, pub connections: u32, pub speed_limit: Option<u64>, pub target_dir: PathBuf,
      pub file_name: Option<String>, pub target_path: Option<PathBuf>, pub error: Option<String> }
  impl Store {
      pub fn open(path: &Path) -> Result<Store, EngineError>;  pub fn open_in_memory() -> Result<Store, EngineError>;
      pub fn create_package(&self, name: &str, source: &str) -> Result<PackageId, EngineError>;
      pub fn insert_download(&self, d: &NewDownload) -> Result<DownloadId, EngineError>;  // position = max + 1
      pub fn get_download(&self, id: DownloadId) -> Result<Download, EngineError>;
      pub fn list_downloads(&self) -> Result<Vec<Download>, EngineError>;  // ordered by priority, position, id
      pub fn set_probe(&self, id: DownloadId, p: &ProbeInfo) -> Result<(), EngineError>;
      pub fn set_state(&self, id: DownloadId, s: DownloadState, error: Option<&str>) -> Result<(), EngineError>;
      pub fn set_completed(&self, id: DownloadId, path: &Path) -> Result<(), EngineError>;
      pub fn set_order(&self, id: DownloadId, p: Priority, position: i64) -> Result<(), EngineError>;
      pub fn delete_download(&self, id: DownloadId) -> Result<(), EngineError>;  // removes its segments too
      pub fn save_segments(&self, id: DownloadId, r: &[Range]) -> Result<(), EngineError>;  // replace all, one transaction
      pub fn load_segments(&self, id: DownloadId) -> Result<Vec<Range>, EngineError>;
      pub fn get_setting(&self, key: &str) -> Result<Option<serde_json::Value>, EngineError>;
      pub fn set_setting(&self, key: &str, v: &serde_json::Value) -> Result<(), EngineError>;
  }
  ```
- Schema: the plan-1 subset of spec section 8, namely `packages`, `downloads` (with probe columns `final_url size etag last_modified resumable file_name content_type`, plus `headers_json`), `segments(download_id, start, pos, end)` and `settings`. Version is tracked with `PRAGMA user_version = 1`. WAL mode and `foreign_keys = ON`.

- [ ] **Step 1: Write the failing tests.**
  - `insert_and_get_roundtrip`: all fields come back equal.
  - `positions_increment`: three inserts give positions 1, 2, 3.
  - `list_orders_by_priority_then_position`: rows inserted as (Normal, Low, High, Normal) list as High, Normal(1), Normal(4), Low.
  - `segments_replace`: saving 3 then 2 ranges loads 2.
  - `state_persists_across_reopen`: open a file DB, set Paused with error `"x"`, drop it, reopen → Paused and `"x"`.
  - `schema_version_is_1`.
  - `delete_cascades_segments`.
- [ ] **Step 2: Run.** `cargo test -p haul-engine store` → FAIL.
- [ ] **Step 3: Implement.** Use `include_str!("schema.sql")`. Methods are synchronous; each takes the mutex for one statement or transaction.
- [ ] **Step 4: Run.** → PASS.
- [ ] **Step 5: Commit.** `feat(engine): sqlite store with migrations`

---

### Task 5: Test server and probe

**Files:** Create `tests/support/mod.rs`, `src/probe.rs` and `tests/probe.rs`.

**Interfaces:**
- Produces, test support only:
  ```rust
  pub fn body(len: usize) -> bytes::Bytes          // byte i = ((i * 31 + 7) % 251) as u8
  #[derive(Clone, Default)] pub struct Spec { pub len: usize, pub ranges: bool, pub head_allowed: bool /* default true via new() */,
      pub etag: Option<String>, pub last_modified: Option<String>, pub content_disposition: Option<String>,
      pub status: Option<u16>,              // force this status on every request
      pub bytes_per_sec: Option<u64>,       // per response
      pub slow_from: Option<(u64, u64)>,    // (offset, bps): ranges starting at >= offset use this rate
      pub drop_after: Option<u64>,          // first response for each range start aborts after N bytes
      pub ignore_range_after_requests: Option<u32>,  // after N requests, answer ranged GETs with 200 + full body
      pub no_length: bool,                  // omit Content-Length, stream with chunked encoding
      pub redirect_from: Option<String> }   // this path 302-redirects to /file
  impl Spec { pub fn new(len: usize) -> Self }  // ranges: true, head_allowed: true
  pub struct TestServer { pub url: String /* .../file */, pub base: String,
      pub requests: Arc<AtomicU32>, pub bytes_sent: Arc<AtomicU64>, pub max_concurrent: Arc<AtomicU32>,
      pub spec: Arc<Mutex<Spec>> /* tests may change it mid-run, e.g. the etag */ }
  pub async fn serve(spec: Spec) -> TestServer       // binds 127.0.0.1:0
  ```
- Produces, engine:
  ```rust
  pub fn http_client() -> reqwest::Client   // rustls, redirect limit 10, connect timeout 15 s, read timeout 30 s, UA "Haul/<version>"
  pub async fn probe(client: &reqwest::Client, url: &str, headers: &reqwest::header::HeaderMap) -> Result<ProbeInfo, EngineError>
  ```

- [ ] **Step 1: Write the failing tests** in `tests/probe.rs`.
  - `probe_range_server`: `Spec::new(5_000_000)` with etag `"abc"` → `size == Some(5_000_000)`, `resumable`, `etag == Some("\"abc\"")`, `file_name == "file"`.
  - `probe_no_ranges`: `ranges: false` → `resumable == false`, size known.
  - `probe_head_refused`: `head_allowed: false` (the server answers HEAD with 405) → falls back to `GET` with `Range: bytes=0-0`; still resumable and the size is correct.
  - `probe_redirect`: request `/old` (the server answers 302 → `/file`) → `final_url` ends with `/file`.
  - `probe_404_offline` → `Err(EngineError::Offline)`. `probe_403_expired` → `Err(LinkExpired(403))`. `probe_410_expired` → `LinkExpired(410)`.
  - `probe_content_disposition`: `attachment; filename="x y.iso"` → `"x y.iso"`.
  - `probe_unknown_size`: `no_length: true, ranges: false` → `size == None`, `resumable == false`.
- [ ] **Step 2: Run.** `cargo test -p haul-engine --test probe` → FAIL.
- [ ] **Step 3: Implement the server and `probe`.**
  - **Server:** an `axum` router on `/file` (and `redirect_from`). It parses `Range: bytes=a-b`, answers `206` with `Content-Range: bytes a-b/len`, and throttles by sleeping between 16 KiB chunks. It counts `requests`, `bytes_sent` and in-flight connections (keeping the maximum in `max_concurrent`).
  - **Probe:**
    - Send `HEAD` first. On HEAD failure, or HEAD without a length, send `GET` with `Range: bytes=0-0`.
    - `206` with `Content-Range` → size and resumable. `200` → `Content-Length` and not resumable.
    - Drop the GET body after the headers.
    - Map status codes as Global Constraints says, and take the name from `naming::filename_from`.
- [ ] **Step 4: Run.** → PASS.
- [ ] **Step 5: Commit.** `feat(engine): link probe + test http server`

---

### Task 6: Transfer, the basics: connections, writing, completion, speed limit

**Files:**
- Create `src/ratelimit.rs`, `src/transfer/mod.rs`, `src/transfer/conn.rs`, `src/transfer/state.rs`, `tests/transfer.rs`.
- Rate-limiter tests go inline in `ratelimit.rs`.

**Interfaces:**
- Consumes: `probe`, `segmenter::{initial_plan, pick_steal, Range}`, `naming::{part_path, unique_path}`, `Store`, `EventBus`.
- Produces:
  ```rust
  pub struct RateLimiter;  impl RateLimiter { pub fn new(bps: Option<u64>) -> Self; pub fn set_rate(&self, bps: Option<u64>);
      pub async fn acquire(&self, bytes: u64); }       // token bucket, burst = bps / 4
  pub struct HostLimiter;  impl HostLimiter { pub fn new(per_host: usize) -> Self;
      pub async fn acquire(&self, host: &str) -> tokio::sync::OwnedSemaphorePermit; }
  #[derive(Clone)] pub struct TransferConfig { pub min_steal: u64, pub slow_ratio: f64, pub slow_window: Duration,
      pub save_interval: Duration, pub retry_delays: Vec<Duration>, pub max_attempts: u32, pub write_buffer: usize,
      pub progress_interval: Duration,
      pub free_space_override: Option<u64> /* tests only: pretend this many bytes are free */ }
  impl TransferConfig { pub fn default() -> Self /* spec values */; pub fn for_tests() -> Self /* delays 10 ms, slow_window 300 ms, save 50 ms */ }
  #[derive(Clone)] pub struct TransferCtx { pub client: reqwest::Client, pub store: Store, pub events: EventBus,
      pub global_limit: Arc<RateLimiter>, pub hosts: Arc<HostLimiter>, pub config: TransferConfig }
  pub struct TransferJob { pub id: DownloadId, pub cancel: tokio_util::sync::CancellationToken }
  #[derive(Debug)] pub enum TransferOutcome { Completed(PathBuf), Paused, Failed(EngineError) }
  pub async fn run_transfer(job: TransferJob, ctx: &TransferCtx) -> TransferOutcome
  ```
  `run_transfer` reads everything it needs (URL, headers, probe, target folder, connections, speed limit) from the store row `job.id`. It probes when the row has no probe yet. It sets state `Running` at start and the final state at the end.

- [ ] **Step 1: Write the failing tests** in `tests/transfer.rs`. Each test compares the final file's SHA-256 to `body(len)` (helper `assert_file_eq(path, len)`).
  - `single_small_file`: 100 KB → `Completed`; the final name is `file`; no `.haulpart` remains.
  - `multi_connection_used`: 32 MiB, 8 connections, server at 4 MB/s per response → `max_concurrent >= 4` and the file is correct.
  - `no_range_server_single_connection`: `ranges: false`, 3 MB → `max_concurrent == 1` and the file is correct.
  - `unknown_size_streams_to_eof`: `no_length: true, ranges: false`, 2 MB → the file is correct (Review Focus 1).
  - `name_conflict_never_overwrites`: the target folder already has `file`, containing `"keep"` → the new file is `file (1)`, and `file` still contains `"keep"`.
  - `same_name_concurrent_downloads`: two downloads of the same server URL into one folder run at the same time → `file` and `file (1)` both correct (Review Focus 2's sibling, Review Focus 4).
  - `speed_limit_respected`: 2 MB with a per-download limit of 1 MB/s → elapsed between 1.6 s and 3.0 s.
  - `per_host_cap`: `HostLimiter::new(2)` in the ctx, 32 MiB with 8 connections → `max_concurrent <= 2` and the file is correct.
  - `disk_full_refuses_start`: `free_space_override: Some(1000)` on a 5 MB file → `Failed(DiskFull)` and no part file is created.
  - `progress_events_throttled`: count `Progress` events for one download over a 1-second window → at most 5.
  - `ratelimit_unit` (inline, `#[tokio::test(start_paused = true)]`): at 1,000,000 B/s, acquiring 3,000,000 in 64 KiB chunks advances virtual time by 2.7–3.3 s; `None` returns at once.
- [ ] **Step 2: Run.** `cargo test -p haul-engine --test transfer` → FAIL.
- [ ] **Step 3: Implement.**
  - **`state.rs`:** `Arc<Mutex<SegTable>>` holding `Vec<Range>` plus per-connection byte counters for speed.
  - **`conn.rs`:** one async loop per connection:
    1. take a range;
    2. wait for the host permit;
    3. `GET` with `Range: bytes=pos-(end-1)`;
    4. stream the chunks, calling `global_limit.acquire`, then the per-download limiter, then writing to its own `tokio::fs::File` (`seek` + `BufWriter` of `write_buffer` bytes);
    5. update `pos` and stop at the current `end`, which a steal may have lowered.
  - **Finishing a range:** the connection locks the table, runs `pick_steal`, splits, and emits `Event::Steal`. If nothing is worth stealing, it exits.
  - **`mod.rs`:**
    - Ensure the target folder exists, check free space with `fs4::available_space` against `size - written` (`DiskFull` if short), and create or open the part file with `set_len(size)` when the size is known.
    - Spawn the connections.
    - A ticker saves segments every `save_interval` and emits progress every `progress_interval`.
    - On completion: flush, `sync_all`, rename to `unique_path`, then `store.set_completed`.
    - A range-less or size-less download uses one connection that writes sequentially.
- [ ] **Step 4: Run.** → PASS.
- [ ] **Step 5: Commit.** `feat(engine): segmented transfer with speed limits`

---

### Task 7: Transfer, robustness: steal, slow connections, retries, pause/resume, fingerprints

**Files:** Modify `src/transfer/*.rs`. Create `tests/resume.rs` and add tests to `tests/transfer.rs`.

**Interfaces:**
- Consumes and produces: the same API as Task 6. No new public items.

- [ ] **Step 1: Write the failing tests.**
  - `steal_happens`: 24 MiB, 4 connections, with `slow_from: Some((18*MIB, 200_000))` so the last range crawls → at least one `Event::Steal` and the file is correct.
  - `slow_connection_replaced`: the same slow setup, with `for_tests()` slow window → the server sees more than 4 requests (the slow one was reopened) and the file is correct.
  - `dropped_connection_retried`: `drop_after: Some(300_000)` on a 6 MiB file → the file is correct and `requests > connections`.
  - `retries_exhausted_fails`: `status: Some(503)` → `Failed(Http(503))` after `max_attempts` requests to `/file`.
  - `expired_mid_download`: start a 20 MiB throttled download, switch `spec.status` to 403 once 5 MiB are done → `Failed(LinkExpired(403))`; DB state is `LinkExpired`; segments are saved with progress > 0.
  - `range_ignored_mid_download_falls_back`: `ignore_range_after_requests: Some(2)`, 8 connections → the file is still correct (Review Focus 3).
  - **In `tests/resume.rs`:**
    - `pause_then_resume_completes`: cancel at about 40% → `Paused`, the part file exists, segments are saved. A new `run_transfer` on the same id → `Completed`, and the server's `bytes_sent < 1.5 × len`, which shows it resumed rather than restarted.
    - `etag_change_restarts`: pause at about 40%, change the server's etag, resume → completes correctly, with `bytes_sent >= 1.4 × len`, which shows it restarted.
    - `resume_with_missing_part_file_restarts`: pause, delete the whole target folder, resume → the folder is recreated and the file completes correctly (Review Focus 5).
    - `crash_simulation`: drop the runtime mid-download without a pause, reopen the `Store` from the file path, resume → correct file. This proves the 2-second saves are enough.
- [ ] **Step 2: Run.** `cargo test -p haul-engine --test transfer --test resume` → the new tests FAIL.
- [ ] **Step 3: Implement.**
  - **Per-connection retry:** errors where `is_retryable()` is true wait `retry_delays[min(attempt, len-1)]` and retry the same `pos`. More than `max_attempts` fails the transfer.
  - **Expired link (403/410):** cancel all connections, save, and return `Failed(LinkExpired)`.
  - **Server ignored the range** (`200` when a range starting above 0 was asked for): cancel everything, truncate the file, and restart with one sequential connection.
  - **Slow connections:** a monitor in the ticker computes each connection's speed over `slow_window`. A connection that `is_slow` against the median is cancelled, its range stays in the table, and a fresh connection takes it.
  - **Cancel:** stop the connections, flush, `sync_all`, save the segments, state `Paused`.
  - **Resume:**
    - Load the segments. If the part file is missing, or the fresh probe's ETag, Last-Modified or size differs from the stored probe (checked in that order), log the reason, delete the part file and plan from zero.
    - Otherwise spawn connections for the unfinished ranges, up to `connections`.
  - **`416`:** restart from zero once, and fail with `RangeNotSatisfiable` the second time.
- [ ] **Step 4: Run.** `cargo test -p haul-engine` → all PASS. Run it 3 times in a row to catch flaky timing; every run must pass.
- [ ] **Step 5: Commit.** `feat(engine): steal, retries, pause/resume with fingerprint check`

---

### Task 8: Queue

**Files:** Create `src/queue.rs` and `tests/queue.rs`.

**Interfaces:**
- Consumes: `run_transfer`, `TransferCtx`, `Store`, `EventBus`.
- Produces:
  ```rust
  #[derive(Clone)] pub struct Queue;
  impl Queue {
      pub fn start(ctx: TransferCtx, max_parallel: usize) -> Queue;   // spawns the dispatcher; resets Running rows to Queued on start
      pub fn add(&self, d: NewDownload) -> Result<DownloadId, EngineError>;   // state Queued, wakes the dispatcher
      pub fn pause(&self, id: DownloadId);  pub fn resume(&self, id: DownloadId);   // resume = back to Queued
      pub fn remove(&self, id: DownloadId);   // pauses if running, deletes the row and part file; never touches finished files
      pub fn set_priority(&self, id: DownloadId, p: Priority);  pub fn move_to(&self, id: DownloadId, position: i64);
      pub fn set_max_parallel(&self, n: usize);   // clamped 1..=20
      pub fn max_parallel(&self) -> usize;
      pub fn set_global_limit(&self, bps: Option<u64>);
      pub fn set_schedule(&self, s: Option<Schedule>);   // from Task 9
      pub async fn shutdown(&self);   // pauses everything and saves; used on app exit
  }
  ```
  - **Dispatcher:** whenever something changes, and every 1 s, it starts the `Queued` rows in `list_downloads()` order while `running < max_parallel`.
  - **Outcomes:** `Completed` → `Completed`; `Paused` → `Paused` (unless it was a user `remove`); `Failed(Offline)` → `Offline`; `LinkExpired` → `LinkExpired`; `DiskFull` → `Paused` with error `"Disk full"`; anything else → `Failed` with the message.
  - When nothing is `Queued` or `Running` and at least one download ran since the last `AllDone`, it emits `Event::AllDone` once.

- [ ] **Step 1: Write the failing tests.**
  - `respects_max_parallel`: 5 throttled downloads with max 2 → watching `StateChanged`, never more than 2 `Running` at once; all 5 end `Completed`.
  - `priority_goes_first`: max 1; add A (Normal), B (Normal), C (High) while A runs → C starts before B.
  - `pause_frees_slot`: max 1; A runs, B waits; `pause(A)` → B starts.
  - `restart_resumes_interrupted`: start a queue on a file DB, let A reach about 30%, drop the queue without `shutdown` (a simulated crash), then `Queue::start` on the same DB → A finishes correctly.
  - `remove_running_deletes_part`: `remove` while running → the row is gone and no `.haulpart` is left in the folder.
  - `disk_full_pauses`: `free_space_override: Some(1000)` → the download ends `Paused` with error `"Disk full"`.
  - `all_done_emitted_once`: two downloads finish → exactly one `AllDone`.
  - `set_max_parallel_clamps`: 0 → 1, 50 → 20 (read back through a `max_parallel()` getter).
- [ ] **Step 2: Run.** `cargo test -p haul-engine --test queue` → FAIL.
- [ ] **Step 3: Implement.** Keep a `CancellationToken` for each running id in a `HashMap` behind a `Mutex`, and use a `tokio::sync::Notify` to wake the dispatcher.
- [ ] **Step 4: Run.** → PASS.
- [ ] **Step 5: Commit.** `feat(engine): download queue with priorities and parallel limit`

---

### Task 9: Scheduler

**Files:** Create `src/schedule.rs` with tests inline; modify `src/queue.rs` and `tests/queue.rs`.

**Interfaces:**
- Produces:
  ```rust
  #[derive(Clone, Debug, Serialize, Deserialize)]
  pub struct Schedule { pub start: chrono::NaiveTime, pub stop: chrono::NaiveTime, pub weekdays: Vec<chrono::Weekday> }
  pub fn is_within_window(now: chrono::NaiveDateTime, s: &Schedule) -> bool
  #[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)] pub enum WhenDone { Nothing, Sleep, Shutdown }
  ```
  - The engine only stores `WhenDone` (setting key `"when_done"`) and emits `AllDone`. Sleep and shutdown are carried out by the app in plan 2.
  - The queue checks the schedule each dispatcher tick. Outside the window it starts nothing and pauses running downloads, marking them for auto-resume. Inside the window, auto-paused downloads go back to `Queued`.

- [ ] **Step 1: Write the failing tests.**
  - `inside_day_window`: Mon 09:00–17:00, Mon 12:00 → true; Mon 18:00 → false; Tue 12:00 with only Mon chosen → false.
  - `overnight_window`: 22:00–06:00 on Fri. Fri 23:00 → true. Sat 05:00 → true, because the window started on Friday. Sat 23:00 → false.
  - `start_equals_stop_means_all_day`: 00:00–00:00 on Mon, Mon 15:00 → true.
  - In `tests/queue.rs`, `schedule_blocks_start`: a schedule whose window excludes now → an added download stays `Queued` for 2 s; clearing the schedule starts it.
- [ ] **Step 2: Run.** `cargo test -p haul-engine schedule` and `--test queue` → FAIL.
- [ ] **Step 3: Implement.** For an overnight window (`start > stop`), the weekday that counts is the day the window **started**.
- [ ] **Step 4: Run.** `cargo test -p haul-engine` → all PASS.
- [ ] **Step 5: Commit.** `feat(engine): download scheduler`

---

### Task 10: CLI example and a real-world check

**Files:** Create `examples/haul_get.rs`.

**Interfaces:**
- Consumes: `Store`, `Queue`, `TransferCtx`, `http_client`, `EventBus`.
- Produces: `cargo run -p haul-engine --release --example haul_get -- <url> [--conns N] [--out DIR] [--limit BYTES_PER_SEC]`. It prints one progress line per second (`12.4% 41.2 MB/s 8 conn`) and the final path. Ctrl+C pauses cleanly. Running the same command again resumes, using the state DB at `<out>/.haul-cli.db`.

- [ ] **Step 1: Implement the example.** Use `tokio::signal::ctrl_c` to call `queue.shutdown()`.
- [ ] **Step 2: Real download.** Run: `cargo run -p haul-engine --release --example haul_get -- https://proof.ovh.net/files/1Gb.dat --out _tmp/dl`. Expected: completes; the speed is close to the line speed; file size `134217728` (the "1Gb" file is 1 gigabit = 128 MiB).
- [ ] **Step 3: Kill and resume.** Run it again with a new `--out`, press Ctrl+C at about 50%, run the same command again → it continues from about 50% (the first progress line shows ≥ 45%) and completes.
- [ ] **Step 4: Compare speed with a single connection.** `--conns 1` on the same file → note both speeds in the commit message. Expected: 8 connections ≥ 1 connection.
- [ ] **Step 5: Full test suite.** `cargo test -p haul-engine` → all PASS; `cargo clippy -p haul-engine -- -D warnings` → no warnings.
- [ ] **Step 6: Commit.** `feat(engine): haul_get CLI example` (include the measured speeds in the body).

---

## Done when

- `cargo test -p haul-engine` passes 3 runs in a row.
- `cargo clippy -p haul-engine -- -D warnings` is clean.
- Task 10's real download, kill/resume and speed comparison are done, with their results recorded in the commit.
- Spec success criteria 1 (speed ≥ a single connection, recovery from drops) and 2 (kill and restart resumes with no corruption) are shown by tests and Task 10.
