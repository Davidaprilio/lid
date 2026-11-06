<script lang="ts">
  // Start positions and progress of the connections (1.x progress.SegmentBar): a 12 px rounded track,
  // each segment filled from its start to its position (green once done), a 2 px gap before every
  // segment but the first; nothing while the size is unknown.
  import type { Segment } from '../lib/types'
  let { size, segments }: { size: number; segments: Segment[] } = $props()
  const parts = $derived(
    size > 0
      ? [...segments]
          .sort((a, b) => a.start - b.start)
          .map((s, i) => {
            const end = s.end >= 0 ? Math.min(s.pos, s.end) : s.pos
            return {
              left: (s.start / size) * 100,
              width: (Math.max(0, end - s.start) / size) * 100,
              done: s.end >= 0 && s.pos >= s.end,
              gap: i > 0,
            }
          })
      : [],
  )
</script>

<div class="segbar">
  {#each parts as p}
    <div class:done={p.done} style:left="{p.left}%" style:width="max(1px, {p.width}%)"></div>
    {#if p.gap}<span class="gap" style:left="{p.left}%"></span>{/if}
  {/each}
</div>
