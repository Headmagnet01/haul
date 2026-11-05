# Haul Plan 3: Collector + Clipboard Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Links from the clipboard, a paste or a drop land in a Collector view first. There they are checked online (8 probes at a time), grouped into packages (multi-part archives are recognised), flagged as duplicates and filtered. The user starts a package, or every online link, and it moves to the Queue.

**Architecture:**
- **Engine `grabber` module** (`crates/engine/src/grabber/`):
  - `grouping.rs` (pure): archive-part recognition and package naming.
  - `dupes.rs` (pure): duplicate detection.
  - `mod.rs`: `Grabber`, which owns the probe pool, the intake, the collector state (computed fresh from the store on every read), start, remove and recheck.
  - The probe function is injected, so tests run without a network.
- **Store:** migration 2 adds three collector tables. Collector packages are *virtual*, derived on every read from the batch and the grouping of the current names. A real `packages` row is created only when a package is started, and `collector_groups` remembers it, so a package started in two goes lands in one Queue package.
- **App crate:** `AppCore` gains collector commands. `clipboard.rs` adds a pure `ClipboardFilter` plus a watcher thread (`clipboard-master` + `arboard`). The bridge's `haul:update` gains `collector: true` whenever the collector changed, and the UI then reloads the collector snapshot.
- **UI:** pure logic in `src/lib/collector.ts` (filters, list building, summaries), state in `src/lib/collector-state.svelte.ts`, and views in `CollectorView.svelte` / `CollectorRow.svelte` / `FilterChips.svelte`. Paste and drop move from `QueueView` to `App.svelte`, so they work in every view.

**Tech Stack:** as in plan 2, plus `clipboard-master` 4 and `arboard` 3 (app crate), and `futures` (already an engine dependency) for `BoxFuture`.

**Spec:** `docs/superpowers/specs/2026-11-05-haul-design.md`: section 5 "Intake", "Grouping into packages", "Duplicates and filters"; section 3 "Link lifecycle" steps 1–4; section 7 (status-bar clipboard toggle, Ctrl+V/drop anywhere); milestone 4. Visual reference: the Collector view in `docs/mockup/index.html` (lines 133–150 CSS, 200–228 markup) and `DESIGN.md`.

**Not in this plan (on purpose):**
- **Browser intake** (`source` = browser, page title as package name, "Auto-start browser links" toggle in the Collector footer, auto-start of browser links): plan 5. `Grabber::intake` takes a `source` string so plan 5 only adds a caller.
- **Settings UI** for "pasted links start right away": plan 6. Pasted and clipboard links always wait in the Collector.
- **Post-processing, categories, `auto_extract`:** plan 4.
- **Cookies/headers on probes:** plan 5. Probes send no extra headers.
- **First-run clipboard explainer:** plan 6. The status-bar toggle has a `title` tooltip instead.

## Global Constraints

- Everything in plan 2's Global Constraints still holds (layout, fonts, tokens, units, motion, "status is icon plus word", never destroy data).
- **Intake rule:** clipboard, paste, drop and the Queue's "Add links" box all go to the Collector. Nothing goes straight to the Queue any more. (This replaces plan 2's ruling "Pasted links queue directly".)
- **Sources:** `"clipboard"`, `"paste"`, `"drop"`. Shown as `Clipboard`, `Paste`, `Drop`; an unknown source is shown as is.
- **Probe pool:** at most **8** probes at once across the whole Collector, each with a **20 s** timeout, one attempt, no extra headers.
- **Link statuses** (`LinkStatus::as_str`): `checking`, `online`, `offline`, `expired`, `error`. Words: `Checking…`, `Online`, `Offline (404)`, `Link expired (<code>)`, `Couldn't check`. The word for `error` has `title` = the stored message.
- **Grouping patterns** (case-insensitive, base must be non-empty): `name.partN.rar`, `name.rar` + `name.rNN` (2+ digits), `name.7z.NNN` and `name.zip.NNN` (3+ digits), `name.zip` + `name.zNN` (2+ digits). `name.rar` or `name.zip` with no sibling volume is not a set.
- **Package naming:** archive set → its base name. The rest of a batch → the common file-name prefix (see Task 2), else the batch label **`Pasted list · <d Mon HH:MM>`** (local time, as in plan 2). A batch's leftover group of exactly **one** link is a lone row with no package.
- **Duplicates:** same URL (original or final, either side) or same file name (case-insensitive) **and** same known size. Kinds and badges: `downloaded` → `Already downloaded`, `queued` → `Already queued`, `collector` → `Also in Collector` (on the later link). Flagged, never dropped, except an **exact URL already waiting in the Collector**, which intake skips.
- **Filters** (settings key `collector_filters`, JSON `{"html":bool,"images":bool,"small":bool,"duplicates":bool,"exts":[string]}`). Default `{"html":true,"images":true,"small":false,"duplicates":false,"exts":[]}`, as in the mockup.
  - html: content type `text/html`, or extension `html htm php asp aspx jsp shtml`.
  - images: content type `image/*`, or extension `jpg jpeg png gif webp bmp svg ico avif tif tiff heic`.
  - small: known size `< 1_000_000`.
  - duplicates: `duplicate != null`.
  - exts: lowercase, no leading dot.
- **Watch clipboard:** settings key `watch_clipboard` (bool), **default off**. Clipboard text over **1,000,000 bytes** is ignored.
- **Start rules:**
  - "Start package" and a lone row's "Start" start the **visible** links that are not `offline` or `expired`.
  - "Start all online" starts visible `online` links only.
  - Started links leave the Collector.
  - Archive parts are queued in part order.
- **Collector layout:** grid `minmax(260px,2fr) 92px 130px 110px 140px`, gap 14px, 20px side padding. Column header 32px; package rows 46px on `--bg`; child rows 40px with a 26px name indent. Chips as in the mockup CSS. The Online column is 130px, not the mockup's 110px, so `Link expired (403)` fits.
- **Copy (exact):**
  - Title `Collector`.
  - Sub line `Links waiting for review · <n> link(s) in <m> package(s)`, plus ` · <h> hidden` when h > 0, or `Nothing waiting` when empty.
  - Buttons `Clear offline`, `Start all online`, `Start package`, `Start`.
  - Chip row label `Hide:` with chips `.html pages`, `images`, `under 1 MB`, `duplicates`, user chips `.<ext>`, and an add button `+ Type`.
  - Columns `Name`, `Size`, `Online`, `Source`.
  - Package tags `<7z|rar|zip> · <n> part(s)` or `<n> file(s)`.
  - Footer `Copied and pasted links wait here for review. Start a package to download it.`
  - Empty state `Nothing waiting. Turn on Watch clipboard and copy links, or paste them anywhere in Haul.`
  - Status-bar toggle `Watch clipboard`, title `Copied links appear in the Collector`.
- **Toasts (exact):**
  - `Sent <n> link(s) to the Collector`, with action `Show`, which opens the Collector.
  - `Already in the Collector`.
  - `No links found`.
  - `Started <n> download(s)`.
  - `Removed <name>` or `Removed <n> links`, with `Undo` (6 s, as in the Queue).
  - `Cleared <n> offline link(s)`, with `Undo`.
- **Rail badge:** Collector shows the total link count (hidden ones included) when it is above 0.

## Review Focus

1. **The app closes while links are still Checking.** Expected: on the next launch they are probed again, not stuck at "Checking…" for ever. Test: `checking_links_reprobed_on_start` (Task 3).
2. **The same text arrives twice:** the clipboard watcher takes a copy, then the user also presses Ctrl+V in Haul, or pastes twice. Expected: no duplicate rows, and the toast says "Already in the Collector". Tests: `intake_skips_urls_already_waiting` (Task 3) and e2e `paste_twice_says_already` (Task 7).
3. **A link is removed or started while its probe is in flight.** Expected: the late probe result changes nothing; no row comes back and nothing panics. Test: `probe_finishing_after_remove_is_ignored` (Task 3).
4. **A huge paste or clipboard** (500 links, or a multi-MB log copied by accident). Expected: no more than 8 probes at once, the UI stays responsive, and clipboard text over 1 MB is ignored. Tests: `probes_at_most_8_at_once` (Task 3) and `filter_rejects_huge_text` (Task 5).
5. **Names that look like archive parts but aren't** (`report.part1.pdf`, `set.r1`, `data.7z.01`, `movie.part1.rar.html`), and mixed case within one set (`X.PART1.RAR` + `x.part2.rar`). Expected: the look-alikes are not grouped, and the mixed-case set is one package. Tests: the `archive_part_table` and `case_insensitive_base` (Task 2).

---

### Task 1: Store migration 2: collector tables

**Files:**
- Create: `crates/engine/src/store/migration_2.sql`
- Create: `crates/engine/src/store/collector.rs` (an `impl Store` block for the collector methods; `mod collector;` in `store/mod.rs`)
- Modify: `crates/engine/src/store/mod.rs` (`SCHEMA_VERSION = 2`; `init` runs migration 1 if `user_version < 1`, then migration 2 if `< 2`, each in its own transaction)
- Modify: `crates/engine/src/types.rs` (`LinkStatus`)
- Test: unit tests in `store/collector.rs`; rename `schema_version_is_1` → `schema_version_is_2`

**Interfaces:**
- Produces:
  - `pub enum LinkStatus { Checking, Online, Offline, Expired, Error }` with `as_str() -> &'static str` and `parse(&str) -> Option<LinkStatus>`, and serde `rename_all = "snake_case"` (in `types.rs`, exported from `lib.rs`).
  - `pub struct Batch { pub id: i64, pub source: String, pub label: String, pub created_at: String }`
  - `pub struct CollectorLink { pub id: i64, pub batch_id: i64, pub url: String, pub status: LinkStatus, pub probe: Option<ProbeInfo>, pub message: Option<String>, pub created_at: String }`
  - `Store::create_batch(&self, source: &str, label: &str) -> Result<i64, EngineError>`
  - `Store::insert_link(&self, batch_id: i64, url: &str) -> Result<i64, EngineError>` (status `checking`)
  - `Store::list_batches(&self) -> Result<Vec<Batch>, EngineError>` (by id)
  - `Store::list_links(&self) -> Result<Vec<CollectorLink>, EngineError>` (by id)
  - `Store::set_link_probe(&self, id: i64, status: LinkStatus, probe: Option<&ProbeInfo>, message: Option<&str>) -> Result<(), EngineError>`: a missing id is a silent no-op.
  - `Store::delete_links(&self, ids: &[i64]) -> Result<(), EngineError>`: one transaction, which also deletes batches left with no links.
  - `Store::group_package(&self, batch_id: i64, group_key: &str) -> Result<Option<PackageId>, EngineError>` and `Store::set_group_package(&self, batch_id: i64, group_key: &str, package_id: PackageId) -> Result<(), EngineError>`

Migration 2 (`migration_2.sql`). `probe_json` is `serde_json` of `ProbeInfo`; this replaces the spec's `probe_result`, and `duplicate_of` is computed on read instead of stored (see the rulings in Task 3):

```sql
CREATE TABLE collector_batches (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    source      TEXT NOT NULL,
    label       TEXT NOT NULL,
    created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);
CREATE TABLE collector_links (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    batch_id    INTEGER NOT NULL REFERENCES collector_batches(id) ON DELETE CASCADE,
    url         TEXT NOT NULL,
    status      TEXT NOT NULL DEFAULT 'checking',
    probe_json  TEXT,
    message     TEXT,
    created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);
CREATE TABLE collector_groups (
    batch_id    INTEGER NOT NULL REFERENCES collector_batches(id) ON DELETE CASCADE,
    group_key   TEXT NOT NULL,
    package_id  INTEGER NOT NULL REFERENCES packages(id) ON DELETE CASCADE,
    PRIMARY KEY (batch_id, group_key)
);
```

- [ ] **Step 1: Write the failing tests** in `store/collector.rs`:
  - `schema_version_is_2`: `open_in_memory().schema_version() == 2`.
  - `migrates_v1_to_v2`: create a file DB containing only migration 1 (execute `schema.sql`, then `PRAGMA user_version = 1`, with a raw `rusqlite::Connection` in a `tempfile::tempdir()`), insert one download, then `Store::open` it. Assert `schema_version() == 2`, the download is still listed, and `create_batch` works.
  - `link_roundtrip`: `create_batch("paste","Pasted list · 5 Nov 18:40")` + `insert_link`. The link is `Checking` with `probe == None`. After `set_link_probe(id, Online, Some(&p), None)`, `list_links()[0].probe == Some(p)`.
  - `set_probe_on_missing_link_is_noop`: `set_link_probe(999, Online, None, None)` is `Ok(())`.
  - `delete_links_drops_empty_batch`: a batch with 2 links; deleting one keeps the batch, and deleting the other leaves `list_batches()` empty.
  - `group_package_roundtrip`: `None` first; after `set_group_package(b, "rest", pkg)` it returns `Some(pkg)`.
- [ ] **Step 2: Run them and watch them fail.** Run: `cargo test -p haul-engine store::` → compile errors (missing methods), then failures.
- [ ] **Step 3: Implement** the migration runner, `LinkStatus`, the structs and the methods above.
- [ ] **Step 4: Run the tests.** Run: `cargo test -p haul-engine` → all pass (the plan-1 and plan-2 store tests included).
- [ ] **Step 5: Commit.** `git commit -m "feat(engine): collector tables (migration 2)"`

---

### Task 2: Grouping (pure): archive parts and package names

**Files:**
- Create: `crates/engine/src/grabber/grouping.rs`, `crates/engine/src/grabber/mod.rs` (only `pub mod grouping;` for now)
- Modify: `crates/engine/src/lib.rs` (`pub mod grabber;`)
- Test: unit tests in `grouping.rs`

**Interfaces:**
- Produces:
  - `#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)] pub enum ArchiveKind { #[serde(rename="rar")] Rar, #[serde(rename="7z")] SevenZ, #[serde(rename="zip")] Zip }` with `as_str()` returning `"rar"`, `"7z"` or `"zip"`.
  - `pub struct PartOf { pub base: String, pub kind: ArchiveKind, pub part: u32, pub explicit: bool }`
  - `pub fn archive_part(name: &str) -> Option<PartOf>`. Part numbers:
    - `.partN.rar` → N.
    - `.rar` → 0 (not explicit).
    - `.rNN` → NN + 1.
    - `.7z.NNN` and `.zip.NNN` → NNN.
    - `.zNN` → NN.
    - `.zip` → `u32::MAX` (not explicit; in a split zip it is the last volume).
  - `pub struct Group { pub key: String, pub archive: Option<ArchiveKind>, pub base: Option<String>, pub members: Vec<usize> }`
  - `pub fn group_names(names: &[&str]) -> Vec<Group>`:
    - Keys are `"<kind>:<lowercase base>"` for archive sets and `"rest"` for the rest.
    - A set becomes a group only if at least one member is explicit; a non-explicit set with no explicit sibling falls into `rest`.
    - Members are sorted by `part` (stable), and `base` is taken from the first member in input order.
    - Archive groups come in order of first appearance, then `rest` (omitted when empty) in input order.
  - `pub fn common_name_prefix(names: &[&str]) -> Option<String>`:
    - Needs at least 2 names.
    - Strips each name's last extension, then takes the longest common prefix by chars.
    - If the prefix stops mid-token (its last char and the next char of any name are both alphanumeric), it is cut back to its last separator.
    - Trailing separators (`' ' '-' '_' '.' '(' '[' ','`) are then trimmed.
    - Result: `Some` only if at least 4 chars remain.

- [ ] **Step 1: Write the failing tests.**

```rust
use ArchiveKind::*;
const M: u32 = u32::MAX;

#[test]
fn archive_part_table() {
    let table: &[(&str, Option<(&str, ArchiveKind, u32, bool)>)] = &[
        ("movie.part1.rar", Some(("movie", Rar, 1, true))),
        ("movie.part01.rar", Some(("movie", Rar, 1, true))),
        ("movie.part002.rar", Some(("movie", Rar, 2, true))),
        ("Movie.Part10.RAR", Some(("Movie", Rar, 10, true))),
        ("my.show.s01.part3.rar", Some(("my.show.s01", Rar, 3, true))),
        ("backup 2024.part1.rar", Some(("backup 2024", Rar, 1, true))),
        ("x.part0.rar", Some(("x", Rar, 0, true))),
        ("set.rar", Some(("set", Rar, 0, false))),
        ("set.r00", Some(("set", Rar, 1, true))),
        ("set.r01", Some(("set", Rar, 2, true))),
        ("set.r99", Some(("set", Rar, 100, true))),
        ("SET.R12", Some(("SET", Rar, 13, true))),
        ("set.r100", Some(("set", Rar, 101, true))),
        ("caminandes-3-4k.7z.001", Some(("caminandes-3-4k", SevenZ, 1, true))),
        ("caminandes-3-4k.7z.004", Some(("caminandes-3-4k", SevenZ, 4, true))),
        ("data.7z.0001", Some(("data", SevenZ, 1, true))),
        ("Data.7Z.010", Some(("Data", SevenZ, 10, true))),
        ("pack.zip.001", Some(("pack", Zip, 1, true))),
        ("pack.zip.012", Some(("pack", Zip, 12, true))),
        ("pack.zip", Some(("pack", Zip, M, false))),
        ("godot-4.4-stable-win64.zip", Some(("godot-4.4-stable-win64", Zip, M, false))),
        ("pack.z01", Some(("pack", Zip, 1, true))),
        ("pack.z09", Some(("pack", Zip, 9, true))),
        ("PACK.Z10", Some(("PACK", Zip, 10, true))),
        ("movie.mkv", None),
        ("movie.2024.mkv", None),
        ("report.part.pdf", None),
        ("report.part1.pdf", None),
        ("notes.part1.txt", None),
        ("x.part1.7z", None),
        ("set.r", None),
        ("set.r1", None),
        ("set.rx1", None),
        ("photo.r2d2.png", None),
        ("data.7z", None),
        ("data.7z.1", None),
        ("data.7z.01", None),
        ("dump.7z.001.tmp", None),
        ("pack.zip.1", None),
        ("pack.z1", None),
        ("pack.zipx", None),
        ("archive.tar.gz", None),
        ("archive.tgz", None),
        ("file.001", None),
        ("readme.txt", None),
        ("v1.2.3.exe", None),
        ("setup.rar.exe", None),
        ("movie.part1.rar.html", None),
        ("part1.rar", None),
        (".rar", None),
        ("song.mp3", None),
        ("linux.iso", None),
        ("setup.msi", None),
    ];
    for (name, want) in table {
        let got = archive_part(name).map(|p| (p.base, p.kind, p.part, p.explicit));
        assert_eq!(got, want.map(|(b, k, n, e)| (b.to_string(), k, n, e)), "{name}");
    }
}

fn shape(gs: &[Group]) -> Vec<(&str, Vec<usize>)> {
    gs.iter().map(|g| (g.key.as_str(), g.members.clone())).collect()
}

#[test]
fn groups_mixed_batch() {
    let g = group_names(&["a.part2.rar", "readme.txt", "a.part1.rar", "b.7z.001", "b.7z.002", "c.bin"]);
    assert_eq!(shape(&g), vec![("rar:a", vec![2, 0]), ("7z:b", vec![3, 4]), ("rest", vec![1, 5])]);
    assert_eq!(g[0].base.as_deref(), Some("a"));
}

#[test]
fn lone_rar_and_zip_stay_in_rest() {
    assert_eq!(shape(&group_names(&["x.rar", "y.zip", "z.bin"])), vec![("rest", vec![0, 1, 2])]);
}

#[test]
fn old_style_rar_set() {
    assert_eq!(shape(&group_names(&["s.r01", "s.rar", "s.r00"])), vec![("rar:s", vec![1, 2, 0])]);
}

#[test]
fn split_zip_last_volume() {
    assert_eq!(shape(&group_names(&["p.zip", "p.z01", "p.z02"])), vec![("zip:p", vec![1, 2, 0])]);
}

#[test]
fn case_insensitive_base() {
    let g = group_names(&["X.PART1.RAR", "x.part2.rar"]);
    assert_eq!(shape(&g), vec![("rar:x", vec![0, 1])]);
    assert_eq!(g[0].base.as_deref(), Some("X"));
}

#[test]
fn different_kinds_same_base_separate() {
    assert_eq!(shape(&group_names(&["a.7z.001", "a.zip.001"])), vec![("7z:a", vec![0]), ("zip:a", vec![1])]);
}

#[test]
fn prefix_names() {
    let p = |n: &[&str]| common_name_prefix(n);
    assert_eq!(p(&["godot-4.4-stable-win64.zip", "godot-4.4-stable-linux.zip"]).as_deref(), Some("godot-4.4-stable"));
    assert_eq!(p(&["Movie.Name.S01E01.mkv", "Movie.Name.S01E02.mkv"]).as_deref(), Some("Movie.Name"));
    assert_eq!(p(&["report-final.pdf", "report-draft.pdf"]).as_deref(), Some("report"));
    assert_eq!(p(&["data_2024_01.csv", "data_2024_02.csv"]).as_deref(), Some("data_2024"));
    assert_eq!(p(&["same.bin", "same.bin"]).as_deref(), Some("same"));
    assert_eq!(p(&["abc1.bin", "abc2.bin"]), None);
    assert_eq!(p(&["a.bin", "b.bin"]), None);
    assert_eq!(p(&["one.bin"]), None);
}
```

- [ ] **Step 2: Run them and watch them fail.** Run: `cargo test -p haul-engine grouping` → compile errors.
- [ ] **Step 3: Implement** `archive_part`, `group_names` and `common_name_prefix` in `grouping.rs`. Use plain string matching on the lowercased name, keeping the original case for `base`; no regex crate.
- [ ] **Step 4: Run the tests.** Run: `cargo test -p haul-engine grouping` → PASS.
- [ ] **Step 5: Commit.** `git commit -m "feat(engine): archive-part grouping and package names"`

---

### Task 3: Grabber: intake, probe pool, collector state, start, remove, recheck

**Files:**
- Create: `crates/engine/src/grabber/dupes.rs`
- Modify: `crates/engine/src/grabber/mod.rs`
- Modify: `crates/engine/src/types.rs` (`Event::CollectorChanged`)
- Modify: `crates/engine/src/lib.rs` (export `Grabber`, `ProbeFn`, `CollectorState`, `CollectedLink`, `CollectorPackage`, `DupKind`, `IntakeResult`, `StartDefaults`, `LinkStatus`, `ArchiveKind`)
- Test: unit tests in `dupes.rs` and `mod.rs`; integration tests in `crates/engine/tests/grabber.rs`

**Interfaces:**
- Consumes: Task 1's store methods; Task 2's `group_names`, `common_name_prefix` and `ArchiveKind`; `probe`, `http_client`, `Queue::add`, `Store::set_probe`, `naming::filename_from`.
- Produces:
  - `Event::CollectorChanged` (no fields).
  - `pub type ProbeFn = Arc<dyn Fn(String) -> futures::future::BoxFuture<'static, Result<ProbeInfo, EngineError>> + Send + Sync>`
  - `pub fn http_prober(client: reqwest::Client) -> ProbeFn`: `probe(&client, &url, &HeaderMap::new())` inside `tokio::time::timeout(20 s)`. A timeout becomes `EngineError::Network("timed out")`.
  - `pub fn outcome(r: Result<ProbeInfo, EngineError>) -> (LinkStatus, Option<ProbeInfo>, Option<String>)`:
    - `Ok` → `(Online, Some, None)`.
    - `Offline` → `(Offline, None, Some("404"))`.
    - `LinkExpired(c)` → `(Expired, None, Some(c.to_string()))`.
    - `Http(c)` → `(Error, None, Some(format!("HTTP {c}")))`.
    - anything else → `(Error, None, Some(e.to_string()))`.
  - `#[serde(rename_all = "snake_case")] pub enum DupKind { Downloaded, Queued, Collector }`
  - `pub struct DupKey<'a> { pub urls: Vec<&'a str>, pub name: &'a str, pub size: Option<u64> }`
  - `pub fn find_duplicates(links: &[DupKey], queued: &[DupKey], finished: &[DupKey]) -> Vec<Option<DupKind>>`. A finished match wins over a queued one; `Collector` means it matches an *earlier* entry in `links`.
  - `#[serde(rename_all = "camelCase")] pub struct CollectedLink { pub id: i64, pub batch_id: i64, pub url: String, pub name: String, pub size: Option<u64>, pub content_type: Option<String>, pub status: LinkStatus, pub message: Option<String>, pub source: String, pub duplicate: Option<DupKind>, pub package_key: Option<String> }`
  - `#[serde(rename_all = "camelCase")] pub struct CollectorPackage { pub key: String, pub name: String, pub archive: Option<ArchiveKind>, pub source: String, pub created_at: String }`. The key is `"<batch_id>:<group key>"`.
  - `#[derive(Serialize)] pub struct CollectorState { pub packages: Vec<CollectorPackage>, pub links: Vec<CollectedLink> }`. Both are in display order: batches by id, then within a batch `group_names` order, with each package's links right after it. A leftover `rest` group of one link has `package_key: None`.
  - `#[serde(rename_all = "camelCase")] pub struct IntakeResult { pub added: usize, pub skipped: usize }`
  - `pub struct StartDefaults { pub target_dir: PathBuf, pub connections: u32 }`
  - `Grabber` (`Clone`; holds `Store`, `EventBus`, `ProbeFn` and `Arc<Semaphore>` with 8 permits):
    - `Grabber::new(store: Store, events: EventBus, probe: ProbeFn) -> Grabber`: must run inside Tokio. It re-spawns a probe for every stored link in `Checking`.
    - `fn intake(&self, links: Vec<String>, source: &str) -> Result<IntakeResult, EngineError>`:
      - Skips URLs already in the Collector (exact string).
      - If nothing is left, it creates nothing.
      - Otherwise it creates one batch labelled `Pasted list · <d Mon HH:MM>` (`chrono::Local`, `%-d %b %H:%M`), inserts the links, spawns their probes and sends `CollectorChanged`.
    - `fn state(&self) -> Result<CollectorState, EngineError>`:
      - A link's name is the probe's `file_name` if it is non-empty, else `naming::filename_from(None, url)`.
      - Its size and content type come from the probe.
      - Duplicates are checked against `list_downloads()`: queued means any state but `Completed`.
    - `fn start(&self, ids: &[i64], queue: &Queue, d: &StartDefaults) -> Result<usize, EngineError>`:
      - Ignores unknown ids and links that are `Offline` or `Expired`.
      - For each package group touched, it reuses `group_package` or creates a `packages` row (the package's current name and the batch source), then calls `set_group_package`.
      - Each link goes to `queue.add(NewDownload { package_id, url, target_dir, connections, priority: Normal, .. })` in display order. `set_probe(new_id, &probe)` follows when the link was online, so the Queue shows its size at once.
      - Then it calls `delete_links`, sends `CollectorChanged` and returns the count.
    - `fn remove(&self, ids: &[i64]) -> Result<(), EngineError>`: calls `delete_links`, then sends `CollectorChanged`.
    - `fn recheck(&self, ids: &[i64]) -> Result<(), EngineError>`: sets each link to `Checking` (no probe, no message), spawns its probe and sends `CollectorChanged`.
  - A probe task acquires a permit, awaits `probe(url)`, then calls `set_link_probe(id, outcome(..))` (a no-op if the link is gone) and sends `CollectorChanged`.

Rulings to record in the ledger:
- Collector packages are computed on every read, not stored, so grouping always uses the newest names. A probe that reveals a `Content-Disposition` name can move a link into an archive set while it is still being checked.
- Duplicates are also computed on read (the spec's `duplicate_of` column is not stored), so starting one copy turns the other into "Already queued" at once.
- An exact URL already waiting in the Collector is skipped, not flagged; this is Review Focus 2.

- [ ] **Step 1: Write the failing unit tests.**
  - In `dupes.rs`:
    - `same_final_url`: a link whose `urls = ["https://a/x?sig=1", "https://cdn/x"]` matches a queued download with `urls = ["https://cdn/x"]` → `Queued`.
    - `same_name_and_size_case_insensitive`: `"X.ZIP", Some(10)` vs `"x.zip", Some(10)` → match.
    - `unknown_size_never_matches_by_name`: `"x.zip", None` vs `"x.zip", None` → `None`.
    - `finished_beats_queued`: matches in both → `Downloaded`.
    - `earlier_collector_link`: two links with the same name and size → `[None, Some(Collector)]`.
  - In `mod.rs`: `outcome_mapping` asserts the five mappings above.
- [ ] **Step 2: Write the failing integration tests** in `tests/grabber.rs`. The helper `fake(map: fn(&str) -> Result<ProbeInfo, EngineError>, delay_ms: u64) -> (ProbeFn, Arc<AtomicU32> /* max concurrent */)` names files from the URL's last segment with size 1_000_000. A helper `wait_until(|| cond, 2 s)` polls `state()`.
  - `intake_creates_checking_links_then_online`: two links are `Checking` right after `intake`, then both are `Online` with `size == Some(1_000_000)`.
  - `probes_at_most_8_at_once`: with 20 links and a 50 ms delay, every link ends `Online`, and the max concurrency is between 2 and 8 inclusive.
  - `intake_skips_urls_already_waiting`: the same 2 URLs twice → the second call returns `{added: 0, skipped: 2}`, and `list_batches().len() == 1`.
  - `state_groups_archive_parts`:
    - Intake `.../m.7z.002`, `.../m.7z.001`, `.../notes.txt`, `.../other.bin`.
    - Packages are `["m" (archive 7z), "Pasted list · …" (archive None)]`.
    - Link names in order are `["m.7z.001","m.7z.002","notes.txt","other.bin"]`.
  - `single_link_has_no_package`: one link gives `package_key == None` and no packages.
  - `rest_named_by_prefix`: `godot-4.4-stable-win64.zip` + `godot-4.4-stable-linux.zip` → package name `godot-4.4-stable`.
  - `duplicates_flagged`:
    - Mark a queued download completed (insert a `NewDownload` with the same URL and set its state to `Completed`), then intake that URL → `Downloaded`.
    - Two links in separate intakes with different URLs but the same name and size → the later one is `Collector`.
  - `start_queues_in_part_order_with_package`:
    - Use a `Queue` with a never-matching schedule, as in `app/src-tauri/src/core.rs` tests' `never()`.
    - Start all ids of the `m.7z.*` package. It returns 2.
    - `list_downloads()` holds `m.7z.001` before `m.7z.002`, both with the same `package_id`. `list_packages()` contains `"m"`, and the probe size is known.
    - The Collector no longer holds those links.
  - `start_again_reuses_package`: starting the two parts in two separate calls gives one `package_id`.
  - `start_skips_offline`: a fake returning `Offline` for URLs containing `404` → `start` returns 0 and the link stays.
  - `probe_finishing_after_remove_is_ignored`: the fake waits on a `tokio::sync::Notify`. Remove the link, release the probe and wait 100 ms: `state().links` is empty and `list_batches()` is empty.
  - `checking_links_reprobed_on_start`: insert a batch and link straight into a store (`Checking`), then build `Grabber::new` → the link becomes `Online`.
  - `recheck_reprobes_error_links`: the fake errors on the first call and succeeds on the second → `Error`, then after `recheck`, `Online`.
  - `http_prober_online_and_404`: using `support::serve`, one server with `Spec::new(2_000_000)` and one with `status: Some(404)` → `Online` with size `2_000_000`, and `Offline` with message `"404"`.
- [ ] **Step 3: Run them and watch them fail.** Run: `cargo test -p haul-engine --test grabber` and `cargo test -p haul-engine grabber::` → compile errors.
- [ ] **Step 4: Implement** `dupes.rs`, `outcome`, `http_prober`, `Grabber` and `Event::CollectorChanged`. Any `match` on `Event` elsewhere in the workspace must handle the new variant: the bridge in Task 4 does; the CLI example ignores it.
- [ ] **Step 5: Run the tests.** Run: `cargo test --workspace` → all pass. Also `cargo clippy --workspace --all-targets -- -D warnings` → clean.
- [ ] **Step 6: Commit.** `git commit -m "feat(engine): grabber with probe pool, grouping, duplicates and start"`

---

### Task 4: App commands and the bridge's collector flag

**Files:**
- Modify: `app/src-tauri/src/core.rs`, `dto.rs`, `commands.rs`, `lib.rs`, `bridge.rs`
- Test: unit tests in `core.rs` and `bridge.rs`

**Interfaces:**
- Consumes: Task 3's `Grabber`, `http_prober`, `CollectorState`, `IntakeResult` and `StartDefaults`.
- Produces (DTOs are camelCase):
  - `pub struct CollectorFilters { pub html: bool, pub images: bool, pub small: bool, pub duplicates: bool, pub exts: Vec<String> }` (`Serialize`, `Deserialize`; `Default` = the Global Constraints default).
  - `pub struct CollectorSnapshot { pub packages: Vec<CollectorPackage>, pub links: Vec<CollectedLink>, pub filters: CollectorFilters, pub watch_clipboard: bool }`
  - `pub struct StartResult { pub started: usize }`
  - The bridge's `Update` gains `pub collector: bool`, set when any `Event::CollectorChanged` arrives in the batch. An update with only that flag set is still emitted.
  - `AppCore` gains a `grabber: Grabber` field and `clipboard_on: Arc<AtomicBool>`, loaded from `watch_clipboard`.
    - `AppCore::start_full(store, default_download_dir, schedule, prober: ProbeFn) -> AppCore`
    - `start_with` keeps its signature and calls `start_full` with `http_prober(ctx.client.clone())`.
  - `AppCore::add_links(&self, text: &str, source: &str) -> Result<IntakeResult, EngineError>`:
    - `source` must be `clipboard`, `paste` or `drop`; anything else is an `InvalidInput` error, like `parse_priority`.
    - It runs `extract_links`, then `grabber.intake`.
  - `AppCore::collector_snapshot(&self) -> Result<CollectorSnapshot, EngineError>`
  - `AppCore::collector_start(&self, ids: &[i64]) -> Result<StartResult, EngineError>`, with `StartDefaults { target_dir: download_dir, connections: 8 }`.
  - `AppCore::collector_remove(&self, ids: &[i64]) -> Result<(), EngineError>`
  - `AppCore::collector_recheck(&self, ids: &[i64]) -> Result<(), EngineError>`
  - `AppCore::set_collector_filters(&self, f: CollectorFilters) -> Result<(), EngineError>`: normalises `exts` (lowercase, no leading dot, no empty or duplicate entries) and persists them.
  - `AppCore::set_watch_clipboard(&self, on: bool) -> Result<(), EngineError>` (persists it and sets the atomic) and `AppCore::watch_clipboard(&self) -> bool`.
  - Tauri commands, all async on a blocking thread as in plan 2:
    - `add_links(text, source)` → `IntakeResult`
    - `collector_snapshot` → `CollectorSnapshot`
    - `collector_start(ids)` → `StartResult`
    - `collector_remove(ids)`
    - `collector_recheck(ids)`
    - `set_collector_filters(filters)`
    - `set_watch_clipboard(on)`

  All of them are registered in `lib.rs`.

- [ ] **Step 1: Write the failing tests.**
  - In `core.rs`, using `start_full` with a fake prober that is always `Online` (name from the URL, size 1_000_000):
    - `add_links_goes_to_collector`: two links → `IntakeResult { added: 2, skipped: 0 }`, `snapshot().downloads` is empty, and `collector_snapshot().links.len() == 2`.
    - `add_links_rejects_unknown_source`: `add_links("https://a/x", "browser")` is `Err`.
    - `add_no_links`: replaces plan 2's test → `{added: 0, skipped: 0}`.
    - `collector_start_moves_to_queue`: after the probes finish, starting both → `started == 2`, `snapshot().downloads.len() == 2` sharing one package, and the Collector is empty.
    - `filters_default_and_persist`: the default has `html` and `images` set. `set_collector_filters({.., exts: [".ISO","iso",""]})` gives `exts == ["iso"]`, and it survives reopening the same file DB.
    - `watch_clipboard_defaults_off_and_persists`.
  - Delete plan 2's `add_one_link_no_package` and `add_many_links_one_package`; the grabber tests cover that behaviour now.
  - In `bridge.rs`: `collector_change_sets_flag`: sending `Event::CollectorChanged` gives one update with `collector == true` and empty lists.
- [ ] **Step 2: Run them and watch them fail.** Run: `cargo test -p haul-app` → compile errors.
- [ ] **Step 3: Implement** the DTOs, methods, commands and bridge flag. Move the `Pasted list` naming out of `core.rs`, since `Grabber` owns it now.
- [ ] **Step 4: Run the tests.** Run: `cargo test --workspace` and clippy → clean.
- [ ] **Step 5: Commit.** `git commit -m "feat(app): collector commands and update flag"`

---

### Task 5: Clipboard watcher

**Files:**
- Create: `app/src-tauri/src/clipboard.rs`
- Modify: `app/src-tauri/Cargo.toml` (`clipboard-master = "4"`, `arboard = "3"`), `app/src-tauri/src/lib.rs` (`pub mod clipboard;`; call `clipboard::spawn_watcher(core.clone())` in `setup` after `app.manage`, logging a failure but not panicking)
- Test: unit tests in `clipboard.rs`

**Interfaces:**
- Consumes: `AppCore::watch_clipboard`, `AppCore::add_links(text, "clipboard")` and `AppCore::events` (Task 4).
- Produces:
  - `pub const MAX_CLIPBOARD_BYTES: usize = 1_000_000;`
  - `pub struct ClipboardFilter`, with `ClipboardFilter::new()`, `fn accept(&mut self, text: &str) -> Option<String>` and `fn note_own_write(&mut self, text: &str)`. `accept` returns `None` when:
    - the text is longer than `MAX_CLIPBOARD_BYTES`;
    - it equals the last text Haul wrote (`note_own_write`);
    - it equals the last text seen (consecutive change events often repeat the same text);
    - or it has no links (`extract_links` is empty).

    Every call updates "last seen" (hashes, not copies of the text).
  - `pub fn spawn_watcher(core: AppCore) -> std::io::Result<()>`:
    - Starts a thread named `haul-clipboard` that runs `clipboard_master::Master` with a handler.
    - On each change, while `core.watch_clipboard()`, it reads `arboard::Clipboard::get_text()`. A read error is logged at debug level and skipped.
    - The text goes through `filter.accept`, then `core.add_links(&text, "clipboard")`. An error is logged.
    - While the switch is off, change events are ignored entirely, and turning it on does not grab what is already on the clipboard.

Ruling to record: `note_own_write` has no caller yet, because Haul writes nothing to the clipboard. It is there for the first "Copy link" action, so the spec's "ignores text Haul itself put on the clipboard" holds from that point on.

- [ ] **Step 1: Write the failing tests:**
  - `filter_accepts_links_once`: `accept("see https://a.org/x")` is `Some`, and the same text again is `None`.
  - `filter_rejects_text_without_links`.
  - `filter_rejects_own_write`: `note_own_write(t)`, then `accept(t)` is `None`.
  - `filter_rejects_huge_text`: 1_000_001 bytes containing a link → `None`.
  - `filter_accepts_again_after_other_text`: A, B, A → `Some`, `Some`, `Some`.
- [ ] **Step 2: Run them and watch them fail.** Run: `cargo test -p haul-app clipboard` → compile errors.
- [ ] **Step 3: Implement** `ClipboardFilter` and `spawn_watcher`. Check `clipboard-master`'s handler trait (`ClipboardHandler::on_clipboard_change` returning `CallbackResult::Next`) against the version Cargo resolves.
- [ ] **Step 4: Run the tests and the build.** Run: `cargo test --workspace`, clippy, and `cd app && npx tauri build --debug --no-bundle` → it compiles.
- [ ] **Step 5: Commit.** `git commit -m "feat(app): clipboard watcher"`

---

### Task 6: Front-end collector model, backend and mock

**Files:**
- Modify: `app/src/lib/types.ts`, `backend.ts`, `backend-tauri.ts`, `backend-mock.ts`
- Create: `app/src/lib/collector.ts`, `app/src/lib/collector.test.ts`, `app/src/lib/backend-mock-collector.ts` (the mock's collector simulation, used by `backend-mock.ts`)
- Modify: `app/src/lib/backend-mock.test.ts`

**Interfaces:**
- Consumes: Task 4's commands and DTO field names.
- Produces (TS, mirroring the DTOs):
  - `type LinkStatus = 'checking' | 'online' | 'offline' | 'expired' | 'error'`
  - `type DupKind = 'downloaded' | 'queued' | 'collector'`
  - `type Source = 'clipboard' | 'paste' | 'drop'`
  - `interface CollectedLink { id; batchId; url; name; size: number|null; contentType: string|null; status: LinkStatus; message: string|null; source: string; duplicate: DupKind|null; packageKey: string|null }`
  - `interface CollectorPackage { key: string; name: string; archive: 'rar'|'7z'|'zip'|null; source: string; createdAt: string }`
  - `interface CollectorFilters { html: boolean; images: boolean; small: boolean; duplicates: boolean; exts: string[] }`
  - `interface CollectorSnapshot { packages; links; filters; watchClipboard: boolean }`
  - `interface IntakeResult { added: number; skipped: number }`
  - `interface StartResult { started: number }`
  - `Update.collector: boolean`
  - `Backend` changes:
    - `addLinks(text, source: Source)` replaces the old one-argument version.
    - New methods: `collectorSnapshot()`, `collectorStart(ids)`, `collectorRemove(ids)`, `collectorRecheck(ids)`, `setCollectorFilters(f)`, `setWatchClipboard(on)`.
    - `tauriBackend` invokes the Task 4 command names, with camelCase arguments: `{ text, source }`, `{ ids }`, `{ filters }` and `{ on }`.
  - `collector.ts` (pure):
    - `isHidden(l: CollectedLink, f: CollectorFilters): boolean`
    - `type CItem = { kind: 'package'; pkg: CollectorPackage; links: CollectedLink[]; visible: CollectedLink[] } | { kind: 'link'; link: CollectedLink; child: boolean }`
    - `buildCollectorList(s: CollectorSnapshot, collapsed: Set<string>, removed: Set<number>): CItem[]`:
      - Skips removed links and filter-hidden ones.
      - Omits a package whose visible links are all gone.
      - Leaves out a collapsed package's children.
    - `linkStatusWord(l): string`
    - `packageStatus(links: CollectedLink[]): { word: string; tone: 'ok' | 'err' | 'checking' }`:
      - any `checking` → `Checking…`;
      - else, offline + expired > 0 → `<k> offline` (err);
      - else, error > 0 → `<k> couldn't check` (err);
      - else `<n> / <n> online` (ok).
    - `packageTag(pkg, links): string`
    - `subLine(s, removed): string`
    - `startableIds(links, f): number[]` (visible and not offline or expired)
    - `onlineIds(s, f, removed): number[]` (visible and online)
    - `dupLabel(k: DupKind): string`
    - `sourceLabel(s: string): string`
  - The mock collector simulates:
    - **Intake:** `extract` as `mockExtract`, skipping exact URLs already waiting.
    - **Checks:** each link resolves after `checkMs` (default 1200; `createMockBackend({ checkMs })`).
      - Status by URL: `404` → offline (`"404"`), `expired` → expired (`"403"`), `unreachable` → error (`"timed out"`), anything else online.
      - Size from the URL hash, as the mock's downloads already do; `contentType` from the extension.
    - **Grouping:** `.partN.rar` and `.7z.NNN` sets only; the rest of a batch is `Pasted list · <date>`, and a single leftover link has no package.
    - **Duplicates:** by URL, against the mock's downloads only.
    - **Start:** creates mock downloads (and a mock package for a package group).
    - **Notifications:** after each change, `onUpdate` listeners get `{ collector: true, … }` on the next tick.
    - **Demo seed** (`demo: true`): the mockup's Collector rows.
      - `caminandes-3-4k.7z.001`–`004`: 001–003 online, 004 checking.
      - A `Pasted list · 5 Nov 18:40` holding `openimages-v7-sample.csv`, `mozilla-common-voice-en.tar.gz` (marked `downloaded`) and `readme-old.pdf` (offline).
      - `godot-4.4-stable` with 2 online files.
      - Sources: clipboard, paste and paste.

- [ ] **Step 1: Write the failing tests** in `collector.test.ts`. Build fixtures with a small `link({...})` factory.
  - `hides_by_chip`: one case per chip:
    - html by content type and by `.php`;
    - images by `image/png` and by `.JPG`;
    - small at `999_999` but not at `1_000_000` or `null`;
    - duplicates.
  - `ext_filter_matches_lowercase`: `exts: ['iso']` hides `Linux.ISO`.
  - `build_list_packages_and_lone_rows`: a package with 2 links, then a lone link → `['package', 'link'(child), 'link'(child), 'link'(not child)]`; collapsed → `['package', 'link']`.
  - `package_hidden_when_all_links_hidden`.
  - `package_status_words`: `4 / 4 online`, `1 offline` (an offline link plus 3 online), `Checking…`, `2 couldn't check`.
  - `package_tag`: `7z · 4 parts`, `rar · 1 part`, `8 files`, `1 file`.
  - `sub_line`: `Links waiting for review · 14 links in 3 packages · 2 hidden`, `Links waiting for review · 1 link in 0 packages`, `Nothing waiting`.
  - `startable_skip_hidden_and_offline`; `online_ids_only_visible_online`.
  - `link_status_words`: `Offline (404)`, `Link expired (410)`, `Couldn't check`.
- [ ] **Step 2: Write the failing mock test** `mock_collector_flow` in `backend-mock.test.ts`, with fake timers and `checkMs: 100`:
  - `addLinks('https://x.org/a.bin https://x.org/404.bin', 'paste')` → `{added: 2}`.
  - After 100 ms, the statuses are `online` and `offline`.
  - `collectorStart([onlineId])` → `{started: 1}`; `snapshot().downloads` has `a.bin`, and the collector has 1 link left.
  - A second identical `addLinks` → `{added: 0, skipped: 2}`.
- [ ] **Step 3: Run them and watch them fail.** Run: `cd app && npm test` → failures in the new tests. The existing callers of `addLinks(text)` now fail `npm run check`; Task 7 fixes them.
- [ ] **Step 4: Implement** the types, backends, mock simulation and `collector.ts`.
- [ ] **Step 5: Run the tests.** Run: `npm test` → all pass.
- [ ] **Step 6: Commit.** `git commit -m "feat(ui): collector model, backend commands and mock"`

---

### Task 7: Collector view, intake anywhere, clipboard toggle

**Files:**
- Create: `app/src/lib/collector-state.svelte.ts`, `app/src/lib/collector-state.svelte.test.ts`, `app/src/components/CollectorView.svelte`, `CollectorRow.svelte`, `FilterChips.svelte`, `app/e2e/collector.spec.ts`
- Modify:
  - `app/src/lib/app-state.svelte.ts` (`view` moves here from `App.svelte`; `addText(text, source)` toasts `Sent …`/`Already in the Collector`/`No links found` with a `Show` action that sets `view = 'collector'`; `showToast` is exposed).
  - `App.svelte`: hosts the window paste/drop handlers moved from `QueueView`, with source `paste`/`drop`, and renders `CollectorView`.
  - `QueueView.svelte`: the paste/drop handlers are removed, and AddLinks calls `app.addText(text, 'paste')`.
  - `Rail.svelte`: a `collectorCount` prop for the badge.
  - `StatusBar.svelte`: the `Watch clipboard` toggle after Speed limit, with `aria-pressed` and the title from the Global Constraints.
  - `Icon.svelte` if needed.

**Interfaces:**
- Consumes: Task 6's `Backend` and `collector.ts`.
- Produces:
  - `createCollectorState(backend: Backend, ui: { showToast: AppState['showToast']; onStarted: () => void })` returns an object with:
    - `snapshot: CollectorSnapshot` (`$state`)
    - `load()`: newest-load-wins, as in the Queue state
    - `onUpdate(u)`: loads when `u.collector`
    - `selected: string | number | null` (a package key or a link id)
    - `collapsed: Set<string>`, `removed: Set<number>`
    - `items` (`$derived` `buildCollectorList`)
    - `startPackage(key)`, `startLink(id)`, `startAllOnline()`: each calls `collectorStart`, toasts `Started <n> download(s)` and then calls the Queue state's `load()` via an injected `onStarted` callback
    - `removeWithUndo(ids)`: hides the ids at once, commits `collectorRemove` after `UNDO_MS`, and Undo restores them
    - `clearOffline()`: `removeWithUndo` on all offline and expired links, with the `Cleared …` toast
    - `recheck(id)`, `toggleChip(name)`, `addExt(ext)`, `removeExt(ext)`: the last three call `setCollectorFilters` optimistically and roll back on error
    - `setWatchClipboard(on)`
  - **Keyboard in the Collector** (`keyAction` from `keys.ts`, ignored in text fields):
    - Up/Down move the selection through visible items.
    - Right/Left expand or collapse a package.
    - Enter starts the selected package or link.
    - Delete removes it (a package removes all its links).
  - Error links show a `Check again` link button in the Online cell that calls `recheck`.

- [ ] **Step 1: Write the failing unit test** `collector-state.svelte.test.ts` (mock backend, fake timers):
  - `remove_hides_then_commits`: after `removeWithUndo([id])` the item is gone from `items` and `collectorRemove` has not been called; after 6 s it has been called once.
  - `undo_restores`.
  - `update_flag_reloads`: `onUpdate({collector: true, …})` triggers `collectorSnapshot`.
- [ ] **Step 2: Write the failing e2e tests** in `e2e/collector.spec.ts`, against the mock with `checkMs` default, in both themes as in plan 2's config:
  - `paste_goes_to_collector`:
    - In the Queue, dispatch a paste of `https://x.org/a.bin https://x.org/b.bin`. The toast `Sent 2 links to the Collector` appears.
    - Click `Show`: the Collector heading is visible, a package row `Pasted list · …` has the tag `2 files`, and both rows reach `Online`.
  - `paste_twice_says_already`: the same paste twice → the second toast is `Already in the Collector`, and there are still 2 rows.
  - `archive_parts_grouped`: pasting `…/m.7z.001 …/m.7z.002 …/m.7z.003 …/lone.bin` → package `m` with tag `7z · 3 parts`, and `lone.bin` as a non-child row.
  - `offline_flagged_and_cleared`: a `404` link shows `Offline (404)`. `Clear offline` → the row is gone and the toast is `Cleared 1 offline link`; `Undo` brings it back.
  - `start_package_moves_to_queue`: `Start package` → the toast is `Started 2 downloads`, the Collector shows the empty state, and the Queue lists `a.bin` and `b.bin` under the package.
  - `filter_chip_hides_and_persists`: in demo mode, toggle `under 1 MB` and add the ext `csv` through `+ Type`. `openimages-v7-sample.csv` is hidden, the sub line says `· 1 hidden`, and after a reload it is still hidden. The mock persists filters in `sessionStorage`, like the speed limit.
  - `collector_keyboard`: with the first package selected, ArrowDown moves to its first child. Delete removes that child, and an Undo toast appears. Enter on the package starts it.
  - `clipboard_toggle`: `Watch clipboard` flips `aria-pressed`, and `__haulMockCalls` contains `setWatchClipboard`.
  - `rail_badge_counts_links`: in demo mode, the Collector rail button contains the demo link count.
- [ ] **Step 3: Run them and watch them fail.** Run: `npm test`, `npm run e2e -- collector` → failures.
- [ ] **Step 4: Implement** the state, components and wiring. Follow the mockup and the Global Constraints for layout and copy. `CollectorRow` is a button-like row with `aria-selected` inside a `role="listbox"` container, matching how the Queue list is built. The pulse animation on the checking dot follows `prefers-reduced-motion`.
- [ ] **Step 5: Run the tests.** Run: `npm test`, `npm run check`, `npm run e2e -- collector` → all pass.
- [ ] **Step 6: Commit.** `git commit -m "feat(ui): Collector view, paste and drop anywhere, clipboard toggle"`

---

### Task 8: Existing tests, accessibility, real-app smoke and docs

**Files:**
- Create: `app/e2e/helpers.ts`
- Modify: `app/e2e/queue.spec.ts`, `drawer.spec.ts`, `remove-reorder.spec.ts`, `a11y.spec.ts`, `app/e2e-real/smoke.spec.ts`, `app/src/lib/app-state.svelte.test.ts`, `DESIGN.md`, `handoff.md`

**Interfaces:**
- Produces: `export async function addViaCollector(page: Page, text: string): Promise<void>`. It pastes the text, clicks `Show`, waits until no row says `Checking…`, clicks `Start all online` and returns to the Queue.

- [ ] **Step 1: Run the full e2e suite** (`npm run e2e`) and list each plan-2 test that fails because pasting no longer queues directly.
- [ ] **Step 2: Fix the failing tests.** Switch each one to `addViaCollector`, and fix `app-state.svelte.test.ts` for the new `addText(text, source)`. Change no assertion about Queue behaviour.
- [ ] **Step 3: Extend the a11y check.** `a11y.spec.ts` gains `collector_has_no_violations`: demo mode, the Collector view, axe with the same rules as the Queue test, in both themes.
- [ ] **Step 4: Update the real-app smoke run.**
  - The existing smoke tests add through the Collector: paste, open the Collector, wait for `Online`, `Start package` or `Start`, then go back to the Queue.
  - New test `clipboard_link_reaches_collector`:
    - Turn on `Watch clipboard`.
    - Run `powershell -NoProfile -Command "Set-Clipboard -Value 'http://127.0.0.1:8765/clip.bin'"` through `child_process.execFileSync`. Check that `scripts/range-server.mjs` serves that path, and widen it to any path if it doesn't.
    - Expect a `clip.bin` row with `Online` in the Collector within 5 s.
    - Turn the toggle off again at the end.
- [ ] **Step 5: Run every check.** All must be green:
  - `cargo test --workspace`
  - `cargo clippy --workspace --all-targets -- -D warnings`
  - `cargo fmt --check`
  - `cd app && npm test && npm run check && npm run e2e`
  - The smoke run with the three shells in `handoff.md`; it must report 3 passed.
- [ ] **Step 6: Update the docs.**
  - `DESIGN.md`: add the Collector grid, row heights, chips, the duplicate badge and the 130px Online column.
  - `handoff.md`: plan 3 status, the new commands and settings keys, the plan 3 rulings (from the ledger), and next = plan 4.
- [ ] **Step 7: Commit.** `git commit -m "test(app): e2e through the Collector, a11y, clipboard smoke; docs: plan 3"`
