<script lang="ts">
  import { tick } from 'svelte';
  import type { Chip, CollectorState } from '../lib/collector-state.svelte';

  let { c }: { c: CollectorState } = $props();

  const CHIPS: { id: Chip; label: string }[] = [
    { id: 'html', label: '.html pages' },
    { id: 'images', label: 'images' },
    { id: 'small', label: 'under 1 MB' },
    { id: 'duplicates', label: 'duplicates' },
  ];

  let adding = $state(false);
  let value = $state('');
  let box: HTMLInputElement | undefined = $state();

  async function open() {
    adding = true;
    await tick();
    box?.focus();
  }

  async function add() {
    await c.addExt(value);
    value = '';
    adding = false;
  }
</script>

<div class="chips">
  <span>Hide:</span>
  {#each CHIPS as chip (chip.id)}
    <button type="button" class="chip" aria-pressed={c.snapshot.filters[chip.id]} onclick={() => c.toggleChip(chip.id)}>
      {chip.label}
    </button>
  {/each}
  {#each c.snapshot.filters.exts as ext (ext)}
    <button type="button" class="chip" aria-pressed="true" title="Click to stop hiding .{ext} files" onclick={() => c.removeExt(ext)}>
      .{ext}
    </button>
  {/each}
  {#if adding}
    <input
      bind:this={box}
      bind:value
      class="ext"
      aria-label="File type to hide"
      placeholder="ext"
      maxlength="12"
      onkeydown={(e) => {
        if (e.key === 'Enter') void add();
        if (e.key === 'Escape') {
          value = '';
          adding = false;
        }
      }}
      onblur={() => {
        if (!value.trim()) adding = false;
      }}
    />
  {:else}
    <button type="button" class="chip add" onclick={open}>+ Type</button>
  {/if}
</div>

<style>
  .chips {
    display: flex; gap: 6px; padding: 10px 20px; border-bottom: 1px solid var(--line); align-items: center;
    font-size: 12.5px; color: var(--ink-3); flex-wrap: wrap;
  }
  .chip {
    border: 1px solid var(--line); border-radius: 14px; padding: 3px 10px; font-size: 12px; color: var(--ink-2);
    background: var(--raised); transition: border-color 0.15s;
  }
  .chip:hover { border-color: var(--ink-3); }
  .chip[aria-pressed='true'] { background: var(--ink); color: var(--surface); border-color: var(--ink); }
  .chip.add { border-style: dashed; }
  .ext {
    width: 72px; border: 1px solid var(--ink-3); border-radius: 14px; padding: 3px 10px; font: 12px var(--mono);
    background: var(--raised); color: var(--ink);
  }
  .ext::placeholder { color: var(--ink-3); }
</style>
