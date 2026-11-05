<script lang="ts">
  let { title }: { title: string } = $props();
</script>

<main class="main">
  <div class="top"><h1>{title}</h1></div>
  <p class="later">Arrives in a later version.</p>
</main>

<style>
  .main { grid-row: 1; display: flex; flex-direction: column; min-width: 0; background: var(--surface); }
  .top { display: flex; align-items: center; padding: 14px 20px; border-bottom: 1px solid var(--line); min-height: 59px; }
  h1 { font: 600 17px var(--sans); letter-spacing: -0.01em; }
  .later { padding: 40px 20px; color: var(--ink-3); }
</style>
