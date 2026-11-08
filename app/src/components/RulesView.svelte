<script lang="ts">
  import RuleEditor from './RuleEditor.svelte';
  import CategoryTable from './CategoryTable.svelte';
  import PasswordList from './PasswordList.svelte';
  import BrowserList from './BrowserList.svelte';
  import type { Backend } from '../lib/backend';
  import type { RulesState } from '../lib/rules-state.svelte';
  import { blankRule, mbToBytes, ruleSummary } from '../lib/rules';
  import { errText } from '../lib/postproc';
  import type { PostProcSettings, Rule } from '../lib/types';

  let { rs, backend, browsersRev = 0 }: { rs: RulesState; backend: Backend; browsersRev?: number } = $props();

  const SWITCHES: { key: keyof PostProcSettings; label: string }[] = [
    { key: 'auto', label: 'Process finished packages automatically' },
    { key: 'sortCategories', label: 'Sort into category folders' },
    { key: 'recycleArchives', label: 'Send archives to the Recycle Bin after extracting' },
    { key: 'virusScan', label: 'Scan with Windows Defender' },
  ];

  const cfg = $derived(rs.config);
  let editing = $state<number | 'new' | null>(null);

  async function saveRule(r: Rule) {
    if (!cfg) return;
    const list = $state.snapshot(cfg.rules) as Rule[];
    if (editing === 'new') list.push(r);
    else if (editing !== null) list[editing] = r;
    if (await rs.saveRules(list)) editing = null;
  }

  async function toggleRule(i: number) {
    if (!cfg) return;
    const list = $state.snapshot(cfg.rules) as Rule[];
    list[i] = { ...list[i], enabled: !list[i].enabled };
    await rs.saveRules(list);
  }

  async function deleteRule(i: number) {
    if (!cfg) return;
    const list = ($state.snapshot(cfg.rules) as Rule[]).filter((_, j) => j !== i);
    if (editing === i) editing = null;
    await rs.saveRules(list);
  }

  // "Try a file name": where would it go under the saved rules?
  let tryName = $state('');
  let tryHost = $state('');
  let trySize = $state('');
  let tryOut = $state('');
  let tryTimer: ReturnType<typeof setTimeout> | undefined;
  let tryGen = 0;
  $effect(() => {
    const name = tryName.trim();
    const host = tryHost.trim();
    const size = mbToBytes(trySize);
    const c = cfg;
    clearTimeout(tryTimer);
    if (!name || !c) {
      tryOut = '';
      return;
    }
    const draft = $state.snapshot({ rules: c.rules, categories: c.categories, sort: c.settings.sortCategories });
    tryTimer = setTimeout(async () => {
      const my = ++tryGen;
      try {
        const d = await backend.tryRules({ name, host, size }, draft as typeof draft);
        if (my !== tryGen) return;
        tryOut = d.folder === null ? `→ stays where it downloaded${d.by ? ` (${d.by})` : ''}` : `→ ${d.folder}\\${d.name} (${d.by})`;
      } catch (e) {
        if (my === tryGen) tryOut = errText(e);
      }
    }, 200);
  });
</script>

<main class="main">
  <div class="top">
    <h1>Rules</h1>
    <span class="sub">What happens after every file in a package has finished</span>
  </div>
  {#if cfg}
    <div class="scroll">
      <section aria-labelledby="h-after">
        <h2 id="h-after">After download</h2>
        <div class="switches">
          {#each SWITCHES as s (s.key)}
            <button type="button" class="tog" aria-pressed={cfg.settings[s.key]} onclick={() => rs.setSetting(s.key, !cfg.settings[s.key])}>
              <span class="sw" aria-hidden="true"></span>{s.label}
            </button>
          {/each}
        </div>
      </section>

      <section aria-labelledby="h-cats">
        <h2 id="h-cats">Categories</h2>
        <p class="help">Files no rule catches go to their category's folder. A folder without a drive letter is inside your download folder.</p>
        <CategoryTable categories={cfg.categories} onsave={(c) => rs.saveCategories(c)} />
      </section>

      <section aria-labelledby="h-rules">
        <h2 id="h-rules">Your rules</h2>
        <p class="help">Checked top to bottom; the first match wins.</p>
        {#if cfg.rules.length === 0 && editing !== 'new'}
          <p class="empty">No rules yet. Files go to their category folder.</p>
        {/if}
        {#if cfg.rules.length}
          <ul class="list" aria-label="Your rules">
            {#each cfg.rules as r, i (r.id)}
              <li class:off={!r.enabled}>
                <div class="rrow">
                  <button type="button" class="tog" aria-pressed={r.enabled} aria-label="Use rule {r.name}" onclick={() => toggleRule(i)}>
                    <span class="sw" aria-hidden="true"></span>
                  </button>
                  <button type="button" class="rule" onclick={() => (editing = editing === i ? null : i)}>
                    <b>{r.name}</b><span class="sum">{ruleSummary(r)}</span>
                  </button>
                  <button type="button" class="btn sm" disabled={i === 0} onclick={() => rs.moveRule(i, -1)}>Move up</button>
                  <button type="button" class="btn sm" disabled={i === cfg.rules.length - 1} onclick={() => rs.moveRule(i, 1)}>Move down</button>
                  <button type="button" class="btn sm" onclick={() => deleteRule(i)}>Delete</button>
                </div>
                {#if editing === i}
                  <RuleEditor rule={$state.snapshot(r) as Rule} onsave={saveRule} oncancel={() => (editing = null)} />
                {/if}
              </li>
            {/each}
          </ul>
        {/if}
        {#if editing === 'new'}
          <RuleEditor rule={blankRule()} onsave={saveRule} oncancel={() => (editing = null)} />
        {:else}
          <button type="button" class="btn" onclick={() => (editing = 'new')}>Add rule</button>
        {/if}
        <div class="try">
          <label>Try a file name<input bind:value={tryName} placeholder="ubuntu-24.04.iso" autocomplete="off" /></label>
          <label>Host<input bind:value={tryHost} placeholder="optional" autocomplete="off" /></label>
          <label>Size (MB)<input bind:value={trySize} placeholder="optional" inputmode="decimal" autocomplete="off" /></label>
        </div>
        <p class="tryout" aria-live="polite">{tryOut}</p>
      </section>

      <section aria-labelledby="h-pw">
        <h2 id="h-pw">Saved passwords</h2>
        <PasswordList
          ids={cfg.passwords}
          onadd={(pw) => rs.addPassword(pw)}
          onremove={(id) => rs.removePassword(id)}
          onmove={(i, by) => rs.movePassword(i, by)}
        />
      </section>

      <section aria-labelledby="h-browsers">
        <h2 id="h-browsers">Browsers</h2>
        <BrowserList {backend} rev={browsersRev} />
      </section>
    </div>
  {/if}
</main>

<style>
  .main { grid-row: 1; display: flex; flex-direction: column; min-width: 0; min-height: 0; background: var(--surface); }
  .top { display: flex; align-items: center; gap: 12px; padding: 14px 20px; border-bottom: 1px solid var(--line); min-width: 0; }
  h1 { font: 600 17px var(--sans); letter-spacing: -0.01em; }
  .sub { color: var(--ink-3); font-size: 12.5px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; min-width: 0; }
  .scroll { overflow: auto; min-height: 0; }
  section { padding: 18px 20px; border-bottom: 1px solid var(--line); max-width: 860px; }
  h2 { font: 500 11px var(--mono); text-transform: uppercase; letter-spacing: 0.08em; color: var(--ink-3); margin-bottom: 12px; }
  .help { font-size: 12px; color: var(--ink-3); margin-bottom: 10px; }
  .empty { font-size: 13px; color: var(--ink-2); margin-bottom: 10px; }
  .switches { display: grid; gap: 6px; justify-items: start; }
  .tog {
    display: inline-flex; align-items: center; gap: 9px; padding: 4px 8px; border-radius: 5px; font-size: 13px;
    color: var(--ink); transition: background 0.15s;
  }
  .tog:hover { background: var(--line-2); }
  .sw { width: 24px; height: 14px; border-radius: 7px; background: var(--line); position: relative; transition: background 0.15s; flex: none; }
  .sw::after {
    content: ''; position: absolute; top: 2px; left: 2px; width: 10px; height: 10px; border-radius: 50%;
    background: var(--surface); transition: transform 0.15s;
  }
  .tog[aria-pressed='true'] .sw { background: var(--signal); }
  .tog[aria-pressed='true'] .sw::after { transform: translateX(10px); }
  .list { list-style: none; margin: 0 0 10px; padding: 0; display: grid; gap: 6px; }
  .rrow { display: flex; align-items: center; gap: 8px; }
  .rule { flex: 1; min-width: 0; text-align: left; display: flex; gap: 10px; align-items: baseline; padding: 4px 6px; border-radius: 5px; }
  .rule:hover { background: var(--line-2); }
  .rule b { font-weight: 500; font-size: 13px; flex: none; }
  .sum { font: 12px var(--mono); color: var(--ink-2); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .off .rule b, .off .sum { color: var(--ink-3); }
  .sm { padding: 3px 8px; font-size: 12px; flex: none; }
  .btn:disabled { opacity: 0.5; cursor: default; }
  .try { display: grid; grid-template-columns: 2fr 1fr 110px; gap: 10px; margin-top: 16px; }
  .try label { display: grid; gap: 4px; font-size: 12px; color: var(--ink-2); }
  .try input {
    color: var(--ink); background: var(--raised); border: 1px solid var(--line); border-radius: 5px; padding: 5px 8px;
    font: 12.5px var(--sans); min-width: 0;
  }
  .tryout { margin-top: 8px; min-height: 18px; font: 12px var(--mono); color: var(--ink-2); word-break: break-all; }
  @media (prefers-reduced-motion: reduce) {
    .sw, .sw::after { transition: none; }
  }
</style>
