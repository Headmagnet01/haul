<script lang="ts" module>
  import type { View } from '../lib/app-state.svelte';
  export type { View };
  export const VIEWS: { id: View; label: string }[] = [
    { id: 'queue', label: 'Queue' },
    { id: 'collector', label: 'Collector' },
    { id: 'finished', label: 'Finished' },
    { id: 'rules', label: 'Rules' },
    { id: 'settings', label: 'Settings' },
  ];
</script>

<script lang="ts">
  import Icon from './Icon.svelte';

  let {
    view,
    onselect,
    queueCount = 0,
    collectorCount = 0,
  }: { view: View; onselect: (v: View) => void; queueCount?: number; collectorCount?: number } = $props();
  const countOf = (v: View) => (v === 'queue' ? queueCount : v === 'collector' ? collectorCount : 0);
</script>

<aside class="rail">
  <div class="mark"><i aria-hidden="true"></i>haul</div>
  <nav class="nav" aria-label="Views">
    {#each VIEWS as v (v.id)}
      <button
        type="button"
        aria-current={view === v.id ? 'page' : undefined}
        onclick={() => onselect(v.id)}
      >
        <Icon name={v.id} />{v.label}
        {#if countOf(v.id) > 0}<span class="ct" aria-hidden="true">{countOf(v.id)}</span>{/if}
      </button>
    {/each}
  </nav>
  <div class="foot"></div>
</aside>

<style>
  .rail {
    grid-row: 1; background: var(--bg); border-right: 1px solid var(--line); padding: 18px 12px;
    display: flex; flex-direction: column; gap: 22px; min-height: 0;
  }
  .mark { font: 600 19px/1 var(--mono); letter-spacing: -0.02em; padding: 0 10px; display: flex; align-items: center; gap: 9px; }
  .mark i { width: 14px; height: 14px; background: var(--signal); display: inline-block; clip-path: polygon(0 0, 100% 50%, 0 100%); }
  .nav { display: flex; flex-direction: column; gap: 2px; }
  .nav button {
    display: flex; align-items: center; gap: 10px; padding: 7px 10px; border-radius: 6px; color: var(--ink-2);
    text-align: left; transition: background 0.15s, color 0.15s;
  }
  .nav button:hover { background: var(--line-2); color: var(--ink); }
  .nav button[aria-current='page'] { background: var(--sel); color: var(--ink); font-weight: 500; }
  .ct { margin-left: auto; font: 500 11px var(--mono); color: var(--ink-3); font-variant-numeric: tabular-nums; }
  .foot { margin-top: auto; }
</style>
