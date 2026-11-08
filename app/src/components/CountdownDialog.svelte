<script lang="ts">
  import { tick } from 'svelte';
  import type { WhenDoneState } from '../lib/when-done-state.svelte';

  let { wd }: { wd: WhenDoneState } = $props();

  let cancelBtn = $state<HTMLButtonElement | null>(null);
  let nowBtn = $state<HTMLButtonElement | null>(null);
  const open = $derived(wd.status.countdown !== null && wd.status.mode !== 'nothing');
  const title = $derived(
    wd.status.mode === 'shutdown' ? `Shutting down in ${wd.status.countdown} s` : `Going to sleep in ${wd.status.countdown} s`,
  );

  // Cancel is the default: Enter and Esc both cancel.
  $effect(() => {
    if (open) void tick().then(() => cancelBtn?.focus());
  });

  function onkeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault();
      void wd.cancel();
    } else if (e.key === 'Tab') {
      e.preventDefault();
      (document.activeElement === cancelBtn ? nowBtn : cancelBtn)?.focus();
    }
  }
</script>

{#if open}
  <div class="scrim">
    <div
      class="dialog"
      role="alertdialog"
      aria-modal="true"
      aria-labelledby="wd-title"
      aria-describedby="wd-body"
      tabindex="-1"
      {onkeydown}
    >
      <h2 id="wd-title" class="num">{title}</h2>
      <p id="wd-body">
        {wd.status.mode === 'shutdown'
          ? 'Downloads are done. Haul saves everything and shuts the PC down.'
          : 'Downloads are done. Haul keeps running while the PC sleeps.'}
      </p>
      <div class="actions">
        <button type="button" class="btn" bind:this={nowBtn} onclick={() => wd.now()}>Do it now</button>
        <button type="button" class="btn primary" bind:this={cancelBtn} onclick={() => wd.cancel()}>Cancel</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .scrim {
    position: fixed; inset: 0; z-index: 60; display: grid; place-items: center;
    background: color-mix(in srgb, var(--ink) 28%, transparent);
  }
  .dialog {
    width: min(440px, calc(100vw - 32px)); background: var(--raised); color: var(--ink);
    border: 1px solid var(--line); border-radius: 8px; padding: 18px 20px;
    box-shadow: 0 12px 32px color-mix(in srgb, var(--ink) 22%, transparent);
  }
  h2 { font: 600 15px var(--sans); letter-spacing: -0.01em; margin-bottom: 8px; font-variant-numeric: tabular-nums; }
  p { font-size: 13px; color: var(--ink-2); margin-bottom: 16px; }
  .actions { display: flex; justify-content: flex-end; gap: 8px; }
</style>
