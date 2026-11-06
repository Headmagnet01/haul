<script lang="ts">
  import SegmentMap from './SegmentMap.svelte';
  import AfterDownload from './AfterDownload.svelte';
  import type { PostprocState } from '../lib/postproc-state.svelte';
  import type { AppState } from '../lib/app-state.svelte';
  import { formatBytes, formatSpeed } from '../lib/format';
  import { stripCells, stripKind } from '../lib/strip';
  import type { PriorityName } from '../lib/types';

  let { app, post }: { app: AppState; post?: PostprocState } = $props();

  let hash = $state('');
  async function saveHash(e: SubmitEvent, id: number) {
    e.preventDefault();
    if (!post) return;
    if (await post.setHash(id, hash.trim() || null)) hash = '';
  }

  const MONTHS = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];

  const d = $derived(app.selected === null ? null : (app.snapshot.downloads.find((x) => x.id === app.selected) ?? null));
  const pkg = $derived(app.selectedPkg === null ? null : (app.snapshot.packages.find((p) => p.id === app.selectedPkg) ?? null));
  const kids = $derived(pkg ? app.childrenOf(pkg.id) : []);

  function added(iso: string): string {
    const t = new Date(iso);
    if (Number.isNaN(t.getTime())) return '—';
    const p = (n: number) => String(n).padStart(2, '0');
    return `${p(t.getHours())}:${p(t.getMinutes())}, ${t.getDate()} ${MONTHS[t.getMonth()]}`;
  }

  const file = $derived.by(() => {
    if (!d) return null;
    const running = d.state === 'running';
    const segs = [...d.segments].sort((a, b) => a[0] - b[0]);
    const speedsByStart = new Map((app.ranges.get(d.id) ?? []).map((r) => [r.start, r]));
    const conns =
      d.size === null
        ? []
        : segs.map((s, i) => {
            const len = s[2] - s[0];
            const done = s[1] >= s[2];
            const bps = speedsByStart.get(s[0])?.bps ?? 0;
            return {
              label: `C${i + 1}`,
              fill: len > 0 ? Math.min(1, (s[1] - s[0]) / len) : 1,
              text: done ? 'done' : running ? formatSpeed(bps) : '—',
            };
          });
    const steal = app.lastSteal.get(d.id);
    return {
      running,
      open: segs.filter((s) => s[1] < s[2]).length,
      cells: stripCells(d.segments, d.size, d.state),
      kind: stripKind(d.state, d.error),
      conns,
      steal,
      speed: app.speeds.get(d.id) ?? 0,
      resumable: d.resumable === null ? 'Not checked yet' : d.resumable ? 'Yes (byte ranges, ETag checked)' : "No (can't resume)",
      saveTo: d.state === 'completed' && d.targetPath ? d.targetPath : d.targetDir,
      moving: d.state === 'running' || d.state === 'queued',
    };
  });

  const pkgInfo = $derived.by(() => {
    const total = kids.reduce((a, c) => a + (c.size ?? 0), 0);
    const done = kids.reduce((a, c) => a + (c.state === 'completed' ? (c.size ?? c.written) : c.written), 0);
    return { total, done, moving: kids.some((c) => c.state === 'running' || c.state === 'queued') };
  });

  async function copyLink(url: string) {
    try {
      await navigator.clipboard.writeText(url);
      app.showToast('Link copied');
    } catch {
      app.showToast("Couldn't copy the link");
    }
  }
</script>

<aside class="drawer" aria-label="Details">
  {#if d && file}
    <div class="dh">
      <div class="k">Selected</div>
      <h2>{d.name}</h2>
      <div class="meta">
        <span class="num">{formatBytes(d.written)} / {formatBytes(d.size)}</span>
        {#if file.running}<span class="num">{formatSpeed(file.speed)}</span>{/if}
      </div>
    </div>
    <div class="sec">
      <h3><span>Segment map</span>{#if file.running}<span>{file.open} connections</span>{/if}</h3>
      <SegmentMap cells={file.cells} kind={file.kind} />
      <p class="steal">
        {#if file.steal !== undefined}A connection finished early and took over half of the largest remaining range <em>({formatBytes(file.steal)})</em>.{/if}
      </p>
      {#if file.conns.length}
        <div class="conns">
          {#each file.conns as c (c.label)}
            <span>{c.label}</span><span class="cbar"><b style:width="{(c.fill * 100).toFixed(1)}%"></b></span><span>{c.text}</span>
          {/each}
        </div>
      {/if}
    </div>
    {#if post && d.packageId === null}
      <AfterDownload {post} run={post.byDownload(d.id)} files={[d]} target={{ downloadId: d.id }} />
    {/if}
    <div class="sec">
      <h3><span>Details</span></h3>
      <dl class="kv">
        <dt>Host</dt><dd>{d.host || '—'}</dd>
        <dt>Resumable</dt><dd>{file.resumable}</dd>
        <dt>Added</dt><dd>{added(d.createdAt)}</dd>
        <dt>Save to</dt><dd>{file.saveTo}</dd>
        <dt><label for="prio">Priority</label></dt>
        <dd>
          <select
            id="prio"
            value={d.priority}
            onchange={(e) => {
              const id = d.id;
              void app.backend.setPriority(id, e.currentTarget.value as PriorityName).then(() => app.load());
            }}
          >
            <option value="high">High</option>
            <option value="normal">Normal</option>
            <option value="low">Low</option>
          </select>
        </dd>
      </dl>
      {#if post}
        <form class="hash" onsubmit={(e) => saveHash(e, d.id)}>
          <label for="hash">Expected checksum</label>
          <div class="hrow">
            <input id="hash" bind:value={hash} placeholder="Paste MD5, SHA-1, SHA-256 or CRC32" autocomplete="off" spellcheck="false" />
            <button type="submit" class="btn">Save</button>
          </div>
        </form>
      {/if}
    </div>
    <div class="dact">
      <button type="button" class="btn" disabled={d.state === 'completed'} onclick={() => app.toggle(d.id)}>
        {file.moving ? 'Pause' : 'Resume'}<kbd>Space</kbd>
      </button>
      <button type="button" class="btn" onclick={() => app.backend.openFolder(d.id)}>Open folder</button>
      <button type="button" class="btn" onclick={() => copyLink(d.url)}>Copy link</button>
      <button type="button" class="btn" onclick={() => app.removeWithUndo([d.id])}>Remove<kbd>Del</kbd></button>
    </div>
  {:else if pkg}
    <div class="dh">
      <div class="k">Package</div>
      <h2>{pkg.name}</h2>
      <div class="meta">
        <span>{kids.length} file{kids.length === 1 ? '' : 's'}</span>
        <span class="num">{formatBytes(pkgInfo.done)} / {formatBytes(pkgInfo.total)}</span>
      </div>
    </div>
    {#if post}
      <AfterDownload {post} run={post.byPackage(pkg.id)} files={kids} target={{ packageId: pkg.id }} />
    {/if}
    <div class="dact">
      {#if pkgInfo.moving}
        <button type="button" class="btn" onclick={() => app.toggleIds(kids)}>Pause all</button>
      {:else}
        <button type="button" class="btn" onclick={() => app.toggleIds(kids)}>Resume all</button>
      {/if}
      <button type="button" class="btn" onclick={() => app.removeWithUndo(kids.map((k) => k.id))}>Remove<kbd>Del</kbd></button>
    </div>
  {:else}
    <p class="hint">Select a download to see its connections.</p>
  {/if}
</aside>

<style>
  .drawer { grid-row: 1; border-left: 1px solid var(--line); background: var(--bg); display: flex; flex-direction: column; min-width: 0; overflow: auto; }
  .dh { padding: 18px 20px 14px; border-bottom: 1px solid var(--line); }
  .k, h3 { font: 500 11px var(--mono); text-transform: uppercase; letter-spacing: 0.08em; color: var(--ink-3); }
  .k { margin-bottom: 6px; }
  h2 { font: 600 15px/1.3 var(--sans); word-break: break-all; }
  .meta { margin-top: 8px; color: var(--ink-2); font-size: 12.5px; display: flex; gap: 12px; }
  .sec { padding: 16px 20px; border-bottom: 1px solid var(--line); }
  h3 { margin-bottom: 12px; display: flex; justify-content: space-between; }
  .steal { margin-top: 10px; font-size: 12px; color: var(--ink-3); min-height: 17px; }
  .steal em { font-style: normal; color: var(--ink-2); font-family: var(--mono); }
  .conns {
    margin-top: 12px; display: grid; grid-template-columns: auto 1fr auto; gap: 5px 10px;
    font: 12px var(--mono); color: var(--ink-2); font-variant-numeric: tabular-nums;
  }
  .cbar { height: 4px; background: var(--track); border-radius: 2px; align-self: center; overflow: hidden; }
  .cbar b { display: block; height: 100%; background: var(--ink-3); }
  .kv { display: grid; grid-template-columns: 100px 1fr; gap: 7px 12px; font-size: 12.5px; align-items: center; }
  .kv dt { color: var(--ink-3); }
  .kv dd { color: var(--ink); word-break: break-all; }
  select { color: var(--ink); background: var(--raised); border: 1px solid var(--line); border-radius: 5px; padding: 2px 6px; font-size: 12.5px; }
  .dact { padding: 16px 20px; display: flex; gap: 8px; flex-wrap: wrap; }
  .btn:disabled { opacity: 0.5; cursor: default; }
  .hint { padding: 24px 20px; color: var(--ink-3); font-size: 13px; }
  .hash { margin-top: 14px; display: grid; gap: 6px; font-size: 12.5px; }
  .hash label { color: var(--ink-3); }
  .hrow { display: flex; gap: 8px; }
  .hrow input {
    flex: 1; min-width: 0; color: var(--ink); background: var(--raised); border: 1px solid var(--line); border-radius: 5px;
    padding: 4px 8px; font: 12px var(--mono);
  }
</style>
