<script lang="ts" module>
  import type { IconName } from '../lib/types';
  export type { IconName };

  // Paths from docs/mockup/index.html where it has them; the rest drawn to match (16px box, 1.5px stroke).
  const PATHS: Record<IconName, { d: string; fill?: boolean }> = {
    queue: { d: 'M8 2v9m0 0 3.5-3.5M8 11 4.5 7.5M2.5 14h11' },
    collector: { d: 'M2 5h12l-1.2 8.2a1 1 0 0 1-1 .8H4.2a1 1 0 0 1-1-.8L2 5Zm3-3h6l1 3H4l1-3Z' },
    finished: { d: 'm3 8.5 3 3 7-7' },
    rules: { d: 'M2 4h7m3 0h2M2 12h2m3 0h7M9 2.5v3M5 10.5v3' },
    settings: { d: 'M10.2 8a2.2 2.2 0 1 1-4.4 0 2.2 2.2 0 0 1 4.4 0ZM8 1.5v2m0 9v2M1.5 8h2m9 0h2M3.4 3.4l1.4 1.4m6.4 6.4 1.4 1.4m0-9.2-1.4 1.4M4.8 11.2l-1.4 1.4' },
    dl: { d: 'M8 2v9m0 0 3.5-3.5M8 11 4.5 7.5' },
    ok: { d: 'm3 8.5 3 3 7-7' },
    err: { d: 'M14 8A6 6 0 1 1 2 8a6 6 0 0 1 12 0ZM8 4.8v3.8m0 2.4v.1' },
    pause: { d: 'M4.6 3h1.4a.6.6 0 0 1 .6.6v8.8a.6.6 0 0 1-.6.6H4.6a.6.6 0 0 1-.6-.6V3.6a.6.6 0 0 1 .6-.6Zm5.4 0h1.4a.6.6 0 0 1 .6.6v8.8a.6.6 0 0 1-.6.6H10a.6.6 0 0 1-.6-.6V3.6A.6.6 0 0 1 10 3Z', fill: true },
    play: { d: 'M5 3.2v9.6a.5.5 0 0 0 .76.43l7.7-4.8a.5.5 0 0 0 0-.86l-7.7-4.8A.5.5 0 0 0 5 3.2Z', fill: true },
    wait: { d: 'M14 8A6 6 0 1 1 2 8a6 6 0 0 1 12 0ZM8 5v3l2 1.5' },
    chev: { d: 'm4 6 4 4 4-4' },
    plus: { d: 'M8 3v10M3 8h10' },
    search: { d: 'M11.5 7a4.5 4.5 0 1 1-9 0 4.5 4.5 0 0 1 9 0Zm-1 3.5 3 3' },
    folder: { d: 'M2 4.5a1 1 0 0 1 1-1h3.2l1.5 1.5H13a1 1 0 0 1 1 1V12a1 1 0 0 1-1 1H3a1 1 0 0 1-1-1V4.5Z' },
    link: { d: 'M6.8 9.2a2.6 2.6 0 0 0 3.7 0l2.2-2.2a2.6 2.6 0 0 0-3.7-3.7l-.9.9m-.9 4.6a2.6 2.6 0 0 0-3.7 0L1.3 11a2.6 2.6 0 0 0 3.7 3.7l.9-.9' },
    x: { d: 'm4 4 8 8m0-8-8 8' },
    ext: { d: 'M2.5 5.5h11v8h-11zM4 2.5h8l1.5 3h-11z' },
  };
</script>

<script lang="ts">
  let { name, size = 16, class: cls = '' }: { name: IconName; size?: number; class?: string } = $props();
  const p = $derived(PATHS[name]);
</script>

<svg
  class={cls}
  width={size}
  height={size}
  viewBox="0 0 16 16"
  aria-hidden="true"
  focusable="false"
  fill={p.fill ? 'currentColor' : 'none'}
  stroke={p.fill ? 'none' : 'currentColor'}
  stroke-width="1.5"
  stroke-linecap="round"
  stroke-linejoin="round"><path d={p.d} /></svg>
