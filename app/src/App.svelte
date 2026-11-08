<script lang="ts">
  import { untrack } from 'svelte';
  import Rail, { VIEWS } from './components/Rail.svelte';
  import Placeholder from './components/Placeholder.svelte';
  import QueueView from './components/QueueView.svelte';
  import FinishedView from './components/FinishedView.svelte';
  import FinishedDrawer from './components/FinishedDrawer.svelte';
  import DeleteDialog from './components/DeleteDialog.svelte';
  import CountdownDialog from './components/CountdownDialog.svelte';
  import { createWhenDoneState } from './lib/when-done-state.svelte';
  import CollectorView from './components/CollectorView.svelte';
  import RulesView from './components/RulesView.svelte';
  import SettingsView from './components/SettingsView.svelte';
  import { createSettingsState } from './lib/settings-state.svelte';
  import { createRulesState } from './lib/rules-state.svelte';
  import Toast from './components/Toast.svelte';
  import PairDialog from './components/PairDialog.svelte';
  import Drawer from './components/Drawer.svelte';
  import StatusBar from './components/StatusBar.svelte';
  import { getBackend } from './lib/backend';
  import { createAppState } from './lib/app-state.svelte';
  import { createCollectorState } from './lib/collector-state.svelte';
  import { createPostprocState } from './lib/postproc-state.svelte';

  const app = createAppState(getBackend());
  const collector = createCollectorState(app.backend, { showToast: app.showToast, onStarted: () => void app.load() });
  // A run that sorted files moved them, so the Queue reloads its snapshot too.
  const post = createPostprocState(app.backend, { showToast: (t) => app.showToast(t), onChanged: () => void app.load() });
  app.onUpdate((u) => collector.onUpdate(u));
  app.onUpdate((u) => post.onUpdate(u));
  // Which rows are Finished depends on the runs.
  $effect(() => {
    app.runs = post.runs;
  });
  void app.load();
  void collector.load();
  void post.load();
  const wd = createWhenDoneState(app.backend, { showToast: (t) => app.showToast(t) });
  const rules = createRulesState(app.backend, { showToast: (t) => app.showToast(t) });
  $effect(() => {
    if (app.view === 'rules') void rules.load();
  });
  // The status bar's toggles and the Settings page are one setting; reload whichever didn't change.
  const settings = createSettingsState(app.backend, {
    onChanged: (p) => {
      if (p.watchClipboard !== undefined) void collector.load();
      if (p.speedLimit !== undefined) void app.load();
    },
  });
  $effect(() => {
    if (app.view === 'settings') void settings.load();
  });
  $effect(() => {
    const watchClipboard = collector.snapshot.watchClipboard;
    untrack(() => settings.mirror({ watchClipboard }));
  });
  $effect(() => {
    const speedLimit = { ...app.snapshot.speedLimit };
    untrack(() => settings.mirror({ speedLimit }));
  });

  // Bumped when a pairing is answered, so the Browsers list reloads.
  let browsersRev = $state(0);

  const label = $derived(VIEWS.find((v) => v.id === app.view)?.label ?? '');
  const active = $derived(
    app.snapshot.downloads.filter((d) => d.state !== 'completed' && !app.finished.has(d.id) && !app.hidden.has(d.id)).length,
  );

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

<div class="app" class:with-drawer={app.view === 'queue' || app.view === 'finished'}>
  <Rail
    view={app.view}
    onselect={(v) => (app.view = v)}
    queueCount={active}
    collectorCount={collector.count}
    finishedCount={app.finishedCount}
  />
  {#if app.view === 'queue'}
    <QueueView {app} {post} />
    <Drawer {app} {post} />
  {:else if app.view === 'finished'}
    <FinishedView {app} />
    <FinishedDrawer {app} {post} />
  {:else if app.view === 'collector'}
    <CollectorView c={collector} />
  {:else if app.view === 'rules'}
    <RulesView rs={rules} backend={app.backend} />
  {:else if app.view === 'settings'}
    <SettingsView ss={settings} backend={app.backend} {browsersRev} />
  {:else}
    <Placeholder title={label} />
  {/if}
  <StatusBar {app} {collector} {wd} />
</div>
<DeleteDialog {app} />
<CountdownDialog {wd} />
<PairDialog backend={app.backend} onanswered={() => browsersRev++} />
<Toast toast={app.toast} ondismiss={() => (app.toast = null)} />

<style>
  .app {
    display: grid; grid-template-columns: 208px 1fr; grid-template-rows: minmax(0, 1fr) 40px;
    height: 100vh; min-width: 1080px; min-height: 0;
  }
  .app.with-drawer { grid-template-columns: 208px minmax(0, 1fr) 340px; }
</style>
