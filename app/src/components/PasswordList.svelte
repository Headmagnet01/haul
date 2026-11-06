<script lang="ts">
  let {
    ids,
    onadd,
    onremove,
    onmove,
  }: {
    ids: number[];
    onadd: (pw: string) => Promise<boolean>;
    onremove: (id: number) => void;
    onmove: (i: number, by: -1 | 1) => void;
  } = $props();

  let pw = $state('');

  async function add(e: SubmitEvent) {
    e.preventDefault();
    if (!pw) return;
    if (await onadd(pw)) pw = '';
  }
</script>

<p class="help">Tried in this order when an archive needs a password. Stored encrypted for your Windows account and never shown again.</p>
{#if ids.length}
  <ul class="list" aria-label="Saved passwords">
    {#each ids as id, i (id)}
      <li>
        <span class="mask">•••••••• ({i + 1})</span>
        <span class="sp"></span>
        <button type="button" class="btn sm" disabled={i === 0} onclick={() => onmove(i, -1)}>Move up</button>
        <button type="button" class="btn sm" disabled={i === ids.length - 1} onclick={() => onmove(i, 1)}>Move down</button>
        <button type="button" class="btn sm" onclick={() => onremove(id)}>Delete</button>
      </li>
    {/each}
  </ul>
{/if}
<form class="add" onsubmit={add}>
  <label for="new-pw">New password</label>
  <input id="new-pw" type="password" autocomplete="off" bind:value={pw} />
  <button type="submit" class="btn" disabled={!pw}>Add password</button>
</form>

<style>
  .help { font-size: 12px; color: var(--ink-3); margin-bottom: 10px; }
  .list { list-style: none; margin: 0 0 10px; padding: 0; display: grid; gap: 6px; }
  li { display: flex; align-items: center; gap: 8px; font-size: 13px; }
  .mask { font-family: var(--mono); letter-spacing: 0.05em; }
  .sp { flex: 1; }
  .add { display: flex; align-items: center; gap: 8px; font-size: 12.5px; }
  .add label { color: var(--ink-2); }
  input {
    color: var(--ink); background: var(--raised); border: 1px solid var(--line); border-radius: 5px; padding: 4px 8px;
    font: inherit; width: 220px;
  }
  .sm { padding: 3px 8px; font-size: 12px; }
  .btn:disabled { opacity: 0.5; cursor: default; }
</style>
