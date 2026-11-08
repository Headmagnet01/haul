import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { open } from '@tauri-apps/plugin-dialog';
import type { Backend } from './backend';
import type { Update } from './types';

function subscribe<T>(event: string, cb: (payload: T) => void): () => void {
  let off: (() => void) | null = null;
  let cancelled = false;
  listen<T>(event, (e) => cb(e.payload)).then((un) => {
    if (cancelled) un();
    else off = un;
  });
  return () => {
    cancelled = true;
    off?.();
  };
}

/** Command names match app/src-tauri/src/commands.rs; argument names are camelCase. */
export const tauriBackend: Backend = {
  settings: () => invoke('settings'),
  setSettings: (patch) => invoke('set_settings', { patch }),
  pickFolder: async () => {
    const r = await open({ directory: true, multiple: false });
    return typeof r === 'string' ? r : null;
  },
  openPath: (path) => invoke('open_path', { path }),
  snapshot: () => invoke('snapshot'),
  addLinks: (text, source) => invoke('add_links', { text, source }),
  collectorSnapshot: () => invoke('collector_snapshot'),
  collectorStart: (ids) => invoke('collector_start', { ids }),
  collectorRemove: (ids) => invoke('collector_remove', { ids }),
  collectorRecheck: (ids) => invoke('collector_recheck', { ids }),
  setCollectorFilters: (filters) => invoke('set_collector_filters', { filters }),
  pause: (ids) => invoke('pause', { ids }),
  resume: (ids) => invoke('resume', { ids }),
  remove: (ids) => invoke('remove', { ids }),
  pauseAll: () => invoke('pause_all'),
  setPriority: (id, priority) => invoke('set_priority', { id, priority }),
  reorder: (ids) => invoke('reorder', { ids }),
  openFolder: (id) => invoke('open_folder', { id }),
  openFile: (id) => invoke('open_file', { id }),
  reveal: (id) => invoke('open_folder', { id }),
  filesExist: (ids) => invoke('files_exist', { ids }),
  deleteFiles: (ids) => invoke('delete_files', { ids }),
  downloadAgain: (id) => invoke('download_again', { id }),
  postprocRuns: () => invoke('postproc_runs'),
  postprocProcess: (t) => invoke('postproc_process', { packageId: t.packageId ?? null, downloadId: t.downloadId ?? null }),
  postprocRetry: (runId) => invoke('postproc_retry', { runId }),
  postprocPassword: (runId, password, remember) => invoke('postproc_password', { runId, password, remember }),
  setExpectedHash: (id, hash) => invoke('set_expected_hash', { id, hash }),
  rulesConfig: () => invoke('rules_config'),
  setPostprocSettings: (settings) => invoke('set_postproc_settings', { settings }),
  setCategories: (categories) => invoke('set_categories', { categories }),
  setRules: (rules) => invoke('set_rules', { rules }),
  tryRules: (f, d) =>
    invoke('try_rules', { name: f.name, host: f.host, size: f.size, rules: d.rules, categories: d.categories, sort: d.sort }),
  addPassword: (password) => invoke('add_password', { password }),
  removePassword: (id) => invoke('remove_password', { id }),
  orderPasswords: (ids) => invoke('order_passwords', { ids }),
  pairAnswer: (n, allow) => invoke('pair_answer', { n, allow }),
  pairedClients: () => invoke('paired_clients'),
  removePairedClient: (id) => invoke('remove_paired_client', { id }),
  onUpdate: (cb) => subscribe<Update>('haul:update', cb),
  onPairRequest: (cb) => subscribe<{ n: number; browser: string }>('haul:pair-request', (p) => cb(p.n, p.browser)),
  onPairClosed: (cb) => subscribe<{ n: number }>('haul:pair-closed', (p) => cb(p.n)),
};
