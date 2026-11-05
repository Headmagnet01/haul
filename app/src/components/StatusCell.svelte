<script lang="ts">
  import Icon from './Icon.svelte';
  import type { Status } from '../lib/status';

  let { status }: { status: Status } = $props();
</script>

<span class="status tone-{status.tone}">
  <Icon name={status.icon} size={13} /><span class="w">{status.word}</span>
</span>

<style>
  .status {
    display: inline-flex; align-items: center; gap: 6px; font-size: 12.5px; color: var(--ink-2);
    justify-content: flex-end; width: 100%; min-width: 0;
  }
  .status :global(svg) { flex: none; }
  .w { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .tone-signal :global(svg) { color: var(--signal); }
  .tone-ok :global(svg) { color: var(--ok); }
  .tone-idle :global(svg) { color: var(--idle); }
  .tone-err { color: var(--err); }
</style>
