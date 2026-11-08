# Haul plan 5: local API, pairing and the browser extension

Design agreed with the owner on 8 Nov 2026. It refines section 6 of `2026-11-05-haul-design.md`; where the two differ, this file wins.

## 1. Goal and scope

Downloads you click in the browser go to Haul and start at once, and you can send a page's links to the Collector.

- **Browsers:** Chrome and Edge, from one Manifest V3 build. Firefox is deferred.
- **Install:** the extension is loaded unpacked ("developer mode"). Store publishing belongs to the packaging plan.
- **A clicked download starts right away** (owner decision). Links sent with "Download all links…" go to the Collector.
- **Success:**
  - A clicked download of 1 MB or more lands in Haul's Queue and completes.
  - Downloads that need a login work, because the site's cookies travel with them.
  - If Haul is closed, not paired or slow, the browser keeps the download. A download is never lost.
- **Out of scope:** Firefox; cookies for "Download all links…"; live progress pushed to the popup; a Settings page (the Browsers list lives in the Rules view until plan 6).

## 2. Approach

Haul runs a small HTTP server on `127.0.0.1` (approach A), built with `axum`. Native messaging (needs registry entries per browser and a relay program) and a WebSocket (Chrome suspends extension workers after ~30 s idle) were rejected.

## 3. Components

### 3.1 Engine: request headers (`crates/engine`)
- `downloads.headers_json` exists but is never sent. It becomes a DPAPI-protected blob (`secret::protect`) holding `{cookie, referer, user_agent}`.
  - Migration 5 adds `downloads.headers_blob BLOB` and drops use of `headers_json`. Plain-text headers are never stored.
- The probe, every connection and every resume send these headers. The stored user agent replaces `Haul/x.y` for that download.
- Headers are deleted (set to NULL) when a download completes, is removed, or fails for good. A paused download keeps them.
- 401 maps to `EngineError::LinkExpired`, joining 403 and 410, so an expired login shows as Link expired.
- `Store::refresh_headers(id, blob)` replaces a download's headers. Used by re-capture (section 4.4).

### 3.2 The front desk (`app/src-tauri/src/api/`)
- `axum` server bound to `127.0.0.1`, port 17654. If that port is busy it tries each port up to 17664, and fails with a logged error (the app still runs) if none is free.
- The chosen port is written to `api-port` in the app data folder.
- Routes:
  - `POST /v1/status` → `{ app: "haul", version, paired, active }`. `paired` says whether this caller's key is valid. It needs no key (so the extension can find Haul), but still passes the Host and Origin checks. It is a POST because Chrome sends no `Origin` header on an extension's GET to a host it has permission for (found in the real-browser test).
  - `POST /v1/pair` → `{ browser }` in, `{ token }` out on Allow.
  - `POST /v1/capture` → `{ url, filename?, size?, referrer?, user_agent?, cookies? }` in, `{ id }` out. Queues one download, a lone download with normal priority, in Haul's save folder, and starts it.
  - `POST /v1/links` → `{ links: [...], page_title? }` in, `{ added, skipped }` out. Calls `add_links` with source `browser`.
- **The three locks**, one middleware layer in front of every route:
  1. `Host` must be exactly `127.0.0.1:<port>`. This blocks DNS rebinding.
  2. `Origin` must be present and start with `chrome-extension://`. For every route but `/v1/pair` and `/v1/status`, it must equal the origin bound to the key.
  3. `Authorization: Bearer <token>`, on every route but `/v1/pair` and `/v1/status`. The SHA-256 of the token must match a row in `paired_clients`.
- No CORS headers are ever sent.
- Bodies are capped at 2 MB. `/v1/links` takes at most 5,000 links.
- The lock rules are pure functions (`check_host`, `check_origin`, `check_token`) with their own unit tests. Routes call `AppCore` methods, the same ones the Tauri commands use.
- The server stops when the app shuts down. In-flight requests get `503`.

### 3.3 Pairing (app and window)
- Migration 5 adds `paired_clients (id, browser, origin, token_hash, created_at, last_seen)`.
- `/v1/pair` flow:
  1. If a pairing request is already waiting, answer `429` at once.
  2. Otherwise bring Haul's window to the front and emit `haul:pair-request { browser }`.
  3. The window shows a dialog, "Allow <browser> to send downloads to Haul?", with Allow and Deny.
  4. The request waits up to 60 s. Deny, closing the dialog or the timeout all answer `403`.
  5. On Allow, make 32 random bytes and return them as hex. Store only their SHA-256 and the caller's origin.
- New commands: `pair_answer(allow)`, `paired_clients()`, `remove_paired_client(id)`.
- The Rules view gains a **Browsers** section: each paired browser's name, when it was paired, when it was last seen, and Remove. Removing a browser invalidates its key at once.

### 3.4 The extension (`extension/`)
TypeScript, built with Vite into `extension/dist/`. It asks for these permissions: `downloads`, `contextMenus`, `cookies`, `storage`, `activeTab`, `scripting`, plus host permissions `<all_urls>` (for cookies) and `http://127.0.0.1/*`.
- **Background service worker:** download capture, the context menu, and talking to Haul (port finder, key, requests with a 1.5 s timeout).
- **Content script:** records Alt+click on links, sending the link's URL and a timestamp to the worker, so "Alt lets the browser handle it" works.
- **Link picker page:** "Download all links…" opens it with the page's links, or only those inside the current selection. Type filters: archives, video, audio, images, documents, programs. It sends the checked links to `/v1/links`.
- **Popup:** paired or not, a Pair button, the result of the last capture (for example "Haul didn't answer"), and the active count from `/v1/status`.
- **Options page:** minimum size (default 1 MB) and the file-type list (default: zip, rar, 7z, exe, msi, iso, mp4, mkv, mp3, flac, pdf).
- Pure modules (`decide.ts`, `port.ts`, `handover.ts`, `picker.ts`) hold all logic and take the browser and network as parameters, so Vitest can test them.

## 4. Flows

### 4.1 A clicked download
1. `downloads.onCreated` fires. The worker calls `downloads.pause(id)` at once.
2. `decide()` says skip for any of these:
   - an Alt+click on this URL within the last 2 s;
   - an incognito download;
   - a `blob:` or `data:` URL;
   - a download with no plain GET URL (a form POST, detected as a missing `finalUrl` or a non-http(s) scheme);
   - a size under the minimum;
   - an unknown size and a type not on the list.
   Otherwise it says catch.
3. On skip, call `downloads.resume(id)` and stop.
4. On catch, collect the final URL, the suggested file name, the size, the referrer and the user agent, plus `cookies.getAll({ url })` joined into one `Cookie` header. POST to `/v1/capture` with a 1.5 s timeout.
5. On `200`, call `downloads.cancel(id)`, then `downloads.erase({ id })`.
6. On any other result (no answer, timeout, 401, 403, 5xx), call `downloads.resume(id)` and record the reason for the popup. A 401 also clears the stored key, so the extension shows as unpaired.

### 4.2 "Download with Haul" (one link)
The same as 4.1 from step 4 onward, with no browser download to pause, cancel or resume. If Haul fails, a browser notification says so.

### 4.3 "Download all links…"
The picker collects the links (via `scripting.executeScript` in the active tab), filters them, and POSTs the checked ones to `/v1/links`. They land in the Collector marked Browser, with the page title as the batch name, and are checked online as usual. No cookies are sent (ruling).

### 4.4 Duplicates and re-capture
- The same URL captured twice within 5 s returns the first capture's id, without adding a second download.
- A capture whose URL matches a download in `LinkExpired` or `Paused` state replaces that download's headers (`refresh_headers`) and resumes it, instead of adding a new one.

### 4.5 Finding Haul
- The extension remembers the last working port.
- On a failed request it scans 17654–17664 once with `/v1/status`, keeps the first answer with `app: "haul"`, then retries once.

## 5. Errors and edge cases
- **Not running or not paired:** the browser keeps the download, and the popup says why.
- **A removed key:** the server answers `401`, and the extension clears its key and shows Pair again.
- **Worker suspended:** the key, port and options live in `chrome.storage.local`. Nothing is held only in memory.
- **Logs:** never cookie values, header values or tokens. A capture logs host, file name and cookie count.
- **The 60 s pairing wait:** the popup closes as soon as Haul's window takes focus, so the popup only asks the worker to pair, and the worker makes the request. A pending `fetch` keeps the worker alive (Chrome 116+), and 60 s is well inside Chrome's 5-minute limit. The popup reads the result from storage when it is opened again.

## 6. Testing
Test-first throughout: each test is seen failing before the code is written.
- **Engine:**
  - headers sent on probe, connection and resume (the test server gains a "require this cookie" switch);
  - the stored blob is encrypted;
  - headers wiped on complete, remove and permanent failure, and kept on pause;
  - 401 gives Link expired;
  - `refresh_headers` and resume.
- **Front desk:**
  - each lock alone: a missing or wrong key, a wrong Host (including a rebinding name such as `evil.test:17654`), a web-page Origin, a missing Origin, and a key used from a different extension origin;
  - each route with a valid key;
  - pairing: Allow, Deny, the timeout, a second request getting 429, and a removed key;
  - the 5 s duplicate rule and re-capture;
  - the 2 MB and 5,000-link caps;
  - port fallback when 17654 is held;
  - no cookie values in the log after a capture.
- **Extension (Vitest):**
  - `decide()` for every rule;
  - the port finder;
  - the hand-over: cancel only after 200, and resume on every failure, including the 1.5 s timeout;
  - the picker's filters.
- **Window (Playwright, light and dark, with axe):** the pairing dialog, and the Browsers section with Remove.
- **Real end-to-end:**
  1. The real Haul, plus Edge with the unpacked extension, pairs with it.
  2. A click on a local test page's link reaches Haul's Queue, completes with a matching SHA-256, and is gone from Edge's download list.
  3. With Haul closed, the browser keeps the file.

  If Edge refuses to load an unpacked extension under automation, stop and ask the owner before installing Playwright's Chromium (~150 MB).

## 7. Rulings (cost if wrong)
- Chrome and Edge only. Cost: a Firefox build later.
- "Download all links…" sends no cookies. Cost: links from a login-only site show Offline in the Collector.
- Alt works through a recorded Alt+click. Cost: downloads a site starts by itself can't be skipped with Alt.
- The Browsers list lives in the Rules view. Cost: it moves to Settings in plan 6.
- Only a hash of each key is stored. Cost: a lost key means pairing again (no recovery needed).
- Incognito downloads are never caught. Cost: one switch if wanted later.
