# Haul: design system

Reference implementation: `docs/mockup/index.html`. When code and this file disagree, update one so they match.

## World
A **calm dispatch board**. Packages are shipments; the app is the board tracking them. Industrial, precise and quiet: paper and graphite surfaces, one safety-orange signal colour, monospaced figures.

## Type
- **IBM Plex Sans** (400, 500, 600) for interface text. **IBM Plex Mono** (400, 500, 600) for all numbers, sizes, speeds, times, column headers and labels.
- The fonts are bundled with the app (OFL licence), never loaded from the web.
- All numbers use `font-variant-numeric: tabular-nums` so they don't shift as they update.
- Sizes:
  - 17px/600 for view titles;
  - 15px/600 for the drawer title;
  - 14px for body text;
  - 13px for buttons and secondary text;
  - 12.5px mono for figures;
  - 11px mono uppercase with 0.07–0.08em tracking for column headers and section labels.

## Colour tokens

| Token | Light | Dark | Use |
|---|---|---|---|
| `--bg` | `#F1EEE6` | `#141412` | Rail, drawer, status bar |
| `--surface` | `#FAF8F3` | `#1A1A17` | Main list |
| `--raised` | `#FFFFFF` | `#22221E` | Inputs, buttons |
| `--ink` | `#1C1B19` | `#ECE8DF` | Primary text |
| `--ink-2` | `#5F5A51` | `#A7A296` | Secondary text |
| `--ink-3` | `#6B6559` | `#8F8B82` | Labels, hints (darkened from the mockup's `#8D877B`/`#77736A` for AA contrast) |
| `--line` | `#DED8CB` | `#2D2C28` | Borders |
| `--line-2` | `#E9E4D9` | `#262521` | Row dividers, hover |
| `--track` | `#E6E1D5` | `#2C2B27` | Empty progress track |
| `--sel` | `#ECE5D6` | `#26251F` | Selected row |
| `--signal` | `#D9520B` | `#FF7B33` | Active downloads, primary activity, focus ring |
| `--ok` | `#2E7A4D` | `#55B57F` | Verified or complete only |
| `--err` | `#B8352A` | `#EC6457` | Errors only |
| `--idle` | `#A7A194` | `#5E5B54` | Paused or checking |

### Colour rules
- **One signal colour.** Orange means "something is moving". Green is reserved for "verified/complete". Red is for errors only.
- **Never colour alone.** Every status pairs an icon and a word with its colour.
- Status words are `--ink-2`; the icon carries the colour; only error words use `--err` (AA contrast).
- **The primary button is ink on surface**, not orange, so orange stays meaningful.
- **Theme:** follows Windows' light/dark setting by default, with a manual override in Settings.

## Shape and spacing
- Radii are 6px for buttons and inputs, 2–3px for segment cells, and 4px for tags.
- No shadows in the main window; separation comes from 1px lines and surface steps.
- Row heights are 40px for files and 44px for packages, with 20px horizontal padding in the list.
- Fixed widths: the left rail is 208px and the detail drawer 340px. The minimum window size is 1080×640.

## Signature: the segment strip
- A 10px-tall strip made of one cell per byte range, with a 2px gap between cells. Each cell's width is proportional to its range and its fill to its progress.
- Fill colours by state:
  - active: `--signal`
  - complete: `--ok`
  - paused: `--idle`
  - error: `--err`
- A package row's strip has one cell per file.
- The drawer's large map uses vertical fill cells (34px tall). Under it, one line of text describes the latest steal.

## Motion
- Progress fills update at most 4 times per second and do not animate between updates.
- Hover and toggle transitions take 150ms.
- The only looping animations are the running-step spinner and the "checking" dot pulse.
- `prefers-reduced-motion` turns off every animation and transition.

## Bans
Gradients, glows, glassmorphism, coloured side-stripe borders, emoji as icons, a pop-up window per download, and modal dialogs for anything that can be inline or a toast.

## Icons
16px line icons with a 1.5px stroke in `currentColor`, drawn to match the mockup set. One icon family only.
