<script lang="ts">
  import { tick } from 'svelte';
  import Icon from './Icon.svelte';
  import ListRow from './ListRow.svelte';
  import AddLinks from './AddLinks.svelte';
  import EmptyState from './EmptyState.svelte';
  import type { AppState } from '../lib/app-state.svelte';
  import { keyAction, type Action } from '../lib/keys';
  import type { ListItem } from '../lib/model';

  let { app, onaction }: { app: AppState; onaction?: (a: Action) => boolean } = $props();

  let adding = $state(false);
  let filterBox: HTMLInputElement | undefined = $state();
  let listEl: HTMLDivElement | undefined = $state();

  const sub = $derived.by(() => {
    const t = app.totals;
    const parts = [
      t.downloading ? `${t.downloading} downloading` : '',
      t.waiting ? `${t.waiting} waiting` : '',
      t.attention ? `${t.attention} needs attention` : '',
    ].filter(Boolean);
    return parts.length ? parts.join(' · ') : 'Nothing queued';
  });

  const keyOf = (it: ListItem) => (it.kind === 'package' ? `p${it.pkg.id}` : `f${it.d.id}`);
  const isSel = (it: ListItem) =>
    it.kind === 'package' ? app.selectedPkg === it.pkg.id : app.selected === it.d.id;
  const selIndex = $derived(app.list.findIndex(isSel));

  function selectItem(it: ListItem) {
    if (it.kind === 'package') app.selectPackage(it.pkg.id);
    else app.select(it.d.id);
  }

  async function focusSelected() {
    await tick();
    listEl?.querySelector<HTMLElement>('[role="row"][aria-selected="true"]')?.focus();
  }

  function isTextField(t: EventTarget | null) {
    const el = t as HTMLElement | null;
    return !!el && (el.tagName === 'INPUT' || el.tagName === 'TEXTAREA' || el.isContentEditable);
  }

  function onKey(e: KeyboardEvent) {
    const action = keyAction({
      key: e.key, altKey: e.altKey, ctrlKey: e.ctrlKey || e.metaKey, shiftKey: e.shiftKey,
      target: e.target as HTMLElement | null,
    });
    if (!action) return;
    const tag = (e.target as HTMLElement | null)?.tagName;
    // Let buttons and links handle their own Space.
    if (action === 'toggle' && (tag === 'BUTTON' || tag === 'A')) return;
    const list = app.list;
    switch (action) {
      case 'focus-filter':
        filterBox?.focus();
        filterBox?.select();
        break;
      case 'up':
      case 'down': {
        if (!list.length) return;
        const i = selIndex < 0 ? (action === 'down' ? 0 : list.length - 1) : selIndex + (action === 'down' ? 1 : -1);
        selectItem(list[Math.max(0, Math.min(list.length - 1, i))]);
        void focusSelected();
        break;
      }
      case 'toggle':
        if (selIndex < 0) return;
        void app.toggle();
        break;
      case 'expand':
      case 'collapse': {
        const it = list[selIndex];
        if (!it) return;
        if (it.kind === 'package') app.setCollapsed(it.pkg.id, action === 'collapse');
        else if (action === 'collapse' && it.child && it.d.packageId !== null) {
          app.selectPackage(it.d.packageId);
          void focusSelected();
        }
        break;
      }
      case 'remove': {
        const it = list[selIndex];
        if (!it) return;
        const next = list[selIndex + 1] ?? list[selIndex - 1];
        app.removeSelected();
        if (next && next !== it) {
          // Select the row that takes this one's place, unless it was removed with it (a package's child).
          const gone = (x: ListItem) => x.kind === 'file' && app.hidden.has(x.d.id);
          const after = app.list.find((x) => keyOf(x) === keyOf(next) && !gone(x));
          if (after) selectItem(after);
          else app.select(null);
        } else app.select(null);
        void focusSelected();
        break;
      }
      case 'move-up':
      case 'move-down':
        if (selIndex < 0) return;
        void app.move(action === 'move-up' ? -1 : 1).then(focusSelected);
        break;
      default:
        if (!onaction?.(action)) return;
    }
    e.preventDefault();
  }

  function onPaste(e: ClipboardEvent) {
    if (isTextField(e.target)) return;
    const text = e.clipboardData?.getData('text/plain') ?? '';
    e.preventDefault();
    void app.addText(text);
  }

  const ROW_TYPE = 'application/x-haul-row';
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
    void app.addText(text);
  }
</script>

<svelte:window onkeydown={onKey} ondragover={onDragOver} ondrop={onDrop} />
<svelte:document onpaste={onPaste} />

<main class="main">
  <div class="top">
    <h1>Queue</h1>
    <span class="sub">{sub}</span>
    <span class="sp"></span>
    <label class="search">
      <Icon name="search" size={14} />
      <input
        bind:this={filterBox}
        type="search"
        placeholder="Filter downloads"
        aria-label="Filter downloads"
        value={app.filter}
        oninput={(e) => (app.filter = e.currentTarget.value)}
        onkeydown={(e) => {
          if (e.key === 'Escape') {
            app.filter = '';
            e.currentTarget.blur();
          }
        }}
      />
    </label>
    <button type="button" class="btn" onclick={() => app.backend.pauseAll()}>Pause all</button>
    <button type="button" class="btn primary" aria-expanded={adding} onclick={() => (adding = !adding)}>
      <Icon name="plus" size={14} />Add links<kbd>Ctrl V</kbd>
    </button>
  </div>
  {#if adding}
    <AddLinks
      onadd={async (t) => {
        const r = await app.addText(t);
        if (r.added > 0) adding = false;
      }}
      oncancel={() => (adding = false)}
    />
  {/if}

  {#if app.loaded && app.snapshot.downloads.length === 0}
    <EmptyState />
  {:else}
    <div class="grid" role="treegrid" aria-label="Downloads" aria-multiselectable="false">
      <div class="cols" role="row">
        <span role="columnheader">Name</span><span role="columnheader" class="r">Size</span>
        <span role="columnheader">Progress</span><span role="columnheader" class="r">Speed</span>
        <span role="columnheader" class="r">Left</span><span role="columnheader" class="r">Status</span>
      </div>
      <div class="list" bind:this={listEl} role="rowgroup">
        {#each app.list as it, i (keyOf(it))}
          <ListRow
            item={it}
            speeds={app.speeds}
            selected={isSel(it)}
            focusable={selIndex < 0 ? i === 0 : selIndex === i}
            onselect={() => selectItem(it)}
            ondropped={(dragged) =>
              app.drop(dragged, it.kind === 'package' ? { kind: 'package', id: it.pkg.id } : { kind: 'file', id: it.d.id })}
            ontogglepkg={it.kind === 'package' ? () => app.setCollapsed(it.pkg.id, it.open) : undefined}
          />
        {/each}
        {#if app.list.length === 0}
          <p class="none">No downloads match “{app.filter}”.</p>
        {/if}
      </div>
    </div>
  {/if}
</main>

<style>
  .main { grid-row: 1; display: flex; flex-direction: column; min-width: 0; min-height: 0; background: var(--surface); }
  .top { display: flex; align-items: center; gap: 12px; padding: 14px 20px; border-bottom: 1px solid var(--line); min-width: 0; }
  h1 { font: 600 17px var(--sans); letter-spacing: -0.01em; }
  .sub { color: var(--ink-3); font-size: 12.5px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; min-width: 0; flex: 0 1 auto; }
  .grid { --cols: minmax(150px, 1.6fr) 72px minmax(120px, 1.3fr) 78px 58px 112px; }
  .sp { flex: 1 1 0; }
  .search {
    display: flex; align-items: center; gap: 8px; border: 1px solid var(--line); background: var(--raised);
    border-radius: 6px; padding: 5px 10px; flex: 0 1 220px; min-width: 120px; color: var(--ink-3); font-size: 13px;
  }
  .search:focus-within { border-color: var(--ink-3); }
  .search input { border: 0; background: none; outline: none; min-width: 0; flex: 1; font-size: 13px; color: var(--ink); }
  .search input::placeholder { color: var(--ink-3); }
  .btn { flex: none; }
  .grid { display: flex; flex-direction: column; min-height: 0; flex: 1; }
  .cols {
    display: grid; grid-template-columns: var(--cols);
    align-items: center; gap: 12px; padding: 0 20px; height: 32px; border-bottom: 1px solid var(--line);
    font: 500 11px var(--mono); text-transform: uppercase; letter-spacing: 0.07em; color: var(--ink-3);
  }
  .r { text-align: right; }
  .list { overflow: auto; flex: 1; min-height: 0; }
  .none { padding: 24px 20px; color: var(--ink-3); font-size: 13px; }
</style>
