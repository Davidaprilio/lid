<script lang="ts">
  // The Categories / Queues sidebar tree.
  import type { TreeNode, TreeFilter } from '../lib/types'
  import Icon from './Icon.svelte'
  import Self from './Tree.svelte'
  import type { ThemeName } from '../theme/tokens'

  let { nodes, filter, theme, depth = 0, onselect, onmenu, expanded }: {
    nodes: TreeNode[]; filter: TreeFilter; theme: ThemeName; depth?: number
    onselect: (f: TreeFilter) => void; onmenu: (f: TreeFilter | null, e: MouseEvent) => void
    expanded: Record<string, boolean>
  } = $props()

  const key = (f: TreeFilter) => `${f.kind}/${f.value ?? ''}`
  const same = (a: TreeFilter, b: TreeFilter) => a.kind === b.kind && (a.value ?? null) === (b.value ?? null)
</script>

{#each nodes as n (key(n.filter))}
  {@const k = key(n.filter)}
  {@const open = expanded[k] ?? n.expanded}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div
    class="node"
    class:selected={same(n.filter, filter)}
    data-depth={depth}
    style:padding-left="{4 + depth * 16}px"
    role="treeitem"
    aria-selected={same(n.filter, filter)}
    tabindex="-1"
    onclick={() => onselect(n.filter)}
    oncontextmenu={(e) => { e.preventDefault(); e.stopPropagation(); onselect(n.filter); onmenu(n.filter, e) }}
  >
    <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
    <span class="branch" role="button" tabindex="-1" onclick={(e) => { e.stopPropagation(); expanded[k] = !open }}>
      {#if n.children.length}<svg width="10" height="10" viewBox="0 0 24 24"><path d={open ? 'M6 9l6 6 6-6' : 'M9 6l6 6-6 6'} fill="none" stroke="var(--fa)" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round"/></svg>{/if}
    </span>
    <Icon name={n.icon} size={18} {theme} />
    <span class="title">{n.title}</span>
    {#if n.count > 0}<span class="badge">{n.count}</span>{/if}
  </div>
  {#if n.children.length && open}
    <Self nodes={n.children} {filter} {theme} depth={depth + 1} {onselect} {onmenu} {expanded} />
  {/if}
{/each}
