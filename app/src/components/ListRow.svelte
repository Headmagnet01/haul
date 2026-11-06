<script lang="ts">
  import Icon from './Icon.svelte';
  import Strip from './Strip.svelte';
  import StatusCell from './StatusCell.svelte';
  import type { ListItem, Sel } from '../lib/model';
  import type { Status } from '../lib/status';
  import { statusOf } from '../lib/status';
  import { runStatus } from '../lib/postproc';
  import type { Run } from '../lib/types';
  import { packageCells, stripCells, stripKind } from '../lib/strip';
  import { etaSeconds, formatBytes, formatEta, formatProgress, formatSpeed } from '../lib/format';

  let {
    item,
    speeds,
    selected,
    focusable,
    onselect,
    ontogglepkg,
    ondropped,
    run = null,
  }: {
    item: ListItem;
    speeds: Map<number, number>;
    selected: boolean;
    focusable: boolean;
    onselect: () => void;
    ontogglepkg?: () => void;
    ondropped?: (dragged: Sel) => void;
    /** This row's after-download run, when it has one. */
    run?: Run | null;
  } = $props();

  const ROW_TYPE = 'application/x-haul-row';
  let over = $state(false);
  const me = $derived<Sel>(item.kind === 'package' ? { kind: 'package', id: item.pkg.id } : { kind: 'file', id: item.d.id });

  function pkgStatus(ch: { state: string }[]): Status {
    const done = ch.filter((c) => c.state === 'completed').length;
    const word = `${done} of ${ch.length} done`;
    if (ch.some((c) => c.state === 'running')) return { icon: 'dl', word, tone: 'signal' };
    if (done === ch.length) return { icon: 'ok', word, tone: 'ok' };
    if (ch.some((c) => c.state === 'failed' || c.state === 'link_expired' || c.state === 'offline'))
      return { icon: 'err', word, tone: 'err' };
    return { icon: 'wait', word, tone: 'none' };
  }

  const view = $derived.by(() => {
    if (item.kind === 'file') {
      const d = item.d;
      const bps = speeds.get(d.id) ?? 0;
      const running = d.state === 'running';
      const open = d.segments.filter((s) => s[1] < s[2]).length || d.connections;
      return {
        name: d.name,
        tag: d.resumable === false ? "Can't resume" : null,
        size: formatBytes(d.size),
        cells: stripCells(d.segments, d.size, d.state),
        kind: stripKind(d.state, d.error),
        pct: formatProgress(d.written, d.size) + (running ? ` · ${open} conn` : ''),
        speed: running ? formatSpeed(bps) : '—',
        left: running ? formatEta(etaSeconds(d.written, d.size, bps)) : '—',
        status: runStatus(run) ?? statusOf(d),
      };
    }
    const ch = item.children;
    const known = ch.every((c) => c.size !== null);
    const total = ch.reduce((a, c) => a + (c.size ?? 0), 0);
    const done = ch.reduce((a, c) => a + (c.state === 'completed' ? (c.size ?? c.written) : c.written), 0);
    const bps = ch.reduce((a, c) => a + (c.state === 'running' ? (speeds.get(c.id) ?? 0) : 0), 0);
    const anyRunning = ch.some((c) => c.state === 'running');
    return {
      name: item.pkg.name,
      tag: `${ch.length} file${ch.length === 1 ? '' : 's'}`,
      size: known ? formatBytes(total) : '—',
      cells: packageCells(ch),
      kind: anyRunning ? ('active' as const) : ch.every((c) => c.state === 'completed') ? ('done' as const) : ('queued' as const),
      pct: known ? `${formatBytes(done)} of ${formatBytes(total)}` : formatBytes(done),
      speed: anyRunning ? formatSpeed(bps) : '—',
      left: anyRunning && known ? formatEta(etaSeconds(done, total, bps)) : '—',
      status: runStatus(run) ?? pkgStatus(ch),
    };
  });
</script>

<!-- svelte-ignore a11y_click_events_have_key_events (keys are handled by the treegrid) -->
<div
  role="row"
  class="row"
  class:pkg={item.kind === 'package'}
  class:child={item.kind === 'file' && item.child}
  class:sel={selected}
  data-kind={item.kind}
  data-id={item.kind === 'file' ? item.d.id : item.pkg.id}
  aria-level={item.kind === 'file' && item.child ? 2 : 1}
  aria-expanded={item.kind === 'package' ? item.open : undefined}
  aria-selected={selected}
  tabindex={focusable ? 0 : -1}
  draggable="true"
  class:over
  onclick={onselect}
  ondragstart={(e) => {
    e.dataTransfer?.setData(ROW_TYPE, JSON.stringify(me));
    if (e.dataTransfer) e.dataTransfer.effectAllowed = 'move';
  }}
  ondragover={(e) => {
    if (e.dataTransfer?.types.includes(ROW_TYPE)) {
      e.preventDefault();
      over = true;
    }
  }}
  ondragleave={() => (over = false)}
  ondrop={(e) => {
    over = false;
    const raw = e.dataTransfer?.getData(ROW_TYPE);
    if (!raw) return;
    e.preventDefault();
    ondropped?.(JSON.parse(raw) as Sel);
  }}
>
  <div class="name" role="gridcell">
    {#if item.kind === 'package'}
      <button
        type="button"
        class="chevbtn"
        class:closed={!item.open}
        tabindex="-1"
        aria-label={item.open ? 'Collapse package' : 'Expand package'}
        onclick={(e) => {
          e.stopPropagation();
          ontogglepkg?.();
        }}><Icon name="chev" /></button>
    {/if}
    <span class="t" title={view.name}>{view.name}</span>
    {#if view.tag}<span class="tag">{view.tag}</span>{/if}
  </div>
  <span class="num size r" role="gridcell">{view.size}</span>
  <div class="prog" role="gridcell">
    <Strip cells={view.cells} kind={view.kind} />
    <div class="pct">{view.pct}</div>
  </div>
  <span class="num r" role="gridcell">{view.speed}</span>
  <span class="num r" role="gridcell">{view.left}</span>
  <span role="gridcell" class="st"><StatusCell status={view.status} /></span>
</div>

<style>
  .row {
    display: grid; grid-template-columns: var(--cols);
    align-items: center; gap: 12px; padding: 0 20px; min-height: 40px; border-bottom: 1px solid var(--line-2);
    cursor: default; transition: background 0.15s;
  }
  .row:hover { background: var(--line-2); }
  .row.sel { background: var(--sel); }
  .row.over { box-shadow: inset 0 2px 0 var(--ink-3); }
  .row:focus-visible { outline-offset: -2px; border-radius: 0; }
  .row.pkg { min-height: 44px; font-weight: 500; }
  .row.child .name { padding-left: 26px; }
  .name { display: flex; align-items: center; gap: 8px; min-width: 0; }
  .t { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0; }
  .tag {
    font: 500 10.5px var(--mono); color: var(--ink-3); border: 1px solid var(--line); border-radius: 4px;
    padding: 0 5px; flex: none; font-weight: 500;
  }
  .chevbtn { display: grid; place-items: center; width: 16px; height: 16px; flex: none; color: var(--ink-3); }
  .chevbtn :global(svg) { transition: transform 0.15s; }
  .chevbtn.closed :global(svg) { transform: rotate(-90deg); }
  .size { color: var(--ink-2); }
  .r { text-align: right; }
  .prog { min-width: 0; }
  .pct { font: 500 11px var(--mono); color: var(--ink-3); margin-top: 4px; font-variant-numeric: tabular-nums; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .st { min-width: 0; }
</style>
