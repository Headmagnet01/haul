<script lang="ts">
  import AfterDownload from './AfterDownload.svelte';
  import type { AppState } from '../lib/app-state.svelte';
  import type { PostprocState } from '../lib/postproc-state.svelte';
  import { finishedTime, locationOf } from '../lib/finished';
  import { formatBytes } from '../lib/format';

  let { app, post }: { app: AppState; post?: PostprocState } = $props();

  const d = $derived(app.selected === null ? null : (app.byId.get(app.selected) ?? null));
  const file = $derived(d && app.finished.has(d.id) ? d : null);
  const pkg = $derived(app.selectedPkg === null ? null : (app.snapshot.packages.find((p) => p.id === app.selectedPkg) ?? null));
  const kids = $derived(pkg ? app.childrenOf(pkg.id) : []);
  const now = new Date();
</script>

<aside class="drawer" aria-label="Details">
  {#if file}
    {@const gone = app.missing.has(file.id)}
    <div class="dh">
      <div class="k">Finished</div>
      <h2>{file.name}</h2>
      <div class="meta">
        <span class="num">{formatBytes(file.size)}</span>
        <span>{finishedTime(file.finishedAt ?? file.createdAt, now)}</span>
        {#if gone}<span class="miss">Missing</span>{/if}
      </div>
    </div>
    {#if post && file.packageId === null}
      <AfterDownload {post} run={post.byDownload(file.id)} files={[file]} target={{ downloadId: file.id }} />
    {/if}
    <div class="sec">
      <h3><span>Details</span></h3>
      <dl class="kv">
        <dt>Host</dt><dd>{file.host || '—'}</dd>
        <dt>Location</dt><dd>{locationOf(file)}</dd>
      </dl>
    </div>
    <div class="dact">
      <button type="button" class="btn primary" disabled={gone} onclick={() => app.openFile(file.id)}>Open<kbd>Enter</kbd></button>
      <button type="button" class="btn" disabled={gone} onclick={() => app.reveal(file.id)}>Open folder</button>
      <button type="button" class="btn" onclick={() => app.downloadAgain(file.id)}>Download again</button>
      <button type="button" class="btn" onclick={() => app.removeWithUndo([file.id])}>Remove from list<kbd>Del</kbd></button>
      <button type="button" class="btn" onclick={() => app.askDelete([file.id])}>Delete file…<kbd>Shift Del</kbd></button>
    </div>
  {:else if pkg && kids.length && kids.every((k) => app.finished.has(k.id))}
    {@const first = kids.find((k) => !app.missing.has(k.id)) ?? null}
    <div class="dh">
      <div class="k">Package</div>
      <h2>{pkg.name}</h2>
      <div class="meta">
        <span>{kids.length} file{kids.length === 1 ? '' : 's'}</span>
        <span class="num">{formatBytes(kids.reduce((a, c) => a + (c.size ?? 0), 0))}</span>
      </div>
    </div>
    {#if post}
      <AfterDownload {post} run={post.byPackage(pkg.id)} files={kids} target={{ packageId: pkg.id }} />
    {/if}
    <div class="dact">
      <button type="button" class="btn" disabled={first === null} onclick={() => first && app.reveal(first.id)}>Open folder</button>
      <button type="button" class="btn" onclick={() => app.removeWithUndo(kids.map((k) => k.id))}>Remove from list<kbd>Del</kbd></button>
      <button type="button" class="btn" onclick={() => app.askDelete(kids.map((k) => k.id))}>Delete files…<kbd>Shift Del</kbd></button>
    </div>
  {:else}
    <p class="hint">Select a finished download to open it or find its folder.</p>
  {/if}
</aside>

<style>
  .drawer { grid-row: 1; border-left: 1px solid var(--line); background: var(--bg); display: flex; flex-direction: column; min-width: 0; overflow: auto; }
  .dh { padding: 18px 20px 14px; border-bottom: 1px solid var(--line); }
  .k, h3 { font: 500 11px var(--mono); text-transform: uppercase; letter-spacing: 0.08em; color: var(--ink-3); }
  .k { margin-bottom: 6px; }
  h2 { font: 600 15px/1.3 var(--sans); word-break: break-all; }
  .meta { margin-top: 8px; color: var(--ink-2); font-size: 12.5px; display: flex; gap: 12px; }
  .miss { color: var(--err); }
  .sec { padding: 16px 20px; border-bottom: 1px solid var(--line); }
  h3 { margin-bottom: 12px; display: flex; justify-content: space-between; }
  .kv { display: grid; grid-template-columns: 100px 1fr; gap: 7px 12px; font-size: 12.5px; align-items: center; }
  .kv dt { color: var(--ink-3); }
  .kv dd { color: var(--ink); word-break: break-all; }
  .dact { padding: 16px 20px; display: flex; gap: 8px; flex-wrap: wrap; }
  .btn:disabled { opacity: 0.5; cursor: default; }
  .hint { padding: 24px 20px; color: var(--ink-3); font-size: 13px; }
</style>
