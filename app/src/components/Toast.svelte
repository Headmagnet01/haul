<script lang="ts">
  import type { ToastMsg } from '../lib/app-state.svelte';

  let { toast, ondismiss }: { toast: ToastMsg | null; ondismiss: () => void } = $props();
</script>

<div class="region" role="status" aria-live="polite">
  {#if toast}
    <div class="toast">
      <span>{toast.text}</span>
      {#if toast.action}
        <button
          type="button"
          class="act"
          onclick={() => {
            toast.action?.run();
            ondismiss();
          }}>{toast.action.label}</button>
      {/if}
    </div>
  {/if}
</div>

<style>
  .region { position: fixed; left: 50%; bottom: 56px; transform: translateX(-50%); z-index: 10; pointer-events: none; }
  .toast {
    pointer-events: auto; display: flex; align-items: center; gap: 14px; padding: 8px 14px; border-radius: 6px;
    background: var(--ink); color: var(--surface); font-size: 13px; white-space: nowrap;
  }
  .act { font-weight: 600; color: var(--surface); text-decoration: underline; text-underline-offset: 3px; }
</style>
