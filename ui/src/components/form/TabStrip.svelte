<script lang="ts">
  // Underlined tabs on a strip (1.x widgets.tab_strip).
  let { names, active = $bindable(0) }: { names: string[]; active?: number } = $props()
</script>

<div class="tabstrip" role="tablist">
  {#each names as n, i}
    <button class="tab" class:on={active === i} role="tab" aria-selected={active === i} onclick={() => (active = i)}>{n}</button>
  {/each}
</div>
