<script lang="ts">
  import type { WhenDoneState } from '../lib/when-done-state.svelte';
  import type { WhenDoneMode } from '../lib/types';

  let { wd }: { wd: WhenDoneState } = $props();
</script>

<label class="wd" class:on={wd.status.mode !== 'nothing'}>
  When done
  <select value={wd.status.mode} onchange={(e) => wd.set(e.currentTarget.value as WhenDoneMode)}>
    <option value="nothing">Nothing</option>
    <option value="sleep">Sleep</option>
    <option value="shutdown">Shut down</option>
  </select>
</label>

<style>
  .wd { display: inline-flex; align-items: center; gap: 7px; }
  select {
    color: var(--ink); background: var(--raised); border: 1px solid var(--line); border-radius: 5px;
    padding: 1px 6px; font-size: 12.5px;
  }
  .on select { border-color: var(--signal); }
</style>
