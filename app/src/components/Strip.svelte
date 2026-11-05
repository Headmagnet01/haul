<script lang="ts">
  import type { Cell, StripKind } from '../lib/strip';

  let { cells, kind }: { cells: Cell[]; kind: StripKind } = $props();
</script>

<div class="strip {kind}" aria-hidden="true">
  {#each cells as c, i (i)}
    <span class="seg" style:flex-grow={c.flex}><b style:width="{(c.fill * 100).toFixed(1)}%"></b></span>
  {/each}
</div>

<style>
  .strip { display: flex; gap: 2px; height: 10px; min-width: 0; }
  .seg { position: relative; flex: 1 1 0; min-width: 2px; background: var(--track); border-radius: 2px; overflow: hidden; }
  .seg b { position: absolute; inset: 0 auto 0 0; background: var(--signal); border-radius: 2px; }
  .done .seg b { background: var(--ok); }
  .paused .seg b { background: var(--idle); }
  .error .seg b { background: var(--err); }
</style>
