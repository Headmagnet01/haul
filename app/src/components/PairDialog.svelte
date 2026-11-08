<script lang="ts">
  import { tick } from 'svelte';
  import type { Backend } from '../lib/backend';

  let { backend, onanswered }: { backend: Backend; onanswered: () => void } = $props();

  let browser = $state<string | null>(null);
  let allowBtn = $state<HTMLButtonElement | null>(null);
  let denyBtn = $state<HTMLButtonElement | null>(null);

  $effect(() =>
    backend.onPairRequest(async (b) => {
      browser = b;
      await tick();
      allowBtn?.focus();
    }),
  );

  async function answer(allow: boolean) {
    if (browser === null) return;
    browser = null;
    await backend.pairAnswer(allow);
    onanswered();
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault();
      void answer(false);
    } else if (e.key === 'Tab') {
      // Keep focus on the two buttons while the question is open.
      e.preventDefault();
      (document.activeElement === allowBtn ? denyBtn : allowBtn)?.focus();
    }
  }
</script>

{#if browser !== null}
  <div class="scrim">
    <div
      class="dialog"
      role="alertdialog"
      aria-modal="true"
      aria-labelledby="pair-title"
      aria-describedby="pair-body"
      tabindex="-1"
      {onkeydown}
    >
      <h2 id="pair-title">Allow {browser} to send downloads to Haul?</h2>
      <p id="pair-body">It can then add downloads and links. You can remove it later in Rules → Browsers.</p>
      <div class="actions">
        <button type="button" class="btn" bind:this={denyBtn} onclick={() => answer(false)}>Deny</button>
        <button type="button" class="btn primary" bind:this={allowBtn} onclick={() => answer(true)}>Allow</button>
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
  h2 { font: 600 15px var(--sans); letter-spacing: -0.01em; margin-bottom: 8px; }
  p { font-size: 13px; color: var(--ink-2); margin-bottom: 16px; }
  .actions { display: flex; justify-content: flex-end; gap: 8px; }
</style>
