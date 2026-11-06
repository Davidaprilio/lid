<script lang="ts">
  // A form row: a fixed label column, then the controls (1.x widgets.form_row).
  import type { Snippet } from 'svelte'
  let { label = '', width = 96, children, top = false }: { label?: string; width?: number; children: Snippet; top?: boolean } = $props()
</script>

<div class="formrow" style:align-items={top ? 'flex-start' : 'center'}>
  <span class="formLabel" style:width="{width}px" style:padding-top={top ? '6px' : null}>{label}</span>
  <div class="formctl">{@render children()}</div>
</div>
