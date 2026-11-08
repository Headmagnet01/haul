<script lang="ts">
  import type { Backend } from '../lib/backend';
  import type { SettingsState } from '../lib/settings-state.svelte';
  import type { UpdateState } from '../lib/update-state.svelte';
  import { WEEKDAYS, type Settings, type SettingsPatch, type Weekday } from '../lib/types';
  import BrowserList from './BrowserList.svelte';
  import { parseSpeed, parseWhole } from '../lib/settings-num';

  let { ss, backend, browsersRev = 0, up }: { ss: SettingsState; backend: Backend; browsersRev?: number; up?: UpdateState } =
    $props();

  const s = $derived(ss.settings);
  const MB = 1_000_000;

  async function pick() {
    const dir = await backend.pickFolder();
    if (dir) await ss.save({ downloadDir: dir });
  }

  const RANGE = { maxParallel: [1, 20], connections: [1, 16] } as const;

  function num(field: 'maxParallel' | 'connections', e: Event) {
    const [min, max] = RANGE[field];
    const v = parseWhole((e.currentTarget as HTMLInputElement).value, min, max);
    if (v === null) ss.refuse(field, `Enter a whole number from ${min} to ${max}.`);
    else void ss.save({ [field]: v } as SettingsPatch);
  }

  function limitValue(cur: Settings, e: Event) {
    const mb = parseSpeed((e.currentTarget as HTMLInputElement).value);
    if (mb === null) ss.refuse('speedLimit', 'Enter a speed of 0.01 MB/s or more.');
    else void ss.save({ speedLimit: { ...cur.speedLimit, bps: Math.round(mb * MB) } });
  }

  const hhmm = (t: string) => t.slice(0, 5);
  const DEFAULT_SCHEDULE = { start: '01:00:00', stop: '07:00:00', weekdays: [...WEEKDAYS] };

  function scheduleTime(cur: Settings, field: 'start' | 'stop', e: Event) {
    const v = (e.currentTarget as HTMLInputElement).value;
    if (cur.schedule && /^\d\d:\d\d$/.test(v)) void ss.save({ schedule: { ...cur.schedule, [field]: `${v}:00` } });
  }

  function toggleDay(cur: Settings, d: Weekday) {
    if (!cur.schedule) return;
    const has = cur.schedule.weekdays.includes(d);
    const weekdays = has ? cur.schedule.weekdays.filter((x) => x !== d) : WEEKDAYS.filter((x) => x === d || cur.schedule!.weekdays.includes(x));
    void ss.save({ schedule: { ...cur.schedule, weekdays } });
  }

  // Enter saves a number field like leaving it does.
  function enter(e: KeyboardEvent) {
    if (e.key === 'Enter') (e.currentTarget as HTMLInputElement).blur();
  }
</script>

{#snippet sw(label: string, on: boolean, patch: (v: boolean) => SettingsPatch, hint?: string)}
  <div class="opt">
    <button type="button" role="switch" class="tog" aria-checked={on} onclick={() => ss.save(patch(!on))}>
      <span class="sw" aria-hidden="true"></span>{label}
    </button>
    {#if hint}<p class="help">{hint}</p>{/if}
  </div>
{/snippet}

{#snippet err(field: keyof Settings)}
  {#if ss.error[field]}<p class="err" role="alert">{ss.error[field]?.replace(/^\w+: /, '')}</p>{/if}
{/snippet}

<main class="main" aria-labelledby="h-settings">
  <div class="top"><h1 id="h-settings">Settings</h1></div>
  {#if s}
    {#key ss.rev}
      <div class="scroll">
        <section aria-labelledby="h-downloads">
          <h2 id="h-downloads">Downloads</h2>
          <div class="field">
            <span class="lbl" id="l-folder">Download folder</span>
            <div class="folder">
              <span class="path" aria-labelledby="l-folder">{s.downloadDir}</span>
              <button type="button" class="btn sm" onclick={pick}>Change…</button>
              <button type="button" class="btn sm" onclick={() => backend.openPath(s.downloadDir)}>Open</button>
            </div>
            {@render err('downloadDir')}
            <p class="help">New downloads go here. Downloads already waiting keep their folder.</p>
          </div>
          <div class="nums">
            <label>Downloads at once
              <input type="number" min="1" max="20" value={s.maxParallel} onchange={(e) => num('maxParallel', e)} onkeydown={enter} />
            </label>
            <label>Connections per download
              <input type="number" min="1" max="16" value={s.connections} onchange={(e) => num('connections', e)} onkeydown={enter} />
            </label>
            <label>Speed limit (MB/s)
              <input type="number" min="0.01" step="0.1" value={+(s.speedLimit.bps / MB).toFixed(2)} onchange={(e) => limitValue(s, e)} onkeydown={enter} />
            </label>
          </div>
          {@render err('maxParallel')}
          {@render err('connections')}
          {@render err('speedLimit')}
          <p class="help">The speed limit applies while its switch in the status bar is on.</p>
        </section>

        <section aria-labelledby="h-adding">
          <h2 id="h-adding">Adding links</h2>
          <fieldset class="radios">
            <legend>Downloads clicked in the browser</legend>
            <label><input type="radio" name="bs" checked={s.browserStart === 'now'} onchange={() => ss.save({ browserStart: 'now' })} /> Start at once</label>
            <label><input type="radio" name="bs" checked={s.browserStart === 'collector'} onchange={() => ss.save({ browserStart: 'collector' })} /> Wait in the Collector</label>
          </fieldset>
          {@render sw('Watch the clipboard', s.watchClipboard, (v) => ({ watchClipboard: v }), 'Haul notices download links you copy; nothing is sent anywhere.')}
        </section>

        <section aria-labelledby="h-schedule">
          <h2 id="h-schedule">Schedule</h2>
          {@render sw('Only download at certain times', s.schedule !== null, (v) => ({ schedule: v ? DEFAULT_SCHEDULE : null }))}
          {#if s.schedule}
            <div class="nums">
              <label>Start at <input type="time" value={hhmm(s.schedule.start)} onchange={(e) => scheduleTime(s, 'start', e)} /></label>
              <label>Stop at <input type="time" value={hhmm(s.schedule.stop)} onchange={(e) => scheduleTime(s, 'stop', e)} /></label>
            </div>
            <fieldset class="days">
              <legend>On these days</legend>
              {#each WEEKDAYS as d (d)}
                <label><input type="checkbox" checked={s.schedule.weekdays.includes(d)} onchange={() => toggleDay(s, d)} /> {d}</label>
              {/each}
            </fieldset>
            {@render err('schedule')}
            <p class="help">Outside these times downloads wait and running ones pause. The same start and stop time means all day.</p>
          {/if}
        </section>

        <section aria-labelledby="h-app">
          <h2 id="h-app">App</h2>
          {@render sw('Closing the window keeps Haul in the tray', s.closeToTray, (v) => ({ closeToTray: v }), 'Downloads keep going. Quit from the tray icon.')}
          {@render sw('Start Haul when I sign in to Windows', s.autostart, (v) => ({ autostart: v }), 'Haul starts in the tray, without a window.')}
          {@render err('autostart')}
          {@render sw('Notify me when a download finishes', s.notifyFinished, (v) => ({ notifyFinished: v }))}
          {@render sw('Check for updates', s.checkUpdates, (v) => ({ checkUpdates: v }), 'Once a day Haul asks GitHub for its latest version. Nothing else is sent.')}
          {#if up}
            <div class="check">
              <button type="button" class="btn sm" disabled={up.checking} onclick={() => up.checkNow()}>Check now</button>
              <p class="help" aria-live="polite">
                {up.checking ? 'Checking…' : up.message()}
                {#if !up.checking && up.status?.latest && up.status.url}
                  <a href={up.status.url} onclick={(e) => { e.preventDefault(); void up.open(); }}>Open the release page</a>
                {/if}
              </p>
            </div>
          {/if}
        </section>

        <section aria-labelledby="h-browsers">
          <h2 id="h-browsers">Browsers</h2>
          <BrowserList {backend} rev={browsersRev} />
        </section>
      </div>
    {/key}
  {/if}
</main>

<style>
  .main { grid-row: 1; display: flex; flex-direction: column; min-width: 0; min-height: 0; background: var(--surface); }
  .top { display: flex; align-items: center; gap: 12px; padding: 14px 20px; border-bottom: 1px solid var(--line); min-width: 0; }
  h1 { font: 600 17px var(--sans); letter-spacing: -0.01em; }
  .scroll { overflow: auto; min-height: 0; }
  section { padding: 18px 20px; border-bottom: 1px solid var(--line); max-width: 860px; display: grid; gap: 10px; justify-items: start; }
  h2 { font: 500 11px var(--mono); text-transform: uppercase; letter-spacing: 0.08em; color: var(--ink-3); }
  .help { font-size: 12px; color: var(--ink-3); }
  .err { font-size: 12px; color: var(--err); }
  .field { display: grid; gap: 6px; width: 100%; }
  .lbl, legend { font-size: 12px; color: var(--ink-2); }
  .folder { display: flex; align-items: center; gap: 8px; min-width: 0; width: 100%; }
  .path {
    font: 12.5px var(--mono); color: var(--ink); background: var(--raised); border: 1px solid var(--line); border-radius: 5px;
    padding: 5px 8px; flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .nums { display: flex; gap: 16px; flex-wrap: wrap; }
  .nums label { display: grid; gap: 4px; font-size: 12px; color: var(--ink-2); }
  input[type='number'], input[type='time'] {
    color: var(--ink); background: var(--raised); border: 1px solid var(--line); border-radius: 5px; padding: 5px 8px;
    font: 12.5px var(--mono); width: 130px;
  }
  fieldset { border: 0; padding: 0; margin: 0; display: flex; gap: 14px; flex-wrap: wrap; align-items: center; }
  fieldset legend { margin-bottom: 6px; width: 100%; }
  fieldset label { font-size: 13px; color: var(--ink); display: inline-flex; gap: 6px; align-items: center; }
  input[type='radio'], input[type='checkbox'] { accent-color: var(--signal); }
  .opt { display: grid; gap: 2px; }
  .opt .help { padding-left: 41px; }
  .tog {
    display: inline-flex; align-items: center; gap: 9px; padding: 4px 8px; border-radius: 5px; font-size: 13px;
    color: var(--ink); transition: background 0.15s;
  }
  .tog:hover { background: var(--line-2); }
  .sw { width: 24px; height: 14px; border-radius: 7px; background: var(--line); position: relative; transition: background 0.15s; flex: none; }
  .sw::after {
    content: ''; position: absolute; top: 2px; left: 2px; width: 10px; height: 10px; border-radius: 50%;
    background: var(--surface); transition: transform 0.15s;
  }
  .tog[aria-checked='true'] .sw { background: var(--signal); }
  .tog[aria-checked='true'] .sw::after { transform: translateX(10px); }
  .sm { padding: 3px 8px; font-size: 12px; flex: none; }
  .check { display: flex; align-items: center; gap: 10px; padding-left: 8px; }
  .check a { color: var(--ink); text-decoration: underline; text-underline-offset: 2px; }
  @media (prefers-reduced-motion: reduce) {
    .sw, .sw::after { transition: none; }
  }
</style>
