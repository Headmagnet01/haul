<script lang="ts">
  import Rail, { VIEWS } from './components/Rail.svelte';
  import Placeholder from './components/Placeholder.svelte';
  import QueueView from './components/QueueView.svelte';
  import CollectorView from './components/CollectorView.svelte';
  import Toast from './components/Toast.svelte';
  import Drawer from './components/Drawer.svelte';
  import StatusBar from './components/StatusBar.svelte';
  import { getBackend } from './lib/backend';
  import { createAppState } from './lib/app-state.svelte';
  import { createCollectorState } from './lib/collector-state.svelte';

  const app = createAppState(getBackend());
  const collector = createCollectorState(app.backend, { showToast: app.showToast, onStarted: () => void app.load() });
  app.onUpdate((u) => collector.onUpdate(u));
  void app.load();
  void collector.load();

  const label = $derived(VIEWS.find((v) => v.id === app.view)?.label ?? '');
  const active = $derived(app.snapshot.downloads.filter((d) => d.state !== 'completed' && !app.hidden.has(d.id)).length);

  // Paste and drop work in every view; links go to the Collector.
  const ROW_TYPE = 'application/x-haul-row';
  function isTextField(t: EventTarget | null) {
    const el = t as HTMLElement | null;
    return !!el && (el.tagName === 'INPUT' || el.tagName === 'TEXTAREA' || el.isContentEditable);
  }
  function onPaste(e: ClipboardEvent) {
    if (isTextField(e.target)) return;
    const text = e.clipboardData?.getData('text/plain') ?? '';
    e.preventDefault();
    void app.addText(text, 'paste');
  }
  function droppable(e: DragEvent) {
    const types = e.dataTransfer?.types ?? [];
    return !types.includes(ROW_TYPE) && (types.includes('text/plain') || types.includes('text/uri-list') || types.includes('Files'));
  }
  function onDragOver(e: DragEvent) {
    if (droppable(e)) e.preventDefault();
  }
  async function onDrop(e: DragEvent) {
    if (!droppable(e) || isTextField(e.target)) return;
    e.preventDefault();
    const dt = e.dataTransfer!;
    let text = dt.getData('text/uri-list') || dt.getData('text/plain');
    for (const f of Array.from(dt.files)) {
      if (f.name.toLowerCase().endsWith('.txt') || f.type === 'text/plain') text += '\n' + (await f.text());
    }
    void app.addText(text, 'drop');
  }
</script>

<svelte:window ondragover={onDragOver} ondrop={onDrop} />
<svelte:document onpaste={onPaste} />

<div class="app" class:with-drawer={app.view === 'queue'}>
  <Rail view={app.view} onselect={(v) => (app.view = v)} queueCount={active} collectorCount={collector.count} />
  {#if app.view === 'queue'}
    <QueueView {app} />
    <Drawer {app} />
  {:else if app.view === 'collector'}
    <CollectorView c={collector} />
  {:else}
    <Placeholder title={label} />
  {/if}
  <StatusBar {app} {collector} />
</div>
<Toast toast={app.toast} ondismiss={() => (app.toast = null)} />

<style>
  .app {
    display: grid; grid-template-columns: 208px 1fr; grid-template-rows: minmax(0, 1fr) 40px;
    height: 100vh; min-width: 1080px; min-height: 0;
  }
  .app.with-drawer { grid-template-columns: 208px minmax(0, 1fr) 340px; }
</style>
