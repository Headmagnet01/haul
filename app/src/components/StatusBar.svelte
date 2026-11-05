<script lang="ts">
  import Sparkline from './Sparkline.svelte';
  import Icon from './Icon.svelte';
  import type { AppState } from '../lib/app-state.svelte';
  import { formatSpeed } from '../lib/format';

  let { app }: { app: AppState } = $props();
  const limit = $derived(app.snapshot.speedLimit);
  const limitText = $derived(formatSpeed(limit.bps).replace(/\.0 /, ' '));
</script>

<footer class="bar">
  <span class="spd">
    <Icon name="dl" size={12} />
    <span class="num">{app.totals.totalBps > 0 ? formatSpeed(app.totals.totalBps) : '0 B/s'}</span>
    <Sparkline samples={app.speedHistory} />
  </span>
  <span class="sep" aria-hidden="true"></span>
  <span>{app.totals.downloading} active · {app.totals.waiting} waiting</span>
  <span class="sep" aria-hidden="true"></span>
  <button
    type="button"
    class="tog"
    aria-pressed={limit.enabled}
    onclick={() => app.setSpeedLimit({ enabled: !limit.enabled, bps: limit.bps })}
  >
    <span class="sw" aria-hidden="true"></span>Speed limit <span class="num lim">{limitText}</span>
  </button>
</footer>

<style>
  .bar {
    grid-column: 1 / -1; grid-row: 2; display: flex; align-items: center; gap: 18px; padding: 0 16px;
    border-top: 1px solid var(--line); background: var(--bg); font-size: 12.5px; color: var(--ink-2); min-width: 0;
  }
  .spd { display: flex; align-items: center; gap: 8px; color: var(--ink); font-weight: 500; }
  .spd .num { min-width: 72px; }
  .sep { width: 1px; height: 16px; background: var(--line); }
  .tog { display: inline-flex; align-items: center; gap: 7px; padding: 3px 8px; border-radius: 5px; transition: background 0.15s; }
  .tog:hover { background: var(--line-2); }
  .sw { width: 24px; height: 14px; border-radius: 7px; background: var(--line); position: relative; transition: background 0.15s; }
  .sw::after {
    content: ''; position: absolute; top: 2px; left: 2px; width: 10px; height: 10px; border-radius: 50%;
    background: var(--surface); transition: transform 0.15s;
  }
  .tog[aria-pressed='true'] .sw { background: var(--signal); }
  .tog[aria-pressed='true'] .sw::after { transform: translateX(10px); }
  .lim { color: var(--ink-3); }
</style>
