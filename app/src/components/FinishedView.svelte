<script lang="ts">
  import { tick, untrack } from 'svelte';
  import Icon from './Icon.svelte';
  import type { AppState } from '../lib/app-state.svelte';
  import { finishedTime, locationOf } from '../lib/finished';
  import { formatBytes } from '../lib/format';
  import { keyAction } from '../lib/keys';
  import type { ListItem } from '../lib/model';

  let { app }: { app: AppState } = $props();

  let filterBox: HTMLInputElement | undefined = $state();
  let listEl: HTMLDivElement | undefined = $state();
  // Re-rendered each minute, so "14:32" turns into a date after midnight.
  let now = $state(new Date());
  $effect(() => {
    const t = setInterval(() => (now = new Date()), 60_000);
    return () => clearInterval(t);
  });

  // Which files are still on disk: checked when Finished opens and when a row is selected.
  $effect(() => {
    const ids = untrack(() => [...app.finished]);
    void app.checkFiles(ids);
  });
  $effect(() => {
    const ids = app.selectedPkg !== null ? app.childrenOf(app.selectedPkg).map((d) => d.id) : app.selected !== null ? [app.selected] : [];
    untrack(() => void app.checkFiles(ids));
  });

  const items = $derived(app.finishedList);
  const keyOf = (it: ListItem) => (it.kind === 'package' ? `p${it.pkg.id}` : `f${it.d.id}`);
  const isSel = (it: ListItem) => (it.kind === 'package' ? app.selectedPkg === it.pkg.id : app.selected === it.d.id);
  const selIndex = $derived(items.findIndex(isSel));
  const idsOf = (it: ListItem) => (it.kind === 'package' ? it.children.map((d) => d.id) : [it.d.id]);

  function row(it: ListItem) {
    if (it.kind === 'file') {
      const d = it.d;
      return {
        name: d.name, tag: null, size: formatBytes(d.size), when: finishedTime(d.finishedAt ?? d.createdAt, now),
        where: locationOf(d), missing: app.missing.has(d.id),
      };
    }
    const ch = it.children;
    const latest = ch.map((d) => d.finishedAt ?? d.createdAt).sort().at(-1) ?? null;
    return {
      name: it.pkg.name, tag: `${ch.length} file${ch.length === 1 ? '' : 's'}`,
      size: ch.every((c) => c.size !== null) ? formatBytes(ch.reduce((a, c) => a + (c.size ?? 0), 0)) : '—',
      when: finishedTime(latest, now), where: ch[0] ? locationOf(ch[0]) : '', missing: ch.length > 0 && ch.every((c) => app.missing.has(c.id)),
    };
  }

  function selectItem(it: ListItem) {
    if (it.kind === 'package') app.selectPackage(it.pkg.id);
    else app.select(it.d.id);
  }

  async function focusSelected() {
    await tick();
    listEl?.querySelector<HTMLElement>('[role="row"][aria-selected="true"]')?.focus();
  }

  function toggleOpen(id: number) {
    if (app.expanded.has(id)) app.expanded.delete(id);
    else app.expanded.add(id);
  }

  function onKey(e: KeyboardEvent) {
    if (document.querySelector('[aria-modal="true"]')) return;
    const action = keyAction({
      key: e.key, altKey: e.altKey, ctrlKey: e.ctrlKey || e.metaKey, shiftKey: e.shiftKey,
      target: e.target as HTMLElement | null,
    });
    if (!action) return;
    const tag = (e.target as HTMLElement | null)?.tagName;
    // Buttons and links handle their own Enter and Space.
    if ((action === 'open' || action === 'toggle') && (tag === 'BUTTON' || tag === 'A')) return;
    const it = items[selIndex];
    switch (action) {
      case 'focus-filter':
        filterBox?.focus();
        filterBox?.select();
        break;
      case 'up':
      case 'down': {
        if (!items.length) return;
        const i = selIndex < 0 ? (action === 'down' ? 0 : items.length - 1) : selIndex + (action === 'down' ? 1 : -1);
        selectItem(items[Math.max(0, Math.min(items.length - 1, i))]);
        void focusSelected();
        break;
      }
      case 'expand':
      case 'collapse':
        if (!it) return;
        if (it.kind === 'package') {
          if (action === 'expand') app.expanded.add(it.pkg.id);
          else app.expanded.delete(it.pkg.id);
        } else if (action === 'collapse' && it.child && it.d.packageId !== null) {
          app.selectPackage(it.d.packageId);
          void focusSelected();
        }
        break;
      case 'open':
        if (!it) return;
        if (it.kind === 'package') toggleOpen(it.pkg.id);
        else if (!app.missing.has(it.d.id)) void app.openFile(it.d.id);
        break;
      case 'remove': {
        if (!it) return;
        const next = items[selIndex + 1] ?? items[selIndex - 1];
        app.removeWithUndo(idsOf(it));
        if (next && !(next.kind === 'file' && app.hidden.has(next.d.id))) selectItem(next);
        else app.select(null);
        void focusSelected();
        break;
      }
      case 'delete-file':
        if (!it) return;
        app.askDelete(idsOf(it));
        break;
      default:
        return;
    }
    e.preventDefault();
  }
</script>

<svelte:window onkeydown={onKey} />

<main class="main">
  <div class="top">
    <h1>Finished</h1>
    <span class="sub">{app.finishedCount} file{app.finishedCount === 1 ? '' : 's'}</span>
    <span class="sp"></span>
    <label class="search">
      <Icon name="search" size={14} />
      <input
        bind:this={filterBox}
        type="search"
        placeholder="Filter finished"
        aria-label="Filter finished"
        value={app.finishedFilter}
        oninput={(e) => (app.finishedFilter = e.currentTarget.value)}
        onkeydown={(e) => {
          if (e.key === 'Escape') {
            app.finishedFilter = '';
            e.currentTarget.blur();
          }
        }}
      />
    </label>
  </div>

  {#if app.loaded && app.finishedCount === 0}
    <div class="empty">
      <span class="ic"><Icon name="finished" size={20} /></span>
      <p class="h">Nothing finished yet</p>
      <p class="s">Completed downloads move here from the Queue.</p>
    </div>
  {:else}
    <div class="grid" role="treegrid" aria-label="Finished downloads" aria-multiselectable="false">
      <div class="cols" role="row">
        <span role="columnheader">Name</span><span role="columnheader" class="r">Size</span>
        <span role="columnheader" class="r">Finished</span><span role="columnheader">Location</span>
      </div>
      <div class="list" bind:this={listEl} role="rowgroup">
        {#each items as it, i (keyOf(it))}
          {@const v = row(it)}
          <!-- svelte-ignore a11y_click_events_have_key_events (keys are handled by the treegrid) -->
          <div
            role="row"
            class="row"
            class:pkg={it.kind === 'package'}
            class:child={it.kind === 'file' && it.child}
            class:sel={isSel(it)}
            data-kind={it.kind}
            data-id={it.kind === 'file' ? it.d.id : it.pkg.id}
            aria-level={it.kind === 'file' && it.child ? 2 : 1}
            aria-expanded={it.kind === 'package' ? it.open : undefined}
            aria-selected={isSel(it)}
            tabindex={(selIndex < 0 ? i === 0 : selIndex === i) ? 0 : -1}
            onclick={() => selectItem(it)}
            ondblclick={() => {
              if (it.kind === 'file' && !v.missing) void app.openFile(it.d.id);
            }}
          >
            <div class="name" role="gridcell">
              {#if it.kind === 'package'}
                {@const id = it.pkg.id}
                <button
                  type="button"
                  class="chevbtn"
                  class:closed={!it.open}
                  tabindex="-1"
                  aria-label={it.open ? 'Collapse package' : 'Expand package'}
                  onclick={(e) => {
                    e.stopPropagation();
                    toggleOpen(id);
                  }}><Icon name="chev" /></button>
              {:else}
                <span class="ok"><Icon name="ok" size={13} /></span>
              {/if}
              <span class="t" title={v.name}>{v.name}</span>
              {#if v.tag}<span class="tag">{v.tag}</span>{/if}
              {#if v.missing}<span class="tag miss">Missing</span>{/if}
            </div>
            <span class="num r" role="gridcell">{v.size}</span>
            <span class="num r" role="gridcell">{v.when}</span>
            <span class="loc" role="gridcell" title={v.where}>{v.where}</span>
          </div>
        {/each}
        {#if items.length === 0}
          <p class="none">No finished downloads match “{app.finishedFilter}”.</p>
        {/if}
      </div>
    </div>
  {/if}
</main>

<style>
  .main { grid-row: 1; display: flex; flex-direction: column; min-width: 0; min-height: 0; background: var(--surface); }
  .top { display: flex; align-items: center; gap: 12px; padding: 14px 20px; border-bottom: 1px solid var(--line); min-width: 0; }
  h1 { font: 600 17px var(--sans); letter-spacing: -0.01em; }
  .sub { color: var(--ink-3); font-size: 12.5px; white-space: nowrap; }
  .sp { flex: 1 1 0; }
  .search {
    display: flex; align-items: center; gap: 8px; border: 1px solid var(--line); background: var(--raised);
    border-radius: 6px; padding: 5px 10px; flex: 0 1 220px; min-width: 120px; color: var(--ink-3); font-size: 13px;
  }
  .search:focus-within { border-color: var(--ink-3); }
  .search input { border: 0; background: none; outline: none; min-width: 0; flex: 1; font-size: 13px; color: var(--ink); }
  .search input::placeholder { color: var(--ink-3); }
  .grid { --cols: minmax(180px, 2fr) 80px 96px minmax(120px, 1.4fr); display: flex; flex-direction: column; min-height: 0; flex: 1; }
  .cols {
    display: grid; grid-template-columns: var(--cols);
    align-items: center; gap: 12px; padding: 0 20px; height: 32px; border-bottom: 1px solid var(--line);
    font: 500 11px var(--mono); text-transform: uppercase; letter-spacing: 0.07em; color: var(--ink-3);
  }
  .r { text-align: right; }
  .list { overflow: auto; flex: 1; min-height: 0; }
  .row {
    display: grid; grid-template-columns: var(--cols);
    align-items: center; gap: 12px; padding: 0 20px; min-height: 40px; border-bottom: 1px solid var(--line-2);
    cursor: default; transition: background 0.15s;
  }
  .row:hover { background: var(--line-2); }
  .row.sel { background: var(--sel); }
  .row:focus-visible { outline-offset: -2px; border-radius: 0; }
  .row.pkg { min-height: 44px; font-weight: 500; }
  .row.child .name { padding-left: 26px; }
  .name { display: flex; align-items: center; gap: 8px; min-width: 0; }
  .ok { display: grid; place-items: center; width: 16px; flex: none; color: var(--ok); }
  .t { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0; }
  .tag {
    font: 500 10.5px var(--mono); color: var(--ink-3); border: 1px solid var(--line); border-radius: 4px;
    padding: 0 5px; flex: none;
  }
  .tag.miss { color: var(--err); border-color: currentColor; }
  .chevbtn { display: grid; place-items: center; width: 16px; height: 16px; flex: none; color: var(--ink-3); }
  .chevbtn :global(svg) { transition: transform 0.15s; }
  .chevbtn.closed :global(svg) { transform: rotate(-90deg); }
  .num { color: var(--ink-2); }
  .loc { color: var(--ink-3); font-size: 12.5px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0; }
  .none { padding: 24px 20px; color: var(--ink-3); font-size: 13px; }
  .empty {
    flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 8px;
    padding: 40px 20px; text-align: center;
  }
  .ic { color: var(--ink-3); margin-bottom: 4px; }
  .h { font: 600 15px var(--sans); color: var(--ink); }
  .s { font-size: 13px; color: var(--ink-3); }
</style>
