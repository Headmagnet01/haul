<script lang="ts">
  let { onadd, oncancel }: { onadd: (text: string) => void | Promise<void>; oncancel: () => void } = $props();
  let text = $state('');
  let box: HTMLTextAreaElement | undefined = $state();

  $effect(() => {
    box?.focus();
  });

  async function submit() {
    await onadd(text);
    text = '';
  }
</script>

<div class="add">
  <textarea
    bind:this={box}
    bind:value={text}
    aria-label="Links to add"
    placeholder="Paste links, one per line or mixed with text"
    rows="3"
    onkeydown={(e) => {
      if (e.key === 'Escape') oncancel();
      if (e.key === 'Enter' && e.ctrlKey) void submit();
    }}
  ></textarea>
  <div class="acts">
    <button type="button" class="btn primary" onclick={submit}>Add</button>
    <button type="button" class="btn" onclick={oncancel}>Cancel</button>
  </div>
</div>

<style>
  .add { display: flex; gap: 12px; padding: 12px 20px; border-bottom: 1px solid var(--line); background: var(--bg); }
  textarea {
    flex: 1; min-width: 0; resize: vertical; border: 1px solid var(--line); background: var(--raised); border-radius: 6px;
    padding: 8px 10px; font: 13px/1.4 var(--mono); color: var(--ink);
  }
  textarea::placeholder { color: var(--ink-3); }
  .acts { display: flex; flex-direction: column; gap: 6px; }
</style>
