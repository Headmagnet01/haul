<script lang="ts">
  import Icon from './Icon.svelte';
  import type { CItem } from '../lib/collector';
  import { dupLabel, linkStatusWord, packageStatus, packageTag, sourceLabel, startableIds } from '../lib/collector';
  import { formatBytes } from '../lib/format';
  import type { CollectedLink, CollectorFilters } from '../lib/types';

  let {
    item,
    filters,
    selected,
    open,
    focusable,
    onselect,
    ontoggle,
    onstart,
    onrecheck,
  }: {
    item: CItem;
    filters: CollectorFilters;
    selected: boolean;
    open: boolean;
    focusable: boolean;
    onselect: () => void;
    ontoggle?: () => void;
    onstart: () => void;
    onrecheck?: (id: number) => void;
  } = $props();

  const toneOf = (l: CollectedLink) =>
    l.status === 'online' ? 'ok' : l.status === 'checking' ? 'checking' : 'err';

  const view = $derived.by(() => {
    if (item.kind === 'package') {
      const known = item.visible.filter((l) => l.size !== null);
      const st = packageStatus(item.visible);
      return {
        name: item.pkg.name,
        tag: packageTag(item.pkg, item.links),
        size: known.length ? formatBytes(known.reduce((a, l) => a + (l.size ?? 0), 0)) : '—',
        word: st.word,
        tone: st.tone,
        title: undefined as string | undefined,
        source: sourceLabel(item.pkg.source),
        dup: null as string | null,
        canStart: startableIds(item.visible, filters).length > 0,
        recheck: false,
      };
    }
    const l = item.link;
    return {
      name: l.name,
      tag: null,
      size: formatBytes(l.size),
      word: linkStatusWord(l),
      tone: toneOf(l),
      title: l.status === 'error' ? (l.message ?? undefined) : undefined,
      source: sourceLabel(l.source),
      dup: l.duplicate ? dupLabel(l.duplicate) : null,
      canStart: !item.child && l.status !== 'offline' && l.status !== 'expired',
      recheck: l.status === 'error',
    };
  });
</script>

<!-- svelte-ignore a11y_click_events_have_key_events (keys are handled by the treegrid) -->
<div
  role="row"
  class="crow"
  class:pkg={item.kind === 'package'}
  class:child={item.kind === 'link' && item.child}
  class:sel={selected}
  data-kind={item.kind}
  aria-level={item.kind === 'link' && item.child ? 2 : 1}
  aria-expanded={item.kind === 'package' ? open : undefined}
  aria-selected={selected}
  tabindex={focusable ? 0 : -1}
  onclick={onselect}
>
  <div class="name" role="gridcell">
    {#if item.kind === 'package'}
      <button
        type="button"
        class="chevbtn"
        class:closed={!open}
        tabindex="-1"
        aria-label={open ? 'Collapse package' : 'Expand package'}
        onclick={(e) => {
          e.stopPropagation();
          ontoggle?.();
        }}><Icon name="chev" /></button>
    {/if}
    <span class="t" title={view.name}>{view.name}</span>
    {#if view.tag}<span class="tag">{view.tag}</span>{/if}
    {#if view.dup}<span class="dupe">{view.dup}</span>{/if}
  </div>
  <span class="num size r" role="gridcell">{view.size}</span>
  <span class="on" role="gridcell">
    <span class="dot {view.tone}" aria-hidden="true"></span><span title={view.title}>{view.word}</span>
    {#if view.recheck && item.kind === 'link'}
      {@const id = item.link.id}
      <button type="button" class="again" onclick={(e) => { e.stopPropagation(); onrecheck?.(id); }}>Check again</button>
    {/if}
  </span>
  <span class="src" role="gridcell">{view.source}</span>
  <span class="act r" role="gridcell">
    {#if view.canStart}
      <button type="button" class="btn" onclick={(e) => { e.stopPropagation(); onstart(); }}>
        {item.kind === 'package' ? 'Start package' : 'Start'}
      </button>
    {/if}
  </span>
</div>

<style>
  .crow {
    display: grid; grid-template-columns: var(--ccols); align-items: center; gap: 14px; padding: 0 20px;
    min-height: 40px; border-bottom: 1px solid var(--line-2); cursor: default; transition: background 0.15s;
  }
  .crow:hover { background: var(--line-2); }
  .crow.pkg { min-height: 46px; font-weight: 500; background: var(--bg); }
  .crow.pkg:hover { background: var(--line-2); }
  .crow.sel, .crow.pkg.sel { background: var(--sel); }
  .crow:focus-visible { outline-offset: -2px; border-radius: 0; }
  .crow.child .name { padding-left: 26px; }
  .name { display: flex; align-items: center; gap: 8px; min-width: 0; }
  .t { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0; }
  .tag {
    font: 500 10.5px var(--mono); color: var(--ink-3); border: 1px solid var(--line); border-radius: 4px;
    padding: 0 5px; flex: none;
  }
  .dupe {
    font: 500 10.5px var(--mono); color: var(--err); border: 1px solid currentColor; border-radius: 4px;
    padding: 0 5px; flex: none; white-space: nowrap;
  }
  .chevbtn { display: grid; place-items: center; width: 16px; height: 16px; flex: none; color: var(--ink-3); }
  .chevbtn :global(svg) { transition: transform 0.15s; }
  .chevbtn.closed :global(svg) { transform: rotate(-90deg); }
  .size { color: var(--ink-2); }
  .r { text-align: right; }
  .on { display: inline-flex; gap: 6px; align-items: center; font-size: 12.5px; font-weight: 400; color: var(--ink-2); min-width: 0; flex-wrap: wrap; }
  .dot { width: 7px; height: 7px; border-radius: 50%; flex: none; background: var(--ok); }
  .dot.err { background: var(--err); }
  .dot.checking { background: var(--idle); animation: pulse 1s ease-in-out infinite; }
  @keyframes pulse { 50% { opacity: 0.3; } }
  .again { font-size: 12px; color: var(--ink); text-decoration: underline; text-underline-offset: 2px; }
  .src { font: 500 11px var(--mono); color: var(--ink-3); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .act .btn { padding: 4px 10px; font-size: 12.5px; }
</style>
