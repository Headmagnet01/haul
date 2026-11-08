<script lang="ts">
  import { tick } from 'svelte';
  import type { AppState } from '../lib/app-state.svelte';

  let { app }: { app: AppState } = $props();

  let cancelBtn = $state<HTMLButtonElement | null>(null);
  let okBtn = $state<HTMLButtonElement | null>(null);
  const ids = $derived(app.confirmDelete);
  const what = $derived.by(() => {
    if (!ids) return '';
    if (ids.length === 1) return app.byId.get(ids[0])?.name ?? 'this file';
    return `${ids.length} files`;
  });

  // Cancel is the default: a delete is never one stray Enter away.
  $effect(() => {
    if (ids) void tick().then(() => cancelBtn?.focus());
  });

  function onkeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault();
      app.cancelDelete();
    } else if (e.key === 'Tab') {
      e.preventDefault();
      (document.activeElement === cancelBtn ? okBtn : cancelBtn)?.focus();
    }
  }
</script>

{#if ids}
  <div class="scrim">
    <div
      class="dialog"
      role="alertdialog"
      aria-modal="true"
      aria-labelledby="del-title"
      aria-describedby="del-body"
      tabindex="-1"
      {onkeydown}
    >
      <h2 id="del-title">Move {what} to the Recycle Bin?</h2>
      <p id="del-body">Haul removes {ids.length === 1 ? 'it' : 'them'} from the list too. You can restore files from the Recycle Bin.</p>
      <div class="actions">
        <button type="button" class="btn" bind:this={cancelBtn} onclick={() => app.cancelDelete()}>Cancel</button>
        <button type="button" class="btn danger" bind:this={okBtn} onclick={() => app.deleteFiles(ids)}>Delete file{ids.length === 1 ? '' : 's'}</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .scrim {
    position: fixed; inset: 0; z-index: 50; display: grid; place-items: center;
    background: color-mix(in srgb, var(--ink) 28%, transparent);
  }
  .dialog {
    width: min(440px, calc(100vw - 32px)); background: var(--raised); color: var(--ink);
    border: 1px solid var(--line); border-radius: 8px; padding: 18px 20px;
    box-shadow: 0 12px 32px color-mix(in srgb, var(--ink) 22%, transparent);
  }
  h2 { font: 600 15px var(--sans); letter-spacing: -0.01em; margin-bottom: 8px; word-break: break-word; }
  p { font-size: 13px; color: var(--ink-2); margin-bottom: 16px; }
  .actions { display: flex; justify-content: flex-end; gap: 8px; }
  .danger { color: var(--err); }
</style>
