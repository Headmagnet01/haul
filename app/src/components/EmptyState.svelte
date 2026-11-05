<script lang="ts">
  import Icon from './Icon.svelte';
</script>

<div class="empty">
  <span class="ic"><Icon name="link" size={20} /></span>
  <p class="h">Copy a link or drop one here</p>
  <p class="s">Press Ctrl+V anywhere to add links.</p>
</div>

<style>
  .empty {
    flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 8px;
    padding: 40px 20px; text-align: center;
  }
  .ic { color: var(--ink-3); margin-bottom: 4px; }
  .h { font: 600 15px var(--sans); color: var(--ink); }
  .s { font-size: 13px; color: var(--ink-3); }
</style>
