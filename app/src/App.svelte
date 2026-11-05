<script lang="ts">
  import Rail, { VIEWS, type View } from './components/Rail.svelte';
  import Placeholder from './components/Placeholder.svelte';
  import QueueView from './components/QueueView.svelte';
  import Toast from './components/Toast.svelte';
  import Drawer from './components/Drawer.svelte';
  import StatusBar from './components/StatusBar.svelte';
  import { getBackend } from './lib/backend';
  import { createAppState } from './lib/app-state.svelte';

  const app = createAppState(getBackend());
  void app.load();

  let view: View = $state('queue');
  const label = $derived(VIEWS.find((v) => v.id === view)?.label ?? '');
  const active = $derived(app.snapshot.downloads.filter((d) => d.state !== 'completed' && !app.hidden.has(d.id)).length);
</script>

<div class="app" class:with-drawer={view === 'queue'}>
  <Rail {view} onselect={(v) => (view = v)} queueCount={active} />
  {#if view === 'queue'}
    <QueueView {app} />
    <Drawer {app} />
  {:else}
    <Placeholder title={label} />
  {/if}
  <StatusBar {app} />
</div>
<Toast toast={app.toast} ondismiss={() => (app.toast = null)} />

<style>
  .app {
    display: grid; grid-template-columns: 208px 1fr; grid-template-rows: minmax(0, 1fr) 40px;
    height: 100vh; min-width: 1080px; min-height: 0;
  }
  .app.with-drawer { grid-template-columns: 208px minmax(0, 1fr) 340px; }
</style>
