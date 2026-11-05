# Haul: handoff

Last updated: 5 Nov 2026. Plan 1 (engine core) is merged. Plan 2 (Tauri shell + Queue UI) is built on `feat/shell-queue-ui`: all 10 tasks are done and reviewed, the review's fixes are in, and every check is green. Only the merge into `master` is left.

## What Haul is
A free, open-source Windows download manager. It pairs IDM's fast engine and browser capture with JDownloader 2's link collector, packages and auto-extraction, in a calm "dispatch board" interface.
- `PRODUCT.md`: product principles.
- `DESIGN.md`: design tokens and rules.
- `docs/mockup/index.html`: the clickable mockup.
- `docs/superpowers/specs/2026-11-05-haul-design.md`: the spec, the source of truth.

## Where things stand
| | |
|---|---|
| Branch | `feat/shell-queue-ui`, cut from `master` at `34e6ab9`. Not merged yet. No Git remote exists. |
| Done | Plan 1 (engine core) and plan 2 (`docs/superpowers/plans/2026-11-05-haul-plan-2-shell-queue-ui.md`, Tasks 1–10). |
| Tests | Rust: 113 tests (engine + app). Front end: 36 Vitest unit tests and 28 Playwright e2e tests, run in both light and dark (56 runs), plus 2 real-app smoke tests. Clippy, rustfmt and svelte-check are clean. |
| Real-app check | Smoke test against the real app (`npm run smoke`): add, pause at 30%+, resume and complete; the SHA-256 matches. Closing the window mid-download and relaunching showed the row Paused, and Space resumed it with ranged requests starting mid-range, not from 0. |
| Next | 1. Merge `feat/shell-queue-ui` into `master`. 2. Plan 3: the Collector and clipboard watching (spec section 5). |

## The app, and how to run it
- **Real app:** `cd app` and `npx tauri dev`. The window is titled Haul, 1280×800 by default.
  - The database is `%APPDATA%\dev.haul.desktop\haul.db` and the logs are in `%LOCALAPPDATA%\dev.haul.desktop\logs\`.
  - Closing the window pauses and saves every download, then quits.
- **Browser mock, no Rust needed:** `cd app`, then `npm run dev:mock`, then open `http://localhost:5174/?demo=1`.
  - `?demo=1` seeds the mockup's rows. Without it you get the empty state.
  - The mock simulates the engine: a new link takes about 6 s, and a URL containing `404` or `expired` fails the matching way.
- **Front-end checks, run in `app/`:** `npm test` (Vitest), `npm run check` (svelte-check) and `npm run e2e` (Playwright on the installed Edge).
- **Real-app smoke test.** Use three shells:
  1. `node scripts/range-server.mjs 8765`
  2. `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222 npx tauri dev`
  3. `npm run smoke` (two tests: a full download with a pause and a SHA-256 check, and removing a running download)
  - The smoke files go to the app's save folder. Point `download_dir` in the settings table at a scratch folder first, or they land in your Downloads.
- **Layout of `app/`:**
  - `src-tauri/`: the `haul-app` crate.
    - `core.rs`: the commands as plain methods.
    - `dto.rs`: what the UI sees.
    - `bridge.rs`: batches engine events into one `haul:update` every 250 ms and asks for a resync when it falls behind.
    - `commands.rs` and `lib.rs`: Tauri glue.
  - `src/lib/`: pure TypeScript, unit-tested: format, status, strip, model (list building, updates, moves and drops), speeds, keys and sparkline. It also holds `backend*.ts` (the interface, Tauri and the mock) and `app-state.svelte.ts` (all Queue state).
  - `src/components/`: the Svelte views.

## Plan 2 rulings (decisions made on the owner's behalf)
Each line says what it costs if the decision is wrong.
- **Contrast** (from the plan): `--ink-3` is `#6B6559` light and `#8F8B82` dark. Status words are `--ink-2`, with the colour on the icon. `DESIGN.md` is updated. The a11y test fails on the old colours. Cost: labels a little darker than the mockup.
- **Pasted links queue directly** until plan 3 adds the Collector. Two or more links make a `Pasted list · d Mon HH:MM` package. Cost: no duplicate or online check yet.
- **TypeScript is pinned to 5.** npm picked 7.0, the native port, which svelte-check can't use yet. Cost: an upgrade later.
- **Columns are narrower than the mockup's.** The mockup's minimum widths add up to 870px, but the list area is only 732px at the default window size, so Status was cut off. Below about 1180px the list scrolls sideways inside itself. Cost: a narrower name column.
- **Sparkline:** the plan's formula contradicted its own test, so the test won: the peak sits 10% below the top. Cost: none.
- **Reorder rules** (`planMove`/`planDrop` in `model.ts`):
  - A package moves as a block, and a file inside a package moves among its siblings.
  - A dropped row takes the target's place.
  - A moved row takes its new neighbour's priority.
  - A file dragged out of a package stays in its package.
  - Cost: a drag may land one row off what someone expects.
- **Remove:** Remove waits 6 s for Undo. A removal still pending when the app closes is never sent, so the row survives. Cost: the user deletes it again.
- **Native drag-and-drop is off** (`dragDropEnabled: false`) so HTML5 drops of links and `.txt` files reach the page. Cost: none until something needs native file drops.
- **Fixes from the smoke run, each test-first:**
  - Finished rows now reload a snapshot, so they learn their saved path.
  - Finished rows show the name on disk, e.g. `file (3).bin`.
- **Smoke server throttle:** 1 MB/s per response, so a test can pause at 30%. Cost: none.
- **Smaller calls:**
  - The snapshot hides packages that have no downloads left.
  - The demo seed adds a running `.msi` row.
  - The speed-limit label reads "10 MB/s".
  - Speed history gets one sample per update.
  - The drawer has a Remove button.
  - The full list with reasons is in `.superpowers/sdd/2026-11-05-haul-plan-2-shell-queue-ui/progress.md` (git-ignored).

## Plan 2 review (5 Nov 2026)
A fresh reviewer read the whole branch and found no Critical issues and 4 Important ones. Each fix has a test that failed first.
- **Removing a running download left a ghost "Paused" row.** The engine now sends `Event::Removed` after the delete, and the UI keeps the row hidden until a snapshot drops it.
- **Updates that arrived during a snapshot load were lost.** Only the newest load now applies, the updates seen during a load are replayed onto it, and resync requests are coalesced.
- **Commands ran on the main thread.** They are now async and do their store work on a blocking thread. A snapshot also loads all segments in one query.
- **Markdown `[url](url)` and comma-joined links** produced bogus downloads. `extract_links` now splits them.

Deferred minors, each a small follow-up:
- **Error handling:**
  - Errors in remove, reorder and add get no toast or rollback, and the same goes for a failed speed-limit change.
  - A startup failure panics with no dialog.
- **Small display and state bugs:**
  - Rows waiting in the Undo window still count toward the total speed.
  - `lastSteal` is never pruned.
  - A queued file whose size is known to be 0 bytes shows 100%.
- **Capability:** it grants `opener:default`, which the UI doesn't use; drop it along with the JS package.
- **Data folder:** dev and release builds share the `dev.haul.desktop` data folder.

## How to build and test
The toolchain is already installed on this PC:
- Rust stable (MSVC).
- VS 2022 Build Tools with the C++ workload and the Windows 11 SDK 10.0.26100.

New shells may not have cargo on PATH:
```bash
export PATH="$HOME/.cargo/bin:$PATH"
```
Run the Rust tests for the engine and the app (about 20 s):
```bash
cargo test --workspace
```
Run clippy:
```bash
cargo clippy --workspace --all-targets -- -D warnings
```
Download a real file with the CLI example:
```bash
cargo run -p haul-engine --release --example haul_get -- https://sin-speed.hetzner.com/100MB.bin --out _tmp/dl
```
- The CLI's flags are `--conns N`, `--out DIR` and `--limit BYTES_PER_SEC`.
- Rerunning the same command resumes, using `<out>/.haul-cli.db`.
- Put scratch output in `_tmp/` (git-ignored).

## The engine, in one page (`crates/engine`)
| File | Job |
|---|---|
| `types.rs`, `error.rs`, `events.rs` | Ids, `DownloadState`, `Priority`, `ProbeInfo` and `Event`; `EngineError` with `is_retryable()` and the status mapping; and `EventBus` (a broadcast with capacity 1024). |
| `naming.rs` | Safe Windows names. Name sources: `filename*` first, then `filename`, then the URL's last path segment. `reserve_unique_path` atomically picks `name (n).ext`. |
| `segmenter.rs` | Pure split-and-steal maths: `initial_plan` and `pick_steal`. |
| `store/` | SQLite (WAL) with `schema.sql` as migration 1. Downloads, segments, packages and settings. |
| `probe.rs` | `http_client()` and `probe()`: HEAD, then a fallback GET with `Range: bytes=0-0`. |
| `ratelimit.rs` | `RateLimiter` (a token bucket with burst = rate / 4) and `HostLimiter` (at most 16 connections per host). |
| `transfer/` | `run_transfer`: probe, resume or plan, run the connections, save every 2 s, finish. See the details below. |
| `queue.rs` | `Queue`: priority order, a parallel limit of 1–20, pause/resume/remove, crash recovery on start, `AllDone`, and the schedule. |
| `schedule.rs` | `Schedule` (overnight windows count from their start day), `is_within_window`, and `WhenDone`, which is stored only. |
| `examples/haul_get.rs` | The CLI used for real-world checks. |
| `tests/support/mod.rs` | A local axum server that can throttle, drop, ignore ranges, force statuses, send 429s, hide the length and redirect. |

How `transfer/` works:
- **`state.rs`**: the shared range table.
- **`conn.rs`**: one connection at a time. It claims a range or steals half of the biggest one, GETs it, buffers 256 KiB, writes with `seek_write`, and only then advances `pos`, which makes it crash-safe.
- **Retries:** a connection retries with backoff. On a 429 it sheds itself while other connections remain, and the last one retries.
- **`mod.rs`**: runs the passes and the slow-connection monitor.
- **Fallbacks:** if the server ignores ranges, it falls back to one sequential connection. A 416 restarts once.
- **Resume:** before resuming it checks the fingerprint (ETag, then Last-Modified, then size).

## Plan 1 rulings (engine)
Each line says what it costs if the decision is wrong.
- **reqwest TLS:** uses `rustls-no-provider` plus `rustls` with **ring**, not the default aws-lc, so the build needs no cmake or NASM. Cost: none; switching back is a feature flag.
- **Part file name:** `<name>.<id>.haulpart`, not the spec's `<name>.haulpart`, so two same-name downloads can't collide. Cost: none.
- **Engine location:** the engine lives in `crates/engine`, not inside `app/src-tauri`. Cost: none; plan 2 adds the Tauri app as a second workspace member.
- **Unique names:** `reserve_unique_path` creates an empty placeholder, so two same-name downloads finishing together never pick the same name. Cost: one extra helper.
- **Range alignment:** ranges are rounded down to 64 KiB multiples. The plan's own tests needed it. Cost: the last range can be slightly larger.
- **Probe fallback:** the probe falls back to the ranged GET on *any* HEAD failure or missing Accept-Ranges, because signed S3-style links reject HEAD. Cost: one extra request per probe.
- **Pinned file name:** the first probe's file name is stored (`Store::set_file_name`), so later probes can't orphan the part file. Cost: none.
- **Retry timing in TDD:** per-connection retry was built in Task 6, ahead of Task 7, so Task 7's retry tests never failed first. Cost: weaker evidence for those tests only.
- **Test tuning:** the slow, steal, expiry and 416 tests were tuned so they actually exercise the behaviour and finish in seconds, not 8+ s. Cost: none.
- **Range-ignored memory:** after a range-ignored fallback, the stored probe still says resumable, so the next resume detects it again. Cost: one wasted request.
- **Real-world test file:** the plan's `proof.ovh.net/files/1Gb.dat` is really 1 GiB, and that host rate-limits with 429 and then blocks the IP. Hetzner's Singapore 100 MB file is used instead. Cost: none.
- **New fix, 429 handling:** a 429 is retryable and sheds extra connections. The real download found this bug, and the tests went RED then GREEN. Cost: hosts that use 429 as a hard limit get retried up to 10 times.
- **Kill-and-resume check:** it was done with a hard kill, not Ctrl+C, because Git Bash can't send Ctrl+C to a native exe. Cost: the CLI's Ctrl+C path hasn't been tried by hand.
- **Formatting:** added `rustfmt.toml` (max width 120). Cost: none.

## Known minors, deferred (plan 1)
- **`AllDone` fires too often:** it also fires when the user pauses every download, so plan 6 ("when done") must check states before acting on it.
- **Stalls under a speed limit:** the stall rule (no bytes for `slow_window` → reconnect) is off while a speed limit is set, so a stall there still waits for the 30 s read timeout.
- **Crash during finish:** a crash between the final rename and `set_completed` re-downloads the file as `name (1)`. Fix: store `target_path` before the rename and check it on recovery.
- **Schedule vs. manual pause:** when a schedule window opens, it re-queues every download it auto-paused, even one the user has since decided to keep paused. This needs an owner decision. It was raised with them and is still open; settle it with the schedule settings in plan 6.

## Plan 1 post-review fixes (5 Nov 2026)
`/code-review` (high) ran on the branch. Fixed, each with a test that failed first except the 429 race:
- A changed file could resume onto stale bytes after an early failure.
- Resume used too few connections.
- The free-space check ignored the already-sized part file.
- Disk full is now a single Paused state.
- Pause/remove could lose a race with the dispatcher (`Store::start_if_queued`).
- A 429 shed could orphan a range.
- The stall rule.
- `.gitattributes`.
- The unused `unique_path` was removed.

Details and rulings are in `.superpowers/sdd/2026-11-05-haul-plan-1-engine-core/progress.md`. There are now 87 tests.

## Working agreements (read before continuing)
- **Write the code yourself.** Don't delegate to the codex/deepseek/opencode workers, even though `C:\Users\User\.claude\deepseek-on` exists. The owner asked for this explicitly.
- **Workflow:** superpowers, in the order spec → writing-plans → executing-plans with TDD. Watch each test fail before writing the code.
  - The ledger lives in `.superpowers/sdd/<plan>/progress.md` (git-ignored). Record each deviation in it as a `Ruling:` line.
- **The owner is new to coding.** Explain choices plainly, make reasonable calls, and flag only real decisions.
- **Never destroy user data.** Deletion goes to the Recycle Bin. Ask before installing software or doing anything destructive.
- **Folder:** the project is `C:\tools\claude for project`. The separate `C:\tools\claude` folder is an unrelated Roblox project.
- **Scope:** Haul is a general-purpose download manager. Don't build anything aimed at piracy sites or their file hosts.

## Environment notes
- **Bash heredocs:** they mangle quotes and backslashes on this machine. Write files with the Write tool, or put edit scripts in the scratchpad and run them with `python -I`.
- **Mockup preview:** `.claude/launch.json` has a python `http.server` config on port 5178.
