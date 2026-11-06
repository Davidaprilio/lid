<script lang="ts">
  // The menu bar drawn in the window (View → Menu bar → In the window).
  import type { Menu as MenuT } from '../lib/types'
  import Menu from './Menu.svelte'
  import Label from './Label.svelte'
  import type { ThemeName } from '../theme/tokens'

  let { menus, theme, onpick }: { menus: MenuT[]; theme: ThemeName; onpick: (id: string) => void } = $props()
  let open = $state(-1)
  let pos = $state({ x: 0, y: 0 })
  let buttons: HTMLButtonElement[] = $state([])
  let popup: Menu | undefined = $state()

  export function openAt(i: number) {
    const b = buttons[i]
    if (!b) return
    const r = b.getBoundingClientRect()
    pos = { x: r.left, y: r.bottom + 2 }
    open = i
  }

  /** Alt+letter opens a menu; while one is open the keys move through it. */
  export function key(e: KeyboardEvent): boolean {
    if (open >= 0) {
      if (popup?.key(e)) return true
      if (e.key === 'ArrowRight' || e.key === 'ArrowLeft') {
        openAt((open + (e.key === 'ArrowRight' ? 1 : menus.length - 1)) % menus.length)
        return true
      }
      return false
    }
    if (e.altKey && e.key.length === 1) {
      const i = menus.findIndex((m) => m.label.toLowerCase().includes('&' + e.key.toLowerCase()))
      if (i >= 0) { openAt(i); return true }
    }
    return false
  }

  function pick(id: string) {
    open = -1
    onpick(id)
  }
</script>

<div class="menubar" role="menubar">
  {#each menus as m, i}
    <button
      bind:this={buttons[i]}
      class:open={open === i}
      onmousedown={(e) => { e.preventDefault(); if (open === i) open = -1; else openAt(i) }}
      onmouseenter={() => { if (open >= 0 && open !== i) openAt(i) }}
    ><Label text={m.label} /></button>
  {/each}
</div>
{#if open >= 0 && menus[open]}
  <!-- svelte-ignore a11y_no_static_element_interactions a11y_click_events_have_key_events -->
  <div style="position:fixed;inset:0;z-index:999" onmousedown={() => (open = -1)}></div>
  <Menu bind:this={popup} items={menus[open].items} x={pos.x} y={pos.y} {theme} onpick={pick} onclose={() => (open = -1)} />
{/if}
