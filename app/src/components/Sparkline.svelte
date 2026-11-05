<script lang="ts">
  import { sparkPoints } from '../lib/sparkline';

  let { samples, width = 84, height = 20 }: { samples: number[]; width?: number; height?: number } = $props();
  const points = $derived(sparkPoints(samples, width, height));
</script>

<svg class="spark" {width} {height} viewBox="0 0 {width} {height}" preserveAspectRatio="none" aria-hidden="true">
  <polyline fill="none" stroke="var(--signal)" stroke-width="1.5" stroke-linejoin="round" {points} />
</svg>

<style>
  .spark { flex: none; }
</style>
