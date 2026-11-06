<script lang="ts">
  import { parseExts } from '../lib/rules';
  import type { Category } from '../lib/types';

  let { categories, onsave }: { categories: Category[]; onsave: (c: Category[]) => void } = $props();

  // Editable text per category; rebuilt when the saved list changes.
  let rows = $state<{ name: string; exts: string; folder: string }[]>([]);
  $effect(() => {
    rows = categories.map((c) => ({ name: c.name, exts: c.exts.join(', '), folder: c.folder }));
  });

  function save(e: SubmitEvent) {
    e.preventDefault();
    onsave(rows.map((r) => ({ name: r.name, exts: parseExts(r.exts), folder: r.folder.trim() || r.name })));
  }
</script>

<form onsubmit={save}>
  <table>
    <thead><tr><th scope="col">Category</th><th scope="col">Extensions</th><th scope="col">Folder</th></tr></thead>
    <tbody>
      {#each rows as r (r.name)}
        <tr>
          <th scope="row">{r.name}</th>
          <td><input aria-label="{r.name} extensions" bind:value={r.exts} placeholder="Everything else" autocomplete="off" /></td>
          <td><input aria-label="{r.name} folder" bind:value={r.folder} autocomplete="off" /></td>
        </tr>
      {/each}
    </tbody>
  </table>
  <button type="submit" class="btn">Save categories</button>
</form>

<style>
  table { width: 100%; border-collapse: collapse; font-size: 12.5px; margin-bottom: 10px; }
  th { text-align: left; font-weight: 500; color: var(--ink-3); padding: 4px 8px 6px 0; }
  tbody th { color: var(--ink); width: 110px; }
  td { padding: 3px 8px 3px 0; }
  td:last-child { width: 30%; }
  input {
    width: 100%; color: var(--ink); background: var(--raised); border: 1px solid var(--line); border-radius: 5px;
    padding: 4px 8px; font: 12px var(--sans);
  }
</style>
