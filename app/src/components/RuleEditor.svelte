<script lang="ts">
  import { untrack } from 'svelte';
  import { bytesToMb, mbToBytes, parseExts } from '../lib/rules';
  import type { Rule } from '../lib/types';

  let { rule, onsave, oncancel }: { rule: Rule; onsave: (r: Rule) => void; oncancel: () => void } = $props();

  // Form fields as text, copied once from the rule being edited; turned back into a rule on Save.
  const start = untrack(() => rule);
  let name = $state(start.name);
  let host = $state(start.match.host ?? '');
  let pattern = $state(start.match.name ?? '');
  let exts = $state(start.match.exts.join(', '));
  let minMb = $state(bytesToMb(start.match.minSize));
  let maxMb = $state(bytesToMb(start.match.maxSize));
  let folder = $state(start.action.folder ?? '');
  let rename = $state(start.action.rename ?? '');

  const opt = (s: string) => (s.trim() ? s.trim() : null);

  function save(e: SubmitEvent) {
    e.preventDefault();
    onsave({
      id: start.id,
      name: name.trim(),
      enabled: start.enabled,
      match: { host: opt(host), name: opt(pattern), exts: parseExts(exts), minSize: mbToBytes(minMb), maxSize: mbToBytes(maxMb) },
      action: { folder: opt(folder), rename: opt(rename) },
    });
  }
</script>

<form class="ed" role="group" aria-label="Rule editor" onsubmit={save}>
  <label>Name<input bind:value={name} autocomplete="off" /></label>
  <label>Host<input bind:value={host} placeholder="example.com or *.example.com" autocomplete="off" /></label>
  <label>File name<input bind:value={pattern} placeholder="*.iso or /regex/" autocomplete="off" spellcheck="false" /></label>
  <label>Extensions<input bind:value={exts} placeholder="iso, img" autocomplete="off" /></label>
  <label>Min size (MB)<input bind:value={minMb} inputmode="decimal" autocomplete="off" /></label>
  <label>Max size (MB)<input bind:value={maxMb} inputmode="decimal" autocomplete="off" /></label>
  <label>Folder<input bind:value={folder} placeholder="Programs\ISO or D:\Somewhere" autocomplete="off" /></label>
  <label>Rename to<input bind:value={rename} placeholder={'{name}.{ext}'} autocomplete="off" spellcheck="false" /></label>
  <p class="help">Tokens: {'{name}'} {'{ext}'} {'{package}'} {'{host}'} {'{date}'}. Empty fields match everything.</p>
  <div class="acts">
    <button type="submit" class="btn primary">Save</button>
    <button type="button" class="btn" onclick={oncancel}>Cancel</button>
  </div>
</form>

<style>
  .ed {
    display: grid; grid-template-columns: 1fr 1fr; gap: 10px 16px; padding: 14px; margin: 8px 0;
    border: 1px solid var(--line); border-radius: 8px; background: var(--bg);
  }
  label { display: grid; gap: 4px; font-size: 12px; color: var(--ink-2); }
  input {
    color: var(--ink); background: var(--raised); border: 1px solid var(--line); border-radius: 5px; padding: 5px 8px;
    font: 12.5px var(--sans);
  }
  .help { grid-column: 1 / -1; font-size: 12px; color: var(--ink-3); }
  .acts { grid-column: 1 / -1; display: flex; gap: 8px; }
</style>
