# Haul: product

## What it is
A free, open-source Windows download manager. It pairs IDM's fast, light engine and browser capture with JDownloader 2's link collecting, packages and auto-extraction, in a calm modern interface.

## Who it is for
People who download large or many files: game and software installers, multi-part archives, datasets, videos, ISOs. They are comfortable with computers but don't want to tune settings. The first user is the owner; a public release comes later.

## The job
Get a link (or a hundred) onto disk, verified, extracted and in the right folder, with as few clicks as possible, and survive crashes and dropped connections without losing work.

## Surfaces
| Surface | Mode | Notes |
|---|---|---|
| Desktop app (Queue, Collector, Finished, Rules, Settings) | Operate | Dense, scannable, glanceable from across the room |
| Browser extension popup and picker | Operate | Tiny, fast, matches the app tokens |
| Future project website | Persuade | Not in v1 |

## Principles
1. **Never lose a download.** If Haul can't take it, the browser keeps it.
2. **Never destroy data.** No silent overwrites; deletion goes to the Recycle Bin and asks first.
3. **Show the engine working.** The segment strip makes speed visible and trustworthy.
4. **Quiet by default.** No pop-up per download, no nags, no bundled offers.
5. **Automate the obvious, ask about the ambiguous.** Browser links start on their own; clipboard links wait for review.

## Anti-references
- JDownloader 2's wall of settings and Java-era chrome.
- IDM's per-download pop-up windows and 2005-era toolbar icons.
- Neon "gamer" download managers with gradients and glowing progress bars.

## Platform
Windows 10 and 11, desktop, mouse and keyboard first. Tauri 2 (WebView2) with a Svelte UI.
