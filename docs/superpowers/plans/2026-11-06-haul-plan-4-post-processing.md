# Haul Plan 4: Post-processing + Rules Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** When every file in a package has finished, Haul verifies checksums, extracts archives with a bundled 7-Zip (trying saved passwords, then asking), sorts the results into category folders or by the user's ordered rules, optionally scans with Windows Defender, and sends a Windows notification. Every step shows in the drawer. A new Rules view edits the switches, categories, rules and saved passwords.

**Architecture:**
- **Engine:**
  - `rules.rs` (pure): categories, rule matching and rename patterns.
  - `postproc/`: `checksum.rs` (pure parsing plus file hashing), `extract.rs` (the `Extractor` trait plus `SevenZip`), `sort.rs` (moves that never overwrite, plus the `Recycler` trait), `scan.rs` (the `Scanner` trait plus `Defender`), `mod.rs` (the `PostProc` service).
  - `secret.rs`: DPAPI.
  - `store/postproc.rs`: runs, passwords and rules (migration 4).
  - `PostProc` subscribes to the `EventBus`. When a `StateChanged { Completed }` finishes a package, it creates a run. One worker task executes runs one at a time, and every step change is saved and broadcast as `Event::PostProcChanged`.
  - The OS-facing parts (7-Zip, the Recycle Bin, Defender, notifications) sit behind traits, so tests can fake them. The real 7-Zip is also tested against committed fixtures.
- **App crate:**
  - `AppCore` owns a `PostProc` and exposes the commands.
  - `haul:update` gains `postproc: true`.
  - Notifications go through `tauri-plugin-notification`.
  - 7-Zip ships as a Tauri resource.
- **UI:**
  - `lib/postproc.ts` (pure words and states) and `lib/postproc-state.svelte.ts` (runs by package and download).
  - The drawer gets an "After download" section, and package rows show the running step.
  - `RulesView.svelte` with `RuleEditor.svelte`, `CategoryTable.svelte` and `PasswordList.svelte`.
  - The browser mock simulates runs.

**Tech Stack:** as in plan 3, plus these crates:
- Engine: `sha2 0.11`, `md-5 0.11`, `sha1 0.11`, `crc32fast 1.5`, `base64 0.23`, `globset 0.4`, `regex 1`, `trash 5`, and `windows-sys 0.61` (features `Win32_Foundation`, `Win32_Security_Cryptography`, `Win32_System_Memory`).
- App: `tauri-plugin-notification 2`.
- Bundled: 7-Zip (latest stable x64 from 7-zip.org) `7z.exe` + `7z.dll` + `License.txt`.

**Spec:** `docs/superpowers/specs/2026-11-06-haul-post-processing-design.md` (the plan-4 design, owner decisions), plus section 5 "Post-processing" and section 8 of `docs/superpowers/specs/2026-11-05-haul-design.md`. Visual reference: the drawer's "After download" list in `docs/mockup/index.html` (lines 249–256, `.steps` CSS) and `DESIGN.md`.

**Not in this plan (on purpose):** the rail's category filter list and the Finished view (plan 7); browser intake (plan 5); an "Open folder" action on the notification; a Settings page (the four switches live in the Rules view for now).

## Global Constraints

- Everything in plan 2's and plan 3's Global Constraints still holds (tokens, fonts, motion, icon plus word, never destroy data).
- **Never destroy user data:** Haul removes a file only by moving it to the Recycle Bin (`trash`). The exceptions are its own temporary extraction folder (`<out>.haul-tmp`) and a cross-drive move's source after the copy's size checks out.
- **Run states** (`RunState::as_str`): `held` (auto mode off, waiting for Process), `queued`, `running`, `waiting` (a file is re-downloading), `needs_password`, `failed`, `done`.
- **Step kinds** in order: `verify`, `extract`, `sort`, `scan`, `notify`. **Step states:** `pending`, `running`, `done`, `skipped`, `failed`, `needs_password`.
- **Settings:**
  - Key `postproc`, JSON `{"auto":bool,"sortCategories":bool,"recycleArchives":bool,"virusScan":bool}`. The default is `{"auto":true,"sortCategories":true,"recycleArchives":false,"virusScan":false}`.
  - Key `categories`, a JSON array of `{"name","exts":[...],"folder"}`. A `folder` is absolute, or relative to the download folder.
- **Default categories** (the folder is the name):
  - Video: `mp4 mkv avi mov wmv flv webm m4v mpg mpeg ts 3gp`
  - Music: `mp3 flac wav aac ogg opus m4a wma aiff`
  - Documents: `pdf doc docx xls xlsx ppt pptx odt ods odp txt rtf csv epub md`
  - Programs: `exe msi msix appx bat cmd ps1 iso img apk`
  - Compressed: `zip rar 7z tar gz tgz bz2 xz zst cab`, plus any name `archive_part` recognises
  - Other: no extensions, takes everything else
- **Checksum files:** `.sfv` (CRC32), `.md5`, `.sha1`, `.sha256`, and files named `MD5SUMS`, `SHA1SUMS` or `SHA256SUMS` (case-insensitive, optionally ending in `.txt`). Hex is compared lowercase.
- **Pasted hash:** surrounding whitespace is trimmed, and an optional `md5:`, `sha1:`, `sha256:` or `crc32:` prefix is allowed. The length tells the algorithm: 8 hex = CRC32, 32 = MD5, 40 = SHA-1, 64 = SHA-256. Anything else is rejected with `Not a checksum: use 8, 32, 40 or 64 hex characters`.
- **Server digest:** `Repr-Digest` / `Digest` with `sha-256` or `md5` (base64), from any probe reply. `Content-MD5` only from a `HEAD` or a full `200`. Stored as `"<algo>:<lowercase hex>"`.
- **Re-download on mismatch:** at most once per download (`verify_retries`). The bad file goes to the Recycle Bin first.
- **7-Zip command:** `7z x -y -bso0 -bsp0 -p<pw> -o<tmp> <first volume>`, with stdin null and no window (`CREATE_NO_WINDOW`).
  - Exit 0 is success.
  - Exit 2 with `Wrong password` in its output is a wrong password.
  - Any other exit is a failure, with the output's last `ERROR:` line as the message, else `7-Zip exit <n>`.
- **Password order:** empty first, then saved passwords by position. A password given in the drawer is tried before the saved ones.
- **Extraction folder:** `<package folder>\<package name>`, made unique like files (`name (1)`). It is extracted into `<that>.haul-tmp` and renamed when done.
- **Nested archives:** first volumes found anywhere inside the output are extracted once, each into a sibling folder named after its base. This does not recurse further.
- **Rename tokens:** `{name}` is the stem, `{ext}` the extension without the dot, `{package}`, `{host}`, and `{date}` as local `YYYY-MM-DD`. The result goes through `naming::sanitize_filename`.
- **Rule matching:**
  - Host: case-insensitive, exact, or `*.x.com`, which matches `x.com` and its subdomains.
  - Name: a glob over the whole file name, case-insensitive, or a regex when written `/…/`, also case-insensitive.
  - Extensions: lowercase, no dot. Sizes are inclusive bytes.
  - An unset field matches everything, and a rule with no fields set matches every file.
- **Copy (exact):**
  - Steps are titled `Verify checksum`, `Extract`, `Sort`, `Virus scan` and `Notify`.
  - Step details:
    - `<ALGO> from <file>`, `<ALGO> from your checksum` and `<ALGO> from the server`, where ALGO is `CRC32`, `MD5`, `SHA-1` or `SHA-256`.
    - `No checksum`, `Checksum mismatch: <name>`, `Downloading <name> again`, `File missing: <name>`.
    - `Not an archive`, `Extracted to <path>`, `Needs a password`, `Wrong password`, `Archive is damaged: <msg>`, `<n> inner archive(s) couldn't be extracted` (appended after `; ` to `Extracted to …`).
    - `Rule “<rule name>” → <folder>`, `<Category> → <folder>`, `→ <n> folders`, `Nothing to move`.
    - `Off`, `No threats`, `Threat found: <path>`, `Windows Defender not found`.
    - `Sent`.
  - Package row words: `Verifying`, `Extracting`, `Sorting`, `Scanning`, `Needs password`, `Needs attention` (failed), `Waiting to process` (held), `Re-downloading`.
  - Drawer section `After download`, with these controls:
    - Buttons `Process`, `Retry`, `Use password`.
    - A tick `Remember this password`.
    - A field labelled `Expected checksum`, with the placeholder `Paste MD5, SHA-1, SHA-256 or CRC32`, and the button `Save`.
  - Notification: title `<package> is ready` or `<package> needs attention`; body = the extraction or sort detail.
  - Rules view:
    - Title `Rules`.
    - Section headings `After download`, `Categories`, `Your rules`, `Saved passwords`.
    - Switches `Process finished packages automatically`, `Sort into category folders`, `Send archives to the Recycle Bin after extracting`, `Scan with Windows Defender`.
    - Buttons `Add rule`, `Add password`, `Save`, `Cancel`, `Delete`, `Move up`, `Move down`.
    - Try box label `Try a file name`, result `→ <folder>\<name> (<by>)`.
    - Rule fields: `Name`, `Host`, `File name`, `Extensions`, `Min size (MB)`, `Max size (MB)`, `Folder`, `Rename to`.
    - Empty rules list: `No rules yet. Files go to their category folder.`
    - A password row shows `•••••••• (<n>)`.
  - Toasts: `Rules saved`, `Password saved`, `Checksum saved`, and the validation message as a toast when a save is rejected.

## Review Focus

1. **A wrong password typed in the drawer.** Expected: the step stays `needs_password` with `Wrong password`, the password is not saved even with Remember ticked, and the box is shown again. Test: `given_wrong_password_stays_needed` (Task 6), e2e `password_prompt_wrong_then_right` (Task 9).
2. **The sort target already has a file of that name** (the same file downloaded twice). Expected: the new one becomes `name (1).ext`, and the old one is untouched. Test: `sort_never_overwrites` (Task 5).
3. **Haul closes in the middle of an extraction.** Expected: next launch restarts that step, removes the stale `.haul-tmp`, and ends with one output folder, not `name (1)`. Test: `interrupted_extract_restarts_clean` (Task 6).
4. **A file vanished from disk** (deleted or moved by the user) before its step ran. Expected: the step fails with `File missing: <name>`, nothing panics, and Retry works once the file is back. Test: `missing_file_fails_step` (Task 6).
5. **A bad regex or glob in a rule.** Expected: the save is rejected with a message, and the stored rules stay unchanged. A broken rule already stored is skipped at run time. Tests: `validate_rejects_bad_regex` (Task 2), `broken_stored_rule_is_skipped` (Task 2), e2e `rule_editor_rejects_bad_regex` (Task 9).

---

### Task 1: Store migration 4 and DPAPI

**Files:**
- Create: `crates/engine/src/store/migration_4.sql`, `crates/engine/src/store/postproc.rs`, `crates/engine/src/secret.rs`
- Modify:
  - `crates/engine/src/store/mod.rs`: `SCHEMA_VERSION = 4`, the migration list, the `Download` fields, `DOWNLOAD_COLS`, `set_probe`, and a new `reset_download`.
  - `crates/engine/src/types.rs`: `ProbeInfo.digest`, and derive `Default`.
  - `crates/engine/src/lib.rs` and `crates/engine/Cargo.toml` (`windows-sys`).
  - Every `ProbeInfo { .. }` literal, 11 of them: add `digest: None`.
- Test: unit tests in `store/postproc.rs` and `secret.rs`. Rename `schema_version_is_3` to `schema_version_is_4`.

**Interfaces:**
- Produces:
  - `ProbeInfo.digest: Option<String>`, with `#[serde(default)]` so the stored collector `probe_json` still parses.
  - `Download` gains `expected_hash: Option<String>`, `server_digest: Option<String>` and `verify_retries: u32`. `set_probe` writes `server_digest` from `p.digest`.
  - `Store::reset_download(&self, id) -> Result<(), EngineError>`: sets state `queued`, clears the error and `target_path`, and deletes the segments.
  - `Store::set_expected_hash(&self, id, h: Option<&str>)` and `Store::bump_verify_retries(&self, id) -> Result<u32, EngineError>`, which returns the new count.
  - Runs, where `RunRow { id: i64, package_id: Option<PackageId>, download_id: Option<DownloadId>, state: String, steps_json: String, created_at: String, updated_at: String }`:
    - `Store::ensure_run(&self, package_id, download_id, state: &str, steps_json: &str) -> Result<(i64, bool), EngineError>`: inserts, or returns the existing row's id. The `bool` is `true` when the row is new.
    - `Store::get_run(&self, id) -> Result<RunRow, EngineError>`, `Store::list_runs(&self) -> Result<Vec<RunRow>, EngineError>` (by id), and `Store::save_run(&self, id, state: &str, steps_json: &str)`, which also bumps `updated_at`.
  - Passwords: `Store::add_password(&self, blob: &[u8]) -> Result<i64, EngineError>` (appended last), `Store::list_passwords(&self) -> Result<Vec<(i64, Vec<u8>)>, EngineError>` (by position), `Store::delete_password(&self, id)`, and `Store::order_passwords(&self, ids: &[i64])`, which gives positions 0..n in that order.
  - Rules: `Store::get_rules_json(&self) -> Result<Vec<(i64, bool, String, String, String)>, EngineError>`, as (id, enabled, name, match_json, action_json) by position, and `Store::set_rules_json(&self, rows: &[(bool, String, String, String)])`, which replaces them all in one transaction.
  - `secret::protect(plain: &[u8]) -> Result<Vec<u8>, EngineError>` and `secret::unprotect(blob: &[u8]) -> Result<Vec<u8>, EngineError>`: `CryptProtectData` / `CryptUnprotectData` for the current user, with the output freed through `LocalFree`.

Migration 4:

```sql
ALTER TABLE downloads ADD COLUMN expected_hash TEXT;
ALTER TABLE downloads ADD COLUMN server_digest TEXT;
ALTER TABLE downloads ADD COLUMN verify_retries INTEGER NOT NULL DEFAULT 0;
CREATE TABLE postproc_runs (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    package_id  INTEGER UNIQUE REFERENCES packages(id) ON DELETE CASCADE,
    download_id INTEGER UNIQUE REFERENCES downloads(id) ON DELETE CASCADE,
    state       TEXT NOT NULL,
    steps_json  TEXT NOT NULL,
    created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);
CREATE TABLE passwords (id INTEGER PRIMARY KEY AUTOINCREMENT, position INTEGER NOT NULL, blob BLOB NOT NULL);
CREATE TABLE rules (
    id INTEGER PRIMARY KEY AUTOINCREMENT, position INTEGER NOT NULL, enabled INTEGER NOT NULL,
    name TEXT NOT NULL, match_json TEXT NOT NULL, action_json TEXT NOT NULL
);
```

- [ ] **Step 1: Write the failing tests:**
  - `schema_version_is_4`.
  - `migrates_v3_to_v4`: a file DB holding migrations 1–3 and one download is opened. The download keeps its fields, with `verify_retries == 0`.
  - `old_probe_json_without_digest_parses`: a collector link stored with probe JSON that has no `digest` key still lists, with `probe.digest == None`.
  - `set_probe_stores_digest`.
  - `reset_download_requeues_from_zero`: a completed row with segments ends up `Queued`, with `target_path == None` and `load_segments` empty.
  - `bump_verify_retries_counts`: returns 1, then 2.
  - `ensure_run_is_idempotent`: the second call for the same package returns the same id with `false`.
  - `run_roundtrip_and_cascade`: `save_run` updates the state and the JSON. Deleting the download deletes its run.
  - `passwords_order`: add a, b, c, then `order_passwords([c, a, b])`, then the list is in that order. Deleting b leaves c, a.
  - `rules_replace_all`.
  - In `secret.rs`, `dpapi_roundtrip` (`unprotect(protect(b"pw")) == b"pw"`, and the blob ≠ the plain text) and `unprotect_garbage_errs`.
- [ ] **Step 2: Run them and watch them fail.** `cargo test -p haul-engine store:: secret::`: compile errors, then failures.
- [ ] **Step 3: Implement** the migration, the fields and the methods above.
- [ ] **Step 4: Run the tests.** `cargo test --workspace`: all pass.
- [ ] **Step 5: Commit.** `feat(engine): post-processing tables (migration 4) and DPAPI`

---

### Task 2: Rules (pure)

**Files:**
- Create: `crates/engine/src/rules.rs`
- Modify: `crates/engine/src/lib.rs`, `crates/engine/Cargo.toml` (`globset`, `regex`)

**Interfaces:**
- Produces, all serde `rename_all = "camelCase"`:
  - `pub struct Category { pub name: String, pub exts: Vec<String>, pub folder: String }` and `pub fn default_categories() -> Vec<Category>`, with the Global Constraints values.
  - `pub struct RuleMatch { pub host: Option<String>, pub name: Option<String>, pub exts: Vec<String>, pub min_size: Option<u64>, pub max_size: Option<u64> }` (`Default`).
  - `pub struct RuleAction { pub folder: Option<String>, pub rename: Option<String> }`.
  - `pub struct Rule { pub id: i64, pub name: String, pub enabled: bool, #[serde(rename = "match")] pub matcher: RuleMatch, pub action: RuleAction }`.
  - `pub struct Facts<'a> { pub name: &'a str, pub host: &'a str, pub size: Option<u64>, pub package: &'a str, pub date: chrono::NaiveDate }`.
  - `#[derive(Serialize)] pub struct Destination { pub folder: Option<PathBuf>, pub name: String, pub by: String }`:
    - `folder: None` means it stays where it is.
    - `by` is `Rule “<name>”`, the category name, or `""` when nothing matched.
  - `pub fn validate_rule(r: &Rule) -> Result<(), String>`. Messages:
    - `Rule needs a name`.
    - `Bad pattern in “<rule>”: <err>`.
    - `Min size is bigger than max size in “<rule>”`.
    - `Rule “<rule>” has no folder or rename`.
  - `pub fn category_of(name: &str, cats: &[Category]) -> usize`: the first category whose extensions contain the extension. Archive parts go to `Compressed` when it exists. Otherwise the result is the index of the category with empty `exts`, else the last one.
  - `pub fn render(pattern: &str, f: &Facts) -> String`
  - `pub fn decide(f: &Facts, rules: &[Rule], cats: &[Category], sort_categories: bool, base: &Path) -> Destination`:
    - The first enabled rule that validates and matches wins. Its folder resolves against `base` when relative; with no folder it stays put. Its rename applies when set.
    - Otherwise, when `sort_categories` is on, the result is the category folder.
    - Otherwise it stays put.

- [ ] **Step 1: Write the failing tests:**
  - `category_table`: `movie.MKV` → Video, `a.tar.gz` → Compressed, `x.part2.rar` → Compressed, `set.r01` → Compressed, `ubuntu.iso` → Programs, `notes` (no extension) → Other, `song.flac` → Music.
  - `host_patterns`: `*.ubuntu.com` matches `releases.ubuntu.com` and `ubuntu.com`, but not `notubuntu.com`.
  - `glob_and_regex_names`: `*.iso` matches `A.ISO`, and `/^ubuntu-\d+/` matches `ubuntu-24.04.iso`.
  - `size_range_inclusive`.
  - `first_match_wins_and_disabled_skipped`.
  - `render_tokens`: `{package}/{name}-{date}.{ext}` with `name="a b.txt"`, package `P` and date 2026-11-06 gives `P_a b-2026-11-06.txt` (the slash is sanitised).
  - `relative_folder_resolves_against_base`.
  - `no_sort_no_rule_stays`: `folder == None`, the name is unchanged, and `by == ""`.
  - `validate_rejects_bad_regex` (`/(/`) and `validate_rejects_min_over_max`.
  - `broken_stored_rule_is_skipped`: a rule whose regex is invalid comes first, and the second rule wins.
- [ ] **Step 2: Run them and watch them fail.** `cargo test -p haul-engine rules::`
- [ ] **Step 3: Implement.** Use `globset::GlobBuilder::case_insensitive(true).literal_separator(false)` and `regex::RegexBuilder::case_insensitive(true)`. `decide` builds the matchers on each call (rule lists are short).
- [ ] **Step 4: Run the tests.** `cargo test -p haul-engine rules::`: pass.
- [ ] **Step 5: Commit.** `feat(engine): rules and categories`

---

### Task 3: Checksums and the server digest

**Files:**
- Create: `crates/engine/src/postproc/checksum.rs`, and `crates/engine/src/postproc/mod.rs` with only `pub mod checksum;` for now
- Modify: `crates/engine/src/probe.rs` (fill `digest`), `crates/engine/src/lib.rs`, `crates/engine/Cargo.toml` (`sha2`, `md-5`, `sha1`, `crc32fast` and `base64` as normal dependencies)
- Test: unit tests in `checksum.rs`; `crates/engine/tests/probe.rs` (server cases)

**Interfaces:**
- Produces:
  - `#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum Algo { Crc32, Md5, Sha1, Sha256 }`, with `label()` giving `CRC32`, `MD5`, `SHA-1` or `SHA-256`, and `key()` giving `crc32`, `md5`, `sha1` or `sha256`.
  - `pub fn is_checksum_file(name: &str) -> bool`
  - `pub fn parse_checksum_file(file_name: &str, text: &str) -> Vec<(String, Algo, String)>`, as (the listed file name, algo, lowercase hex). It understands:
    - `.sfv` lines `name HEX8`, with comments starting `;`.
    - GNU lines `<hex> [ *]name`.
    - BSD lines `SHA256 (name) = hex`.
    - The algorithm comes from the extension or the `*SUMS` name, else from the hex length.
    - Lines that parse as none of these are ignored.
  - `pub fn parse_pasted(s: &str) -> Result<(Algo, String), String>`, with the error from Global Constraints.
  - `pub fn parse_tagged(s: &str) -> Option<(Algo, String)>` for the stored `"<key>:<hex>"` form.
  - `pub fn digest_from_headers(h: &HeaderMap, full_body: bool) -> Option<String>`, which returns `"<key>:<hex>"`.
  - `pub fn hash_file(path: &Path, algo: Algo) -> std::io::Result<String>`: reads in 1 MiB chunks and returns lowercase hex. It is blocking; callers use `spawn_blocking`.

- [ ] **Step 1: Write the failing tests:**
  - `parses_sfv`.
  - `parses_gnu_and_bsd`: `SHA256SUMS` holding `<64hex> *ubuntu.iso` and `SHA256 (b.bin) = <hex>`.
  - `algo_from_length_in_generic_file`.
  - `ignores_garbage_lines`.
  - `is_checksum_file_table`: `x.sha256` and `sha256sums.txt` are true, while `x.sha256.html` and `SUMS` are false.
  - `pasted_lengths`: the 4 good lengths, the `sha256:` prefix and spaces are accepted, while 10 hex and non-hex are rejected.
  - `hash_file_known_values`: `b"hello haul\n"` against its known CRC32, MD5, SHA-1 and SHA-256 (computed in the test with the same crates, plus one literal SHA-256 checked against `certutil`).
  - `digest_headers`: `Repr-Digest: sha-256=:<b64>:`, `Digest: MD5=<b64>`, and `Content-MD5` accepted only when `full_body`.
  - In `tests/probe.rs`, `probe_reads_repr_digest`: the test server sends the header on HEAD, and `probe().digest == Some("sha256:…")`. The support server gains an option for an extra response header.
- [ ] **Step 2: Run them and watch them fail.** `cargo test -p haul-engine checksum:: --test probe`
- [ ] **Step 3: Implement.** `probe` calls `digest_from_headers(h, true)` for HEAD and for a 200, and `(h, false)` for a 206.
- [ ] **Step 4: Run the tests.** `cargo test --workspace`: pass.
- [ ] **Step 5: Commit.** `feat(engine): checksum files, pasted hashes and server digests`

---

### Task 4: Bundled 7-Zip, fixtures and the extractor

**Files:**
- Create:
  - `app/src-tauri/resources/7zip/7z.exe`, `7z.dll`, `License.txt` and `README.md` (version, source URL, SHA-256 of each file).
  - `crates/engine/src/postproc/extract.rs`.
  - `crates/engine/tests/fixtures/make-fixtures.ps1` and its outputs in `crates/engine/tests/fixtures/archives/`.
  - `crates/engine/tests/extract.rs`.
- Modify: `app/src-tauri/tauri.conf.json` (`bundle.resources`: `{"resources/7zip/*": "7zip/"}`), `.gitattributes` (`*.exe *.dll *.7z *.rar *.zip *.0* binary`)

**Getting 7-Zip** (the owner approved the download): fetch `7zr.exe` and the current stable `7zNNNN-x64.exe` from `https://www.7-zip.org/download.html` into `_tmp/7zip-dl/`. Run `7zr.exe x 7zNNNN-x64.exe -o_tmp/7zip-x64`, then copy `7z.exe`, `7z.dll` and `License.txt`. Record the SHA-256 of each file in the README.

**Fixtures** (`make-fixtures.ps1`, run once with the bundled 7z and `C:\Program Files\WinRAR\Rar.exe`; the outputs are committed, all under 200 KB). The source tree is `hello.txt` (`hello haul\n`), `inner/readme.md` (`# inner\n`) and `data.bin` (64 KiB of incompressible pseudo-random bytes from a fixed seed, so the volumes really split):
- `plain.zip`, and `plain.7z`.
- `split.7z.001`–`.003` (`-v24k`).
- `multi.part1.rar`–`.part3.rar` (`Rar.exe a -v24k -m0`).
- `oldstyle.rar` + `.r00` + `.r01` (`Rar.exe a -v24k -vn -m0`).
- `secret.7z` and `secret.rar`, password `haul-secret` (7z also with `-mhe=on`).
- `damaged.7z`: `plain.7z` with bytes 40–80 overwritten with 0xFF.
- `nested.zip`, which contains `plain.7z`.
- `SHA256SUMS`, covering `plain.zip` and `plain.7z`.

**Interfaces:**
- Produces:
  - `#[derive(Debug, PartialEq)] pub enum ExtractOutcome { Ok, WrongPassword, Failed(String) }`
  - `pub trait Extractor: Send + Sync { fn extract(&self, first_volume: &Path, out: &Path, password: &str) -> ExtractOutcome; }`, which is blocking.
  - `pub struct SevenZip { pub exe: PathBuf }`, which implements `Extractor` with the Global Constraints command.
  - `pub fn first_volumes(names: &[&str]) -> Vec<Vec<usize>>`: the archive sets in a list of names, each in part order, with the first volume first. It reuses `grabber::grouping::archive_part`. A lone `.zip`, `.rar`, `.7z`, `.tar`, `.gz` or `.tgz` counts as a one-volume set. Checksum files are never archives.
  - `pub fn seven_zip_for_tests() -> PathBuf`: `CARGO_MANIFEST_DIR/../../app/src-tauri/resources/7zip/7z.exe` (behind `#[doc(hidden)]`).

- [ ] **Step 1: Get 7-Zip and make the fixtures**, as described above. Check with `app/src-tauri/resources/7zip/7z.exe i`, which exits 0 and prints the version.
- [ ] **Step 2: Write the failing tests** in `tests/extract.rs`, each extracting into a `tempdir`:
  - `extracts_zip_and_7z`: `hello.txt` content matches.
  - `extracts_split_7z`.
  - `extracts_multipart_rar`.
  - `extracts_oldstyle_rar`.
  - `wrong_password_is_told_apart` (`secret.7z` and `secret.rar` with `""` and `"nope"` give `WrongPassword`).
  - `right_password_extracts`.
  - `damaged_is_failed_not_password` (`Failed(_)`).
  - `first_volumes_table` (unit, in `extract.rs`): the fixture names plus `notes.txt` and `SHA256SUMS` give the expected sets.
- [ ] **Step 3: Run them and watch them fail.** `cargo test -p haul-engine --test extract`
- [ ] **Step 4: Implement `SevenZip` and `first_volumes`.**
- [ ] **Step 5: Run the tests.** `cargo test -p haul-engine --test extract`: pass. If a 7-Zip message differs from the constraints (for example on `-mhe` archives), record a `Ruling:` and match what 7-Zip really prints.
- [ ] **Step 6: Commit.** `feat(engine): bundled 7-Zip extractor with fixtures`

---

### Task 5: Moving, the Recycle Bin and the scan

**Files:**
- Create: `crates/engine/src/postproc/sort.rs`, `crates/engine/src/postproc/scan.rs`
- Modify: `crates/engine/src/naming.rs` (`reserve_unique_dir`), `crates/engine/Cargo.toml` (`trash`)

**Interfaces:**
- Produces:
  - `naming::reserve_unique_dir(parent: &Path, name: &str) -> std::io::Result<PathBuf>`: creates `name`, else `name (1)` and so on, atomically (`create_dir` fails if the folder exists).
  - `pub trait Recycler: Send + Sync { fn recycle(&self, paths: &[PathBuf]) -> Result<(), String>; }`, with `pub struct RecycleBin;` (`trash::delete_all`).
  - `pub fn move_item(src: &Path, dest_dir: &Path, name: &str) -> std::io::Result<PathBuf>`:
    - Creates `dest_dir` and reserves a unique name: a placeholder file via `reserve_unique_path`, or a folder via `reserve_unique_dir`.
    - Then `rename`s over the placeholder. For a folder: it removes the empty reserved folder, then renames.
    - On `ErrorKind::CrossesDevices` (or raw OS error 17) it copies recursively, checks the total size, then removes the source.
    - It returns the final path. A source equal to its destination is a no-op.
  - `pub fn move_item_with(src, dest_dir, name, rename: impl Fn(&Path, &Path) -> std::io::Result<()>) -> std::io::Result<PathBuf>`: the testable core.
  - `pub enum ScanOutcome { Clean, Threat, Unavailable }`, `pub trait Scanner: Send + Sync { fn scan(&self, path: &Path) -> ScanOutcome; }` and `pub struct Defender;`:
    - Its path is `%ProgramFiles%\Windows Defender\MpCmdRun.exe`, and a missing file gives `Unavailable`.
    - Exit 0 is `Clean`, exit 2 is `Threat`, and any other exit is `Unavailable`.

- [ ] **Step 1: Write the failing tests** (unit, `tempdir`):
  - `sort_never_overwrites`: the destination already has `a.txt`; the move gives `a (1).txt`, and both contents are intact.
  - `moves_folder_unique`.
  - `cross_device_falls_back_to_copy`: `move_item_with` with a rename that always returns `CrossesDevices`. The tree arrives whole, and the source is gone.
  - `same_place_is_noop`.
  - `reserve_unique_dir_counts`.
  - `defender_missing_is_unavailable`: `Defender` pointed at a missing path through a `#[cfg(test)]` constructor `Defender::at(path)`.
- [ ] **Step 2: Run them and watch them fail.** `cargo test -p haul-engine sort:: scan:: naming::`
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run the tests.** pass.
- [ ] **Step 5: Commit.** `feat(engine): safe moves, recycle bin and Defender scan`

---

### Task 6: The `PostProc` service

**Files:**
- Modify:
  - `crates/engine/src/postproc/mod.rs`.
  - `crates/engine/src/types.rs` (`Event::PostProcChanged { run_id: i64 }`).
  - `crates/engine/src/queue.rs` (`Queue::restart(id)`, which calls `Store::reset_download`, sends `StateChanged Queued` and wakes).
  - `crates/engine/src/lib.rs`.
- Create: `crates/engine/tests/postproc.rs` (uses `tests/support` and the fixtures)

**Interfaces:**
- Consumes: Tasks 1–5.
- Produces, all serde camelCase, with `as_str` matching the constants:
  - `pub enum StepKind { Verify, Extract, Sort, Scan, Notify }`, `pub enum StepState { Pending, Running, Done, Skipped, Failed, NeedsPassword }` and `pub enum RunState { Held, Queued, Running, Waiting, NeedsPassword, Failed, Done }`.
  - `pub struct Step { pub kind: StepKind, pub state: StepState, pub detail: Option<String> }`.
  - `pub struct Run { pub id: i64, pub package_id: Option<PackageId>, pub download_id: Option<DownloadId>, pub state: RunState, pub steps: Vec<Step>, pub updated_at: String }`. `steps_json` stores `{ "steps": [...], "verified": [DownloadId], "extractedTo": Option<String>, "moved": [String] }`, and the private fields stay in the JSON only.
  - `#[derive(Clone, Copy, Serialize, Deserialize)] pub struct PostProcSettings { pub auto: bool, pub sort_categories: bool, pub recycle_archives: bool, pub virus_scan: bool }`, with `Default` set to the Global Constraints default. Settings key `postproc`.
  - `pub struct Notice { pub title: String, pub body: String, pub folder: Option<PathBuf> }`.
  - `pub struct PostProcDeps { pub extractor: Arc<dyn Extractor>, pub recycler: Arc<dyn Recycler>, pub scanner: Arc<dyn Scanner>, pub notify: Arc<dyn Fn(Notice) + Send + Sync> }`.
  - `PostProc::start(store: Store, events: EventBus, queue: Queue, deps: PostProcDeps, download_dir: PathBuf) -> PostProc` (`Clone`). It subscribes before returning. At start it turns runs left `running` into `queued`, and removes any `*.haul-tmp` folder recorded for them.
  - `PostProc::runs(&self) -> Result<Vec<Run>, EngineError>`.
  - `PostProc::process(&self, package_id: Option<PackageId>, download_id: Option<DownloadId>) -> Result<i64, EngineError>`: makes or gets the run, then queues it from its first unfinished step. It errors `Not finished yet` unless every download of the target is completed.
  - `PostProc::retry(&self, run_id)`: valid in `failed`, `held` and `needs_password`.
  - `PostProc::give_password(&self, run_id, password: String, remember: bool)`.
  - `PostProc::set_expected_hash(&self, download_id, hash: Option<&str>) -> Result<(), EngineError>`: validates through `parse_pasted` and stores `"<key>:<hex>"`.
  - `PostProc::settings()` / `PostProc::set_settings(PostProcSettings)`, `PostProc::categories()` / `set_categories(Vec<Category>)` (rejects an empty list or an empty name), and `PostProc::rules()` / `set_rules(Vec<Rule>)` (each rule goes through `validate_rule`, and the first error is returned as `EngineError::Invalid(msg)`; add that variant if it doesn't exist).
  - `PostProc::add_password(&self, pw: &str)` (rejects empty), `PostProc::passwords(&self) -> Result<Vec<i64>, EngineError>` (ids in order), `PostProc::remove_password(id)` and `PostProc::order_passwords(&[i64])`.
  - `pub fn try_rules(facts: &Facts, rules: &[Rule], cats: &[Category], sort: bool, base: &Path) -> Result<Destination, String>`: validates first, then `decide`.

**Run algorithm** (the worker task loops over `queued` runs in id order; each step saves and sends `PostProcChanged`):
1. **Trigger:** on `StateChanged { Completed }`, load the download. The target is its package, or the download itself when it has none.
   - If every download of the target is `Completed`: `ensure_run`, with state `queued` when `settings.auto` is on, else `held`. A run that already exists and is `waiting` goes back to `queued`.
   - On `StateChanged` to `Failed` for a download its `waiting` run is re-downloading, the run fails verify with `Checksum mismatch: <name>`.
2. **Verify:**
   - Expected hashes come from checksum files in the package (parsed from disk), else `expected_hash`, else `server_digest`.
   - For each download not yet in `verified` with an expected hash, hash it with `spawn_blocking`.
   - On a mismatch: if `verify_retries == 0`, then `bump_verify_retries`, recycle the file, `queue.restart(id)`, set the run to `waiting` with the detail `Downloading <name> again`, and stop.
   - On a second mismatch, fail.
   - When no file has a hash, the step is `skipped`.
   - The detail names the source of the first verified file.
3. **Extract:**
   - `first_volumes` over the completed files' names.
   - Output: `reserve_unique_dir(pkg_dir, package_name)`, extracted into `<out>.haul-tmp`, then renamed over it. Record `extractedTo`.
   - Passwords to try: the given one (held in memory only, for one attempt), then empty, then the saved ones.
   - When every set fails with `WrongPassword`, the step and the run become `needs_password`.
   - Then the nested pass and the optional recycle of the volumes.
4. **Sort:**
   - Items: `extractedTo` (placed by its largest file), each non-archive, non-checksum file, then the archives and checksum files.
   - Each destination comes from `rules::decide` with the download's host and `chrono::Local::now().date_naive()`.
   - `move_item`, then `set_completed(id, new_path)` for each file's download row. Record `moved`.
5. **Scan:** `Off` when off, otherwise `scanner.scan` for each `moved` item.
6. **Notify:** call `deps.notify`. The detail is `Sent`. The run becomes `done`, or `failed` when a step failed, and a failed or password-needing run also notifies once (with the "needs attention" title).

- [ ] **Step 1: Write the failing tests** in `tests/postproc.rs`. Use a helper that builds a `Store` in memory, a `Queue` with a test `TransferCtx`, and `PostProc` with `SevenZip(seven_zip_for_tests())`, a fake `Recycler` (records paths and moves them into a `recycled/` dir) and a fake `Scanner`. Downloads are inserted as completed by copying fixtures into a temp download dir and calling `set_completed`, then sending `StateChanged Completed`. Each test waits for `PostProcChanged` until the run is in a final state, or up to 10 s.
  - `split_7z_package_extracts_and_sorts`: the 3 volumes in one package `split`. The extracted folder `split` is placed by its largest file (`data.bin` → Other), so `Other\split\hello.txt` exists, and the volumes are in `Compressed\`. The steps are `skipped, done, done, done(Off), done`, and notify was called once.
  - `checksum_file_verifies`: `plain.zip` + `SHA256SUMS`. Verify is `done` with `SHA-256 from SHA256SUMS`.
  - `mismatch_redownloads_once_then_fails`: a lone download from the test server (`/file` serving known bytes) with `expected_hash` wrong. The first run recycles the file and re-downloads, and the second mismatch gives `failed` with `Checksum mismatch: <name>`. `verify_retries == 1`.
  - `mismatch_then_good_passes`: the server serves bad bytes first, then good ones, and the run ends `done`.
  - `password_needed_then_given`: for `secret.7z`, the state is `needs_password`, then `give_password("haul-secret", true)` gives `done`, with one saved password.
  - `given_wrong_password_stays_needed`: still `needs_password`, with detail `Wrong password` and zero saved passwords.
  - `saved_password_used`.
  - `damaged_archive_fails`: detail starts `Archive is damaged`, and `retry` re-runs from extract.
  - `nested_extracted_one_level`.
  - `auto_off_holds_until_process`.
  - `recycle_archives_when_on`.
  - `rule_beats_category`.
  - `missing_file_fails_step`.
  - `interrupted_extract_restarts_clean`: store a run `running` at extract with an `extractedTo` path whose `.haul-tmp` exists. Start `PostProc` and expect one output folder (no ` (1)`) and no `.haul-tmp`.
  - `lone_download_gets_its_own_run`.
  - `package_with_unfinished_file_waits`: one of 2 completed gives no run.
- [ ] **Step 2: Run them and watch them fail.** `cargo test -p haul-engine --test postproc`
- [ ] **Step 3: Implement** the service, `Queue::restart` and the event.
- [ ] **Step 4: Run the tests.** `cargo test --workspace`: pass, and the earlier queue/transfer tests still pass.
- [ ] **Step 5: Commit.** `feat(engine): post-processing runs (verify, extract, sort, scan, notify)`

---

### Task 7: App commands, the bridge and notifications

**Files:**
- Modify:
  - `app/src-tauri/src/core.rs`: owns `postproc: PostProc`, and `start_full` gains `deps: PostProcDeps`. Real deps come from `AppCore::real_deps(seven_zip: PathBuf, notify)`.
  - `dto.rs`, `bridge.rs`, `commands.rs`, `lib.rs`.
  - `app/src-tauri/Cargo.toml` (`tauri-plugin-notification = "2"`) and `capabilities/default.json` (`notification:default`).
- Test: unit tests in `core.rs` and `bridge.rs`

**Interfaces:**
- Produces Tauri commands, each a thin async wrapper over `AppCore` (store work on `spawn_blocking`, as before):
  - `postproc_runs() -> Vec<Run>`.
  - `postproc_process(package_id: Option<i64>, download_id: Option<i64>) -> i64`.
  - `postproc_retry(run_id)`, `postproc_password(run_id, password, remember)` and `set_expected_hash(id, hash: Option<String>)`.
  - `rules_config() -> RulesConfig`, where `RulesConfig { settings: PostProcSettings, categories: Vec<Category>, rules: Vec<Rule>, passwords: Vec<i64> }` (camelCase).
  - `set_postproc_settings(s)`, `set_categories(c)` and `set_rules(r)`.
  - `try_rules(name: String, host: String, size: Option<u64>, rules: Vec<Rule>, categories: Vec<Category>, sort: bool) -> Destination`. Errors come back as the message string.
  - `add_password(password)`, `remove_password(id)` and `order_passwords(ids)`.
- `Update` gains `postproc: bool`, set by `Event::PostProcChanged`.
- `lib.rs`:
  - 7-Zip lives at `app.path().resource_dir()?.join("7zip/7z.exe")`. In debug builds, when that path doesn't exist, it falls back to `CARGO_MANIFEST_DIR/resources/7zip/7z.exe`. When neither exists, it logs a warning and uses that path anyway, and extraction fails with the 7-Zip message.
  - Notifications go through `tauri_plugin_notification::NotificationExt::notification().builder().title(..).body(..).show()`.
- [ ] **Step 1: Write the failing tests:**
  - `batcher_flags_postproc` (bridge).
  - `rules_config_defaults`: the default settings, the 6 categories, no rules and no passwords.
  - `set_rules_rejects_bad_regex_and_keeps_old`.
  - `try_rules_reports_rule`.
  - `password_roundtrip_hides_plaintext`: `rules_config().passwords.len() == 1`, and the JSON of `rules_config` does not contain the password.
  - `postproc_process_requires_finished`.
- [ ] **Step 2: Run them and watch them fail.** `cargo test -p haul-app`
- [ ] **Step 3: Implement**, and register the plugin and the commands.
- [ ] **Step 4: Run the tests.** `cargo test --workspace` and `cargo clippy --workspace --all-targets -- -D warnings`: clean.
- [ ] **Step 5: Commit.** `feat(app): post-processing and rules commands, notifications`

---

### Task 8: UI state, mock and drawer

**Files:**
- Create: `app/src/lib/postproc.ts` (+ `.test.ts`), `app/src/lib/postproc-state.svelte.ts` (+ `.test.ts`), `app/src/lib/backend-mock-postproc.ts`, `app/src/components/AfterDownload.svelte`
- Modify: `types.ts`, `backend.ts`, `backend-tauri.ts`, `backend-mock.ts` (wire in the postproc mock), `App.svelte`, `Drawer.svelte`, `ListRow.svelte`/`StatusCell.svelte` (package status word), `Icon.svelte` (an `ext` icon from the mockup)

**Interfaces:**
- TS mirrors in `types.ts`: `StepKind`, `StepState`, `RunState`, `Step`, `Run`, `PostProcSettings`, `Category`, `RuleMatch`, `RuleAction`, `Rule`, `RulesConfig`, `Destination`, and `Update.postproc: boolean`.
- `Backend` gains `postprocRuns`, `postprocProcess(t: { packageId?: number; downloadId?: number })`, `postprocRetry`, `postprocPassword(runId, password, remember)`, `setExpectedHash(id, hash: string | null)`, `rulesConfig`, `setPostprocSettings`, `setCategories`, `setRules`, `tryRules(facts: { name: string; host: string; size: number | null }, draft: { rules: Rule[]; categories: Category[]; sort: boolean })`, `addPassword`, `removePassword` and `orderPasswords`.
- `postproc.ts` (pure):
  - `STEP_TITLES: Record<StepKind, string>`.
  - `packageWord(run: Run | null): string | null`, which uses the Global Constraints row words. `running` gives the running step's word, and `done` and `null` give `null`.
  - `stepIcon(s: StepState): IconName`: `done` → `ok`, `failed` → `err`, `running` → `ext`, `needs_password` → `err`, `skipped` → `ok`, `pending` → `wait`.
  - `canProcess(downloads: DownloadView[], run: Run | null): boolean`.
- `postproc-state.svelte.ts`: `createPostprocState(backend, { showToast })` with `runs`, `byPackage(id)`, `byDownload(id)`, `load()` (newest load wins, like the collector), `onUpdate(u)` (`u.postproc` reloads), and actions `process`, `retry`, `password` and `setHash`, which show the toasts.
- Mock (`backend-mock-postproc.ts`): when every download of a package (or a lone download) becomes `completed` in the mock, it creates a run and moves it one step per tick: verify, then extract, then sort, then scan (`Off`), then notify.
  - A package whose name contains `secret` stops at `needs_password` until it gets the password `haul`.
  - A name containing `damaged` fails extract.
  - The rules config is held in memory, and `tryRules` does extension categories and a simple `*.ext` glob.
  - It flags `postproc: true` on its next update.
- [ ] **Step 1: Write the failing tests** (Vitest):
  - `postproc.test.ts`: `packageWord` for every state, `stepIcon`, and `canProcess`.
  - `postproc-state.svelte.test.ts`: `load_on_postproc_update`, `newest_load_wins`, and `password_wrong_toast_free`. A wrong password shows no toast: the step detail shows it.
  - `backend-mock.test.ts`: `mock_package_runs_steps_in_order` and `mock_secret_needs_password`.
- [ ] **Step 2: Run them and watch them fail.** `npm test`
- [ ] **Step 3: Implement** the libs, the mock, and `AfterDownload.svelte`:
  - It shows an ordered list of the 5 steps (icon, title and detail; `aria-live="polite"` on the list).
  - It has `Process` when `canProcess`, `Retry` when failed, and a password form when `needs_password` (`type="password"`, the `Remember this password` tick checked, `Use password`).
  - It goes in the drawer for a package and for a lone file, under the segment map.
  - The file drawer also gets the `Expected checksum` form.
  - Package rows show `packageWord` in the Status column with the `ext` icon.
- [ ] **Step 4: Run the tests.** `npm test` and `npm run check`: pass.
- [ ] **Step 5: Commit.** `feat(ui): after-download steps in the drawer`

---

### Task 9: Rules view

**Files:**
- Create: `app/src/lib/rules.ts` (+ `.test.ts`), `app/src/lib/rules-state.svelte.ts`, `app/src/components/RulesView.svelte`, `RuleEditor.svelte`, `CategoryTable.svelte`, `PasswordList.svelte`, `app/e2e/postproc.spec.ts`, `app/e2e/rules.spec.ts`
- Modify: `App.svelte` (route `rules` to `RulesView`), `app/e2e/a11y.spec.ts` (Rules view in both themes; drawer with steps)

**Interfaces:**
- `rules.ts` (pure):
  - `parseExts(s: string): string[]`: splits on commas and spaces, lowercases, strips dots and drops duplicates.
  - `mbToBytes(s: string): number | null` (an empty string gives `null`, MB = 1,000,000 bytes) and `bytesToMb(n: number | null): string`.
  - `blankRule(): Rule`, `moveItem<T>(xs: T[], i: number, by: -1 | 1): T[]`, and `ruleSummary(r: Rule): string`, for example `*.iso · releases.ubuntu.com → Programs\ISO`.
- `rules-state.svelte.ts`: loads `rulesConfig`, holds a draft for the open editor, and saves through the backend, showing `Rules saved` or the rejection message as a toast.
- `RulesView`, one scrolling page in four sections:
  - The four switches (the existing `.tog` style), saved on click.
  - The categories table, with editable `exts` and `folder` per row and a Save.
  - Your rules: the ordered list (an enabled switch, the summary, `Move up` / `Move down` / `Delete`), a row click opening `RuleEditor` inline, and `Add rule`.
  - Saved passwords, as masked rows with `Move up` / `Move down` / `Delete` and an `Add password` form.
  - The `Try a file name` box (name, optional host and size) sits under Your rules. It calls `tryRules` with the current draft, 200 ms after typing stops.
- [ ] **Step 1: Write the failing tests:**
  - Vitest `rules.test.ts`: `parseExts`, the MB round trip, `moveItem` at the edges, and `ruleSummary`.
  - e2e `postproc.spec.ts`:
    - `drawer_shows_steps_after_finish`: add a link in the mock, wait for completion, select its row, and see the steps reach `Sort` done.
    - `password_prompt_wrong_then_right` (a package named `secret`).
    - `damaged_shows_retry`.
    - `expected_checksum_saves_and_rejects` (10 hex is rejected with the message).
  - e2e `rules.spec.ts`:
    - `switches_persist_across_reload` (the mock keeps state per page load, so check the call log through `__haulMockCalls`).
    - `add_rule_and_try_name`.
    - `rule_editor_rejects_bad_regex`.
    - `reorder_rules_with_buttons`.
    - `add_and_remove_password_masked` (the page text never contains the password).
  - a11y: the Rules view and the drawer's steps, with axe clean in both themes.
- [ ] **Step 2: Run them and watch them fail.** `npm test` and `npm run e2e`
- [ ] **Step 3: Implement.** Follow `DESIGN.md`:
  - Sections are `.sec`-like blocks with the mono uppercase `h3`.
  - Inputs use the drawer's `select` styling.
  - Every control has a visible label or an `aria-label`.
  - Everything works from the keyboard.
- [ ] **Step 4: Run the tests.** `npm test`, `npm run check` and `npm run e2e`: all pass in light and dark.
- [ ] **Step 5: Commit.** `feat(ui): Rules view`

---

### Task 10: Real-app smoke and handoff

**Files:**
- Modify: `app/scripts/range-server.mjs` (serve `/fixtures/<name>` from `crates/engine/tests/fixtures/archives/`), `app/e2e-real/smoke.spec.ts`, `handoff.md`

- [ ] **Step 1: Write the smoke test** `split_7z_extracts_and_sorts`:
  - Paste the 3 `split.7z.00N` fixture URLs, then `Start package`.
  - Wait up to 60 s for the package's drawer to show `Sort` done.
  - Assert, through Node `fs`, that `<download_dir>\Other\split\hello.txt` exists and reads `hello haul\n`.
- [ ] **Step 2: Run it against the real app** (three shells, as in `handoff.md`, with `download_dir` pointed at `_tmp/smoke`). It fails until the server route exists. Then add the route and run all 4 smoke tests to green. Check that a Windows notification appeared, by eye or in the log line `notification sent`.
- [ ] **Step 3: Full checks:**
  - `cargo test --workspace`.
  - `cargo clippy --workspace --all-targets -- -D warnings`.
  - `cargo fmt --all -- --check`.
  - In `app/`: `npm test`, `npm run check` and `npm run e2e`.
- [ ] **Step 4: Update `handoff.md`:**
  - Where things stand.
  - A "Plan 4" section covering the flow, where the code is, the settings keys, the rulings from the ledger, and the bugs found.
  - Test counts.
  - Next: plan 5.
- [ ] **Step 5: Commit.** `docs: handoff after plan 4`
