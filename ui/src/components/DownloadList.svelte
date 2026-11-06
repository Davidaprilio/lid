<script lang="ts">
  // The download list: a check column with a tri-state header, the user's columns, status pills.
  // Only the rows in view are drawn, so long lists stay fast.
  import type { Column, Row } from '../lib/types'
  import Icon from './Icon.svelte'
  import type { ThemeName } from '../theme/tokens'

  let {
    columns, rows, checkState, selected, current, rowHeight, theme, sortKey, sortDesc,
    onselect, oncheck, ontoggleall, onactivate, onmenu, onheader, onheadermenu, onwidth,
  }: {
    columns: Column[]; rows: Row[]; checkState: string; selected: number[]; current: number | null; rowHeight: number
    theme: ThemeName; sortKey: string; sortDesc: boolean
    onselect: (ids: number[], current: number | null) => void; oncheck: (id: number, on: boolean) => void
    ontoggleall: () => void; onactivate: (id: number) => void; onmenu: (e: MouseEvent) => void
    onheader: (key: string) => void; onheadermenu: (e: MouseEvent) => void; onwidth: (key: string, w: number) => void
  } = $props()

  let body: HTMLDivElement | undefined = $state()
  let header: HTMLDivElement | undefined = $state()
  let scroll = $state(0)
  let height = $state(400)
  let widths = $state<Record<string, number>>({})
  let anchor: number | null = null
  let sortShown = $state(false)

  const width = (c: Column) => widths[c.key] ?? c.width
  const first = $derived(Math.max(0, Math.floor(scroll / rowHeight) - 5))
  const last = $derived(Math.min(rows.length, Math.ceil((scroll + height) / rowHeight) + 5))
  const sel = $derived(new Set(selected))
  const total = $derived(40 + columns.reduce((s, c) => s + width(c), 0))

  export function scrollTo(id: number) {
    const i = rows.findIndex((r) => r.id === id)
    if (i < 0 || !body) return
    const top = i * rowHeight
    if (top < body.scrollTop) body.scrollTop = top
    else if (top + rowHeight > body.scrollTop + body.clientHeight) body.scrollTop = top + rowHeight - body.clientHeight
  }

  function click(e: MouseEvent, r: Row, index: number) {
    if (e.shiftKey && anchor !== null) {
      const a = rows.findIndex((x) => x.id === anchor)
      const [lo, hi] = a < index ? [a, index] : [index, a]
      onselect(rows.slice(lo, hi + 1).map((x) => x.id), r.id)
    } else if (e.ctrlKey) {
      const ids = sel.has(r.id) ? selected.filter((i) => i !== r.id) : [...selected, r.id]
      anchor = r.id
      onselect(ids, r.id)
    } else {
      anchor = r.id
      onselect([r.id], r.id)
    }
  }

  function context(e: MouseEvent, r: Row) {
    e.preventDefault()
    if (!sel.has(r.id)) {
      anchor = r.id
      onselect([r.id], r.id)
    }
    onmenu(e)
  }

  export function key(e: KeyboardEvent): boolean {
    if (e.key !== 'ArrowDown' && e.key !== 'ArrowUp' && e.key !== 'Home' && e.key !== 'End') return false
    if (!rows.length) return true
    let i = rows.findIndex((r) => r.id === current)
    if (e.key === 'Home') i = 0
    else if (e.key === 'End') i = rows.length - 1
    else i = Math.max(0, Math.min(rows.length - 1, i + (e.key === 'ArrowDown' ? 1 : -1)))
    anchor = rows[i].id
    onselect([rows[i].id], rows[i].id)
    scrollTo(rows[i].id)
    return true
  }

  function grip(e: MouseEvent, c: Column) {
    e.preventDefault()
    e.stopPropagation()
    const startX = e.clientX
    const startW = width(c)
    const move = (ev: MouseEvent) => (widths[c.key] = Math.max(c.min || 30, startW + ev.clientX - startX))
    const up = () => {
      window.removeEventListener('mousemove', move)
      window.removeEventListener('mouseup', up)
      onwidth(c.key, width(c))
    }
    window.addEventListener('mousemove', move)
    window.addEventListener('mouseup', up)
  }

  $effect(() => {
    if (!body) return
    const ro = new ResizeObserver(() => (height = body!.clientHeight))
    ro.observe(body)
    return () => ro.disconnect()
  })

  const check = (on: boolean) => on
    ? '<svg width="11" height="11" viewBox="0 0 24 24"><path d="M6.5 12.5l3.8 3.8 7.4-8.6" fill="none" stroke="#fff" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"/></svg>'
    : ''
</script>

<div class="list" role="grid" tabindex="0">
  <div class="header" bind:this={header} oncontextmenu={(e) => { e.preventDefault(); onheadermenu(e) }} role="row" tabindex="-1">
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <div class="h c0" style="width:40px;justify-content:center;padding:0" role="columnheader" tabindex="-1" onclick={ontoggleall}>
      <span class="box" class:on={checkState === 'checked'} class:partial={checkState === 'partial'}>
        {#if checkState === 'checked'}{@html check(true)}{:else if checkState === 'partial'}<svg width="10" height="10" viewBox="0 0 24 24"><path d="M7 12h10" stroke="#fff" stroke-width="3" stroke-linecap="round"/></svg>{/if}
      </span>
    </div>
    {#each columns as c, i (c.key)}
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <div class="h" class:last={i === columns.length - 1} style:width="{width(c)}px" role="columnheader" tabindex="-1"
        onclick={() => { sortShown = true; onheader(c.key) }}>
        <span style="overflow:hidden;text-overflow:ellipsis">{c.title}</span>
        {#if sortShown && sortKey === c.key}<span class="faint">{sortDesc ? '▾' : '▴'}</span>{/if}
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <span class="grip" onmousedown={(e) => grip(e, c)} onclick={(e) => e.stopPropagation()}></span>
      </div>
    {/each}
  </div>
  <div class="body" bind:this={body} onscroll={() => { scroll = body!.scrollTop; if (header) header.scrollLeft = body!.scrollLeft }}
    oncontextmenu={(e) => { if (e.target === body) { e.preventDefault(); onselect([], null); onmenu(e) } }} role="rowgroup" tabindex="-1">
    <div style:height="{rows.length * rowHeight}px" style:min-width="{total}px" style="position:relative">
      <div style:transform="translateY({first * rowHeight}px)">
        {#each rows.slice(first, last) as r, k (r.id)}
          {@const index = first + k}
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <div class="row" class:selected={sel.has(r.id)} class:checked={r.checked && !sel.has(r.id)}
            class:odd={index % 2 === 1}
            role="row" tabindex="-1"
            onmousedown={(e) => { if (e.button === 0) click(e, r, index) }}
            ondblclick={() => onactivate(r.id)}
            oncontextmenu={(e) => context(e, r)}>
            <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
            <div class="cell c0" role="checkbox" aria-checked={r.checked} tabindex="-1" onmousedown={(e) => e.stopPropagation()} onclick={() => oncheck(r.id, !r.checked)}>
              <span class="box" class:on={r.checked}>{@html check(r.checked)}</span>
            </div>
            {#each columns as c, ci (c.key)}
              <div class="cell" class:mono={c.mono} class:last={ci === columns.length - 1} style:width="{width(c)}px">
                {#if c.key === 'filename'}
                  <Icon name={r.icon} size={16} {theme} /><span style="overflow:hidden;text-overflow:ellipsis">{r.cells[ci]}</span>
                {:else if c.key === 'queue'}
                  {#if r.queue}<span title={r.queue}><Icon name="queue" size={16} {theme} /></span>{/if}
                {:else if c.key === 'status'}
                  <span class="pill {r.pill.kind}" title={r.error || null}>
                    {#if r.pill.kind === 'run' || r.pill.kind === 'pause'}<span class="fill" style:width="{Math.round(r.pill.progress * 100)}%"></span>{/if}
                    {#if r.pill.kind === 'verified'}<Icon name="shield-check" size={12} {theme} />{/if}
                    {#if r.pill.kind === 'mismatch'}<Icon name="shield-alert" size={12} {theme} />{/if}
                    <span class="t">{r.pill.kind === 'done' ? '✓ ' : r.pill.kind === 'error' ? '! ' : ''}{r.pill.text}</span>
                  </span>
                {:else}
                  {r.cells[ci]}
                {/if}
              </div>
            {/each}
          </div>
        {/each}
      </div>
    </div>
  </div>
</div>
