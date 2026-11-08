<script lang="ts">
  import type { Backend } from '../lib/backend';
  import { whenLabel } from '../lib/browsers';
  import type { PairedClient } from '../lib/types';

  /** `rev` changes when a pairing was answered, so the list reloads. */
  let { backend, rev }: { backend: Backend; rev: number } = $props();

  let clients = $state<PairedClient[]>([]);

  async function load() {
    clients = await backend.pairedClients();
  }

  $effect(() => {
    void rev;
    void load();
  });

  async function remove(id: number) {
    await backend.removePairedClient(id);
    await load();
  }
</script>

<p class="help">Browsers with the Haul extension that may send downloads here. Removing one makes it ask again.</p>
{#if clients.length}
  <ul class="list" aria-label="Paired browsers">
    {#each clients as c (c.id)}
      <li>
        <span class="name">{c.browser}</span>
        <span class="meta">Paired {whenLabel(c.createdAt)} · Last used {whenLabel(c.lastSeen)}</span>
        <span class="sp"></span>
        <button type="button" class="btn sm" aria-label="Remove {c.browser}" onclick={() => remove(c.id)}>Remove</button>
      </li>
    {/each}
  </ul>
{:else}
  <p class="empty">No browsers paired. Install the Haul extension and press Pair in its popup.</p>
{/if}

<style>
  .help { font-size: 12px; color: var(--ink-3); margin-bottom: 10px; }
  .empty { font-size: 13px; color: var(--ink-2); }
  .list { list-style: none; margin: 0; padding: 0; display: grid; gap: 6px; }
  li { display: flex; align-items: center; gap: 10px; font-size: 13px; }
  .name { font-weight: 500; }
  .meta { color: var(--ink-2); font-size: 12px; }
  .sp { flex: 1; }
  .sm { padding: 3px 8px; font-size: 12px; }
</style>
