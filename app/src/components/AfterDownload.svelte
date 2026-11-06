<script lang="ts">
  import Icon from './Icon.svelte';
  import { STEP_TITLES, canProcess, stepIcon } from '../lib/postproc';
  import type { PostprocState } from '../lib/postproc-state.svelte';
  import type { DownloadView, Run, StepState } from '../lib/types';

  let {
    post,
    run,
    files,
    target,
  }: {
    post: PostprocState;
    run: Run | null;
    files: DownloadView[];
    target: { packageId?: number; downloadId?: number };
  } = $props();

  const IDLE: Record<StepState, string> = {
    pending: 'Waiting', running: 'Working…', done: 'Done', skipped: 'Skipped', failed: 'Failed', needs_password: 'Needs a password',
  };

  let password = $state('');
  let remember = $state(true);
  let busy = $state(false);

  const process = $derived(canProcess(files, run));
  const needsPw = $derived(run?.state === 'needs_password');

  async function usePassword(e: SubmitEvent) {
    e.preventDefault();
    if (!run || busy || !password) return;
    busy = true;
    const ok = await post.password(run.id, password, remember);
    busy = false;
    if (ok) password = '';
  }
</script>

<div class="sec">
  <h3><span>After download</span></h3>
  {#if run}
    <ol class="steps" aria-live="polite" aria-label="After download steps">
      {#each run.steps as s (s.kind)}
        <li class="st-{s.state}" data-step={s.kind} data-state={s.state}>
          <span class="ic"><Icon name={stepIcon(s.state)} size={11} /></span>
          <div>{STEP_TITLES[s.kind]}<small>{s.detail ?? IDLE[s.state]}</small></div>
        </li>
      {/each}
    </ol>
  {:else}
    <p class="note">Runs when every file has finished: verify, extract, sort.</p>
  {/if}
  {#if needsPw && run}
    <form class="pw" onsubmit={usePassword}>
      <label for="pp-pw">Archive password</label>
      <input id="pp-pw" type="password" autocomplete="off" bind:value={password} />
      <label class="tick"><input type="checkbox" bind:checked={remember} />Remember this password</label>
      <button type="submit" class="btn" disabled={busy || !password}>Use password</button>
    </form>
  {/if}
  {#if run?.state === 'failed'}
    <div class="acts"><button type="button" class="btn" onclick={() => run && post.retry(run.id)}>Retry</button></div>
  {:else if process}
    <div class="acts"><button type="button" class="btn" onclick={() => post.process(target)}>Process</button></div>
  {/if}
</div>

<style>
  .sec { padding: 16px 20px; border-bottom: 1px solid var(--line); }
  h3 {
    font: 500 11px var(--mono); text-transform: uppercase; letter-spacing: 0.08em; color: var(--ink-3);
    margin-bottom: 12px; display: flex; justify-content: space-between;
  }
  .steps { list-style: none; display: flex; flex-direction: column; gap: 10px; margin: 0; padding: 0; }
  .steps li { display: grid; grid-template-columns: 20px 1fr; gap: 10px; font-size: 13px; color: var(--ink); }
  .ic {
    width: 20px; height: 20px; border-radius: 50%; border: 1.5px solid var(--line); display: grid; place-items: center;
    color: var(--ink-3);
  }
  .st-done .ic { background: var(--ok); border-color: var(--ok); color: var(--surface); }
  .st-skipped .ic { color: var(--ok); }
  .st-running .ic { border-color: var(--signal); color: var(--signal); }
  .st-failed .ic, .st-needs_password .ic { border-color: var(--err); color: var(--err); }
  small { display: block; color: var(--ink-3); font-size: 12px; word-break: break-all; }
  .st-failed small, .st-needs_password small { color: var(--err); }
  .note { font-size: 12.5px; color: var(--ink-3); }
  .pw { margin-top: 14px; display: grid; gap: 8px; font-size: 12.5px; }
  .pw label { color: var(--ink-2); }
  .pw input[type='password'] {
    color: var(--ink); background: var(--raised); border: 1px solid var(--line); border-radius: 5px; padding: 5px 8px;
    font: inherit;
  }
  .tick { display: flex; align-items: center; gap: 8px; }
  .pw .btn { justify-self: start; }
  .acts { margin-top: 12px; display: flex; gap: 8px; }
  .btn:disabled { opacity: 0.5; cursor: default; }
</style>
