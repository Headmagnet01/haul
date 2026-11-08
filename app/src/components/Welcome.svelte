<script lang="ts">
  import { onMount } from 'svelte';
  import Icon from './Icon.svelte';
  import type { AppState } from '../lib/app-state.svelte';
  import type { CollectorState } from '../lib/collector-state.svelte';
  import type { PairedClient } from '../lib/types';

  // The first-run empty state: three steps, each ticked when done. Shown until the first link arrives.
  let { app, collector }: { app: AppState; collector: CollectorState } = $props();

  let clients = $state<PairedClient[]>([]);
  const loadClients = async () => {
    try {
      clients = await app.backend.pairedClients();
    } catch {
      /* the list stays as it was */
    }
  };
  onMount(() => {
    void loadClients();
    return app.backend.onPairClosed(() => void loadClients());
  });

  const paired = $derived(clients.at(-1)?.browser ?? null);
  const watching = $derived(collector.snapshot.watchClipboard);

  async function openFolder() {
    try {
      await app.backend.openExtensionFolder();
    } catch (e) {
      app.showToast(typeof e === 'string' ? e : "The extension folder couldn't be opened.");
    }
  }
</script>

{#snippet done(on: boolean)}
  {#if on}<span class="done"><Icon name="ok" size={14} />Done</span>{/if}
{/snippet}

<section class="welcome" aria-labelledby="welcome-h">
  <h2 id="welcome-h">Welcome to Haul</h2>
  <ol>
    <li>
      <span class="n" aria-hidden="true">1</span>
      <div class="body">
        <p class="t">Add a link {@render done(false)}</p>
        <p class="s">Copy a link or drop one here. Press <kbd>Ctrl V</kbd> anywhere to add links.</p>
      </div>
    </li>
    <li class:ticked={watching}>
      <span class="n" aria-hidden="true">2</span>
      <div class="body">
        <p class="t">Watch the clipboard {@render done(watching)}</p>
        <p class="s">Haul notices download links you copy; nothing is sent anywhere.</p>
        <button
          type="button"
          role="switch"
          class="tog"
          aria-checked={watching}
          onclick={() => collector.setWatchClipboard(!watching)}
        >
          <span class="sw" aria-hidden="true"></span>Watch clipboard
        </button>
      </div>
    </li>
    <li class:ticked={paired !== null}>
      <span class="n" aria-hidden="true">3</span>
      <div class="body">
        <p class="t">Pair your browser {@render done(paired !== null)}</p>
        {#if paired}
          <p class="s">Paired with {paired}</p>
        {:else}
          <p class="s">
            In Edge or Chrome, open the extensions page, turn on Developer mode, choose Load unpacked and pick
            Haul's extension folder. Then click the Haul button in the toolbar and choose Pair with Haul.
          </p>
        {/if}
        <button type="button" class="btn" onclick={openFolder}>
          <Icon name="folder" size={14} />Open the extension folder
        </button>
      </div>
    </li>
  </ol>
</section>

<style>
  .welcome { flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 18px; padding: 40px 20px; }
  h2 { font: 600 17px var(--sans); color: var(--ink); }
  ol { list-style: none; margin: 0; padding: 0; display: grid; gap: 14px; max-width: 520px; width: 100%; }
  li { display: flex; gap: 12px; padding: 14px 16px; border: 1px solid var(--line); border-radius: 8px; background: var(--raised); }
  .n {
    flex: none; width: 22px; height: 22px; border-radius: 50%; display: grid; place-items: center;
    font: 600 12px var(--sans); color: var(--ink-2); background: var(--line-2);
  }
  li.ticked .n { background: var(--signal); color: var(--surface); }
  .body { display: grid; gap: 6px; justify-items: start; min-width: 0; }
  .t { font: 600 14px var(--sans); color: var(--ink); display: flex; gap: 10px; align-items: center; }
  .s { font-size: 13px; color: var(--ink-2); line-height: 1.45; }
  .done { display: inline-flex; gap: 4px; align-items: center; font: 500 12px var(--sans); color: var(--ink-2); }
  .done :global(svg) { color: var(--signal); }
  kbd { font: 11px var(--mono); padding: 1px 4px; border: 1px solid var(--line); border-radius: 3px; }
  .tog {
    display: inline-flex; align-items: center; gap: 9px; padding: 4px 8px; margin-left: -8px; border-radius: 5px; font-size: 13px;
    color: var(--ink); transition: background 0.15s;
  }
  .tog:hover { background: var(--line-2); }
  .sw { width: 24px; height: 14px; border-radius: 7px; background: var(--line); position: relative; transition: background 0.15s; flex: none; }
  .sw::after {
    content: ''; position: absolute; top: 2px; left: 2px; width: 10px; height: 10px; border-radius: 50%;
    background: var(--surface); transition: transform 0.15s;
  }
  .tog[aria-checked='true'] .sw { background: var(--signal); }
  .tog[aria-checked='true'] .sw::after { transform: translateX(10px); }
  @media (prefers-reduced-motion: reduce) {
    .sw, .sw::after, .tog { transition: none; }
  }
</style>
