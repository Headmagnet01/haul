<script lang="ts">
  import { tick } from 'svelte';
  import Icon from './Icon.svelte';
  import CollectorRow from './CollectorRow.svelte';
  import FilterChips from './FilterChips.svelte';
  import type { CollectorState, CSel } from '../lib/collector-state.svelte';
  import type { CItem } from '../lib/collector';
  import { onlineIds } from '../lib/collector';
  import { keyAction } from '../lib/keys';

  let { c }: { c: CollectorState } = $props();
  let listEl: HTMLDivElement | undefined = $state();

  const keyOf = (it: CItem) => (it.kind === 'package' ? `p${it.pkg.key}` : `l${it.link.id}`);
  const selOf = (it: CItem): CSel => (it.kind === 'package' ? { kind: 'package', key: it.pkg.key } : { kind: 'link', id: it.link.id });
  const isSel = (it: CItem) => {
    const s = c.selected;
    if (!s) return false;
    return it.kind === 'package' ? s.kind === 'package' && s.key === it.pkg.key : s.kind === 'link' && s.id === it.link.id;
  };
  const selIndex = $derived(c.items.findIndex(isSel));
  const hasOffline = $derived(
    c.snapshot.links.some((l) => !c.removed.has(l.id) && (l.status === 'offline' || l.status === 'expired')),
  );
  const canStartAll = $derived(onlineIds(c.snapshot, c.removed).length > 0);
  const empty = $derived(c.loaded && c.count === 0);

  async function focusSelected() {
    await tick();
    listEl?.querySelector<HTMLElement>('[role="row"][aria-selected="true"]')?.focus();
  }

  function onKey(e: KeyboardEvent) {
    const target = e.target as HTMLElement | null;
    const tag = target?.tagName;
    if (tag === 'BUTTON' || tag === 'A') {
      if (e.key === 'Enter' || e.key === ' ') return;
    }
    const items = c.items;
    if (e.key === 'Enter' && !e.ctrlKey && !e.altKey && tag !== 'INPUT' && tag !== 'TEXTAREA') {
      if (selIndex < 0 || e.repeat) return;
      e.preventDefault();
      void c.startSelected();
      return;
    }
    const action = keyAction({
      key: e.key, altKey: e.altKey, ctrlKey: e.ctrlKey || e.metaKey, shiftKey: e.shiftKey, target,
    });
    switch (action) {
      case 'up':
      case 'down': {
        if (!items.length) return;
        const i = selIndex < 0 ? (action === 'down' ? 0 : items.length - 1) : selIndex + (action === 'down' ? 1 : -1);
        c.select(selOf(items[Math.max(0, Math.min(items.length - 1, i))]));
        void focusSelected();
        break;
      }
      case 'expand':
      case 'collapse': {
        const it = items[selIndex];
        if (!it) return;
        if (it.kind === 'package') c.setCollapsed(it.pkg.key, action === 'collapse');
        else if (action === 'collapse' && it.child && it.link.packageKey) {
          c.select({ kind: 'package', key: it.link.packageKey });
          void focusSelected();
        }
        break;
      }
      case 'remove': {
        const it = items[selIndex];
        if (!it) return;
        const next = items[selIndex + 1] ?? items[selIndex - 1];
        c.removeSelected();
        const after = next && c.items.find((x) => keyOf(x) === keyOf(next));
        c.select(after ? selOf(after) : null);
        void focusSelected();
        break;
      }
      default:
        return;
    }
    e.preventDefault();
  }
</script>

<svelte:window onkeydown={onKey} />

<main class="main">
  <div class="top">
    <h1>Collector</h1>
    <span class="sub">{c.sub}</span>
    <span class="sp"></span>
    <button type="button" class="btn" disabled={!hasOffline} onclick={() => c.clearOffline()}>Clear offline</button>
    <button type="button" class="btn primary" disabled={!canStartAll} onclick={() => c.startAllOnline()}>Start all online</button>
  </div>
  <FilterChips {c} />

  {#if empty}
    <div class="empty">
      <span class="ic"><Icon name="collector" size={20} /></span>
      <p class="s">Nothing waiting. Turn on Watch clipboard and copy links, or paste them anywhere in Haul.</p>
    </div>
  {:else}
    <div class="grid" role="treegrid" aria-label="Collector links">
      <div class="ccols" role="row">
        <span role="columnheader">Name</span><span role="columnheader" class="r">Size</span>
        <span role="columnheader">Online</span><span role="columnheader">Source</span>
        <span role="columnheader" class="r"><span class="vh">Actions</span></span>
      </div>
      <div class="list" bind:this={listEl} role="rowgroup">
        {#each c.items as it, i (keyOf(it))}
          <CollectorRow
            item={it}
            filters={c.snapshot.filters}
            selected={isSel(it)}
            open={it.kind === 'package' ? !c.collapsed.has(it.pkg.key) : true}
            focusable={selIndex < 0 ? i === 0 : selIndex === i}
            onselect={() => c.select(selOf(it))}
            ontoggle={it.kind === 'package'
              ? () => c.setCollapsed(it.pkg.key, !c.collapsed.has(it.pkg.key))
              : undefined}
            onstart={() => (it.kind === 'package' ? c.startPackage(it.pkg.key) : c.startLink(it.link.id))}
            onrecheck={(id) => c.recheck(id)}
          />
        {/each}
        {#if c.items.length === 0}
          <p class="none">Every waiting link is hidden by the filters above.</p>
        {/if}
      </div>
    </div>
  {/if}
  <div class="cfoot">Copied and pasted links wait here for review. Start a package to download it.</div>
</main>

<style>
  .main {
    grid-row: 1; display: flex; flex-direction: column; min-width: 0; min-height: 0; background: var(--surface);
    --ccols: minmax(260px, 2fr) 92px 130px 110px 140px;
  }
  .top { display: flex; align-items: center; gap: 12px; padding: 14px 20px; border-bottom: 1px solid var(--line); min-width: 0; }
  h1 { font: 600 17px var(--sans); letter-spacing: -0.01em; }
  .sub { color: var(--ink-3); font-size: 12.5px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; min-width: 0; flex: 0 1 auto; }
  .sp { flex: 1 1 0; }
  .btn { flex: none; }
  .btn:disabled { opacity: 0.5; cursor: default; }
  .btn:disabled:hover { border-color: var(--line); }
  .btn.primary:disabled:hover { border-color: var(--ink); }
  .grid { display: flex; flex-direction: column; min-height: 0; flex: 1; overflow-x: auto; }
  .ccols {
    display: grid; grid-template-columns: var(--ccols); align-items: center; gap: 14px; padding: 0 20px; height: 32px;
    border-bottom: 1px solid var(--line); font: 500 11px var(--mono); text-transform: uppercase; letter-spacing: 0.07em;
    color: var(--ink-3); flex: none;
  }
  .r { text-align: right; }
  .vh { position: absolute; width: 1px; height: 1px; overflow: hidden; clip-path: inset(50%); white-space: nowrap; }
  .list { overflow: auto; flex: 1; min-height: 0; }
  .none { padding: 24px 20px; color: var(--ink-3); font-size: 13px; }
  .empty {
    flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 8px;
    padding: 40px 20px; text-align: center;
  }
  .ic { color: var(--ink-3); margin-bottom: 4px; }
  .s { font-size: 13px; color: var(--ink-3); max-width: 420px; }
  .cfoot {
    display: flex; align-items: center; gap: 10px; padding: 12px 20px; border-top: 1px solid var(--line);
    font-size: 12.5px; color: var(--ink-3); flex: none;
  }
</style>
