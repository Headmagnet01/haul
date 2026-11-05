# Haul: handoff

Last updated: 5 Nov 2026. Plan 1 (engine core) is done, reviewed and merged. Plan 2 is written but not started.

## What Haul is
A free, open-source Windows download manager. It pairs IDM's fast engine and browser capture with JDownloader 2's link collector, packages and auto-extraction, in a calm "dispatch board" interface.
- `PRODUCT.md`: product principles.
- `DESIGN.md`: design tokens and rules.
- `docs/mockup/index.html`: the clickable mockup.
- `docs/superpowers/specs/2026-11-05-haul-design.md`: the spec, the source of truth.

## Where things stand
| | |
|---|---|
| Branch | `master`. Plan 1 (`feat/engine-core`) is merged, including the post-review fixes. No Git remote exists. |
| Done | Plan 1, the engine core: `docs/superpowers/plans/2026-11-05-haul-plan-1-engine-core.md`, Tasks 0–10. |
| Tests | 87 tests, green 6 runs in a row after the review fixes. `cargo clippy -- -D warnings` is clean. |
| Real-world check | 100 MB from `sin-speed.hetzner.com`: correct SHA-256, resumed after a hard kill, never overwrote an existing file. |
| Next | Execute plan 2 (Tauri shell + Queue UI): `docs/superpowers/plans/2026-11-05-haul-plan-2-shell-queue-ui.md`. Two owner answers come first; see "Plan 2: before you start". |

## Plan 2: before you start
**The plan:** 10 test-first tasks, from spec **section 7** (user interface) and milestone 3, plus `DESIGN.md` and the mockup.
- The old note here said "sections 5–6"; that was wrong. Sections 5–6 are the Collector (plan 3) and the extension (plan 5).
- Execution branches off `master`, e.g. `feat/shell-queue-ui`.

**Waiting on the owner (ask before Task 2):**
1. **Install approval.**
   - Task 2 installs, all inside `app/` and the Cargo cache: npm packages (Tauri CLI and API, Svelte 5, Vite, TypeScript, Vitest, Playwright, axe-core, the IBM Plex fonts; a few hundred MB in `node_modules`) and the Tauri Rust crates.
   - Playwright uses the installed Edge (`channel: 'msedge'`), so it downloads no browsers.
2. **Execution method.**
   - Native: the session builds every task, then one fresh whole-branch review. This was recommended, since the tasks share interfaces tightly.
   - Subagent-driven: a fresh implementer and reviewer per task.
   - Either way, the "write the code yourself, no worker delegation" agreement below still holds.

**Toolchain already on this PC** (checked 5 Nov 2026):
- Node 24.22, npm 11.2 and bun 1.4.
- No global Tauri CLI; the plan uses `npx tauri` from the dev dependency.
- WebView2 and Edge come with Windows 11.

**Plan 2 rulings made while writing it** (full list at the end of the plan):
- **AA contrast fix.**
  - `--ink-3` becomes `#6B6559` light and `#8F8B82` dark; the old values are 2.8:1 and 3.3:1.
  - Status words use `--ink-2`, with the colour on the icon, because orange or green text fails AA in light mode.
  - Task 2 updates `DESIGN.md` to match.
- **Pasted links queue directly** until plan 3 adds the Collector. Several links become one package named `Pasted list · <d Mon HH:MM>`.
- **Working controls only:** no clipboard toggle (plan 3) and no "When done" or tray (plan 6). Closing the window quits cleanly.
- **Speeds are per range:** the drawer's per-connection speeds are measured per range in the UI.
- **Engine additions (Task 1):** `extract_links`, `Store::reorder` and `Queue::reorder`, `Store::list_packages`, and `Download.created_at`.

## How to build and test
The toolchain is already installed on this PC:
- Rust stable (MSVC).
- VS 2022 Build Tools with the C++ workload and the Windows 11 SDK 10.0.26100.

New shells may not have cargo on PATH:
```bash
export PATH="$HOME/.cargo/bin:$PATH"
```
Run the tests (about 15 s):
```bash
cargo test -p haul-engine
```
Run clippy:
```bash
cargo clippy -p haul-engine --all-targets -- -D warnings
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

## Decisions made on the owner's behalf (rulings)
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

## Known minors, deferred
- **`AllDone` fires too often:** it also fires when the user pauses every download, so plan 6 ("when done") must check states before acting on it.
- **Stalls under a speed limit:** the stall rule (no bytes for `slow_window` → reconnect) is off while a speed limit is set, so a stall there still waits for the 30 s read timeout.
- **Crash during finish:** a crash between the final rename and `set_completed` re-downloads the file as `name (1)`. Fix: store `target_path` before the rename and check it on recovery.
- **Schedule vs. manual pause:** when a schedule window opens, it re-queues every download it auto-paused, even one the user has since decided to keep paused. This needs an owner decision. It was raised with them and is still open; settle it with the schedule settings in plan 6.

## Post-review fixes (5 Nov 2026)
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
