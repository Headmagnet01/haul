<script lang="ts">
  import type { Cell, StripKind } from '../lib/strip';

  let { cells, kind }: { cells: Cell[]; kind: StripKind } = $props();
</script>

<div class="bigmap {kind}" style:grid-template-columns="repeat({cells.length}, 1fr)" aria-hidden="true">
  {#each cells as c, i (i)}
    <div class="cell" class:full={c.fill >= 1}><b style:height="{(c.fill * 100).toFixed(1)}%"></b></div>
  {/each}
</div>

<style>
  .bigmap { display: grid; gap: 3px; }
  .cell { height: 34px; background: var(--track); border-radius: 3px; position: relative; overflow: hidden; min-width: 2px; }
  .cell b { position: absolute; left: 0; right: 0; bottom: 0; background: var(--signal); }
  .cell.full b, .done .cell b { background: var(--ok); }
  .paused .cell:not(.full) b { background: var(--idle); }
  .error .cell:not(.full) b { background: var(--err); }
</style>
