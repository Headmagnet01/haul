import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { Backend } from './backend';
import type { Update } from './types';

/** Command names match app/src-tauri/src/commands.rs; argument names are camelCase. */
export const tauriBackend: Backend = {
  snapshot: () => invoke('snapshot'),
  addLinks: (text) => invoke('add_links', { text }),
  pause: (ids) => invoke('pause', { ids }),
  resume: (ids) => invoke('resume', { ids }),
  remove: (ids) => invoke('remove', { ids }),
  pauseAll: () => invoke('pause_all'),
  setPriority: (id, priority) => invoke('set_priority', { id, priority }),
  reorder: (ids) => invoke('reorder', { ids }),
  setSpeedLimit: (l) => invoke('set_speed_limit', { enabled: l.enabled, bps: l.bps }),
  openFolder: (id) => invoke('open_folder', { id }),
  onUpdate(cb) {
    let off: (() => void) | null = null;
    let cancelled = false;
    listen<Update>('haul:update', (e) => cb(e.payload)).then((un) => {
      if (cancelled) un();
      else off = un;
    });
    return () => {
      cancelled = true;
      off?.();
    };
  },
};
