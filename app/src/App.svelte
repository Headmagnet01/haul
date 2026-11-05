<script lang="ts">
  import Rail, { VIEWS, type View } from './components/Rail.svelte';
  import Placeholder from './components/Placeholder.svelte';

  let view: View = $state('queue');
  const label = $derived(VIEWS.find((v) => v.id === view)?.label ?? '');
</script>

<div class="app" class:with-drawer={view === 'queue'}>
  <Rail {view} onselect={(v) => (view = v)} />
  {#if view === 'queue'}
    <Placeholder title="Queue" />
    <aside class="drawer" aria-label="Details"></aside>
  {:else}
    <Placeholder title={label} />
  {/if}
  <footer class="bar"></footer>
</div>

<style>
  .app {
    display: grid; grid-template-columns: 208px 1fr; grid-template-rows: minmax(0, 1fr) 40px;
    height: 100vh; min-width: 1080px; min-height: 0;
  }
  .app.with-drawer { grid-template-columns: 208px minmax(0, 1fr) 340px; }
  .drawer { grid-row: 1; border-left: 1px solid var(--line); background: var(--bg); min-width: 0; overflow: auto; }
  .bar {
    grid-column: 1 / -1; grid-row: 2; display: flex; align-items: center; gap: 18px; padding: 0 16px;
    border-top: 1px solid var(--line); background: var(--bg); font-size: 12.5px; color: var(--ink-2);
  }
</style>
