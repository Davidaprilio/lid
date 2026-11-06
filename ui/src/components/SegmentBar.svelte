<script lang="ts">
  // Start positions and progress of the connections (1.x dialogs/progress.py SegmentBar).
  import type { Segment } from '../lib/types'
  let { size, segments }: { size: number; segments: Segment[] } = $props()
  const parts = $derived(
    size > 0
      ? segments.map((s) => ({
          left: (s.start / size) * 100,
          width: (Math.max(0, s.pos - s.start) / size) * 100,
          done: s.end >= 0 && s.pos >= s.end,
        }))
      : [],
  )
</script>

<div class="segbar">
  {#each parts as p}<div class:done={p.done} style:left="{p.left}%" style:width="{p.width}%"></div>{/each}
</div>
