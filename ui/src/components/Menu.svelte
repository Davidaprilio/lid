<script lang="ts">
  // A popup menu: the dropdowns of the menu bar, context menus and split buttons.
  import type { MenuItem } from '../lib/types'
  import Icon from './Icon.svelte'
  import Label from './Label.svelte'
  import Self from './Menu.svelte'
  import type { ThemeName } from '../theme/tokens'

  let {
    items, x, y, theme, onpick, onclose, sub = false,
  }: { items: MenuItem[]; x: number; y: number; theme: ThemeName; onpick: (id: string) => void; onclose: () => void; sub?: boolean } = $props()

  let el: HTMLDivElement | undefined = $state()
  let active = $state(-1)
  let openSub = $state(-1)
  let subPos = $state({ x: 0, y: 0 })
  let pos = $state({ x: 0, y: 0 })

  $effect(() => {
    // Keep the menu on screen.
    if (!el) return
    const r = el.getBoundingClientRect()
    pos = {
      x: Math.max(4, Math.min(x, window.innerWidth - r.width - 4)),
      y: Math.max(4, Math.min(y, window.innerHeight - r.height - 4)),
    }
  })

  const usable = (i: number) => items[i] && items[i].type !== 'separator' && !(items[i].type === 'action' && !(items[i] as any).enabled)

  function pick(i: number, ev?: Event) {
    const it = items[i]
    if (!it || it.type === 'separator') return
    if (it.type === 'submenu') {
      showSub(i, ev?.currentTarget as HTMLElement | undefined)
      return
    }
    if (!it.enabled) return
    onpick(it.id)
  }

  function showSub(i: number, target?: HTMLElement) {
    const row = target ?? (el?.querySelectorAll('.item, .sep')[i] as HTMLElement | undefined)
    if (!row) return
    const r = row.getBoundingClientRect()
    subPos = { x: r.right - 4, y: r.top - 6 }
    openSub = i
  }

  export function key(e: KeyboardEvent): boolean {
    if (openSub >= 0 && subRef?.key(e)) return true
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      const step = e.key === 'ArrowDown' ? 1 : -1
      for (let n = 1; n <= items.length; n++) {
        const i = (active + step * n + items.length * 2) % items.length
        if (usable(i)) { active = i; break }
      }
      return true
    }
    if (e.key === 'ArrowRight' && items[active]?.type === 'submenu') { showSub(active); return true }
    if (e.key === 'ArrowLeft' && sub) { onclose(); return true }
    if (e.key === 'Enter' || e.key === ' ') { pick(active); return true }
    if (e.key === 'Escape') { onclose(); return true }
    // A mnemonic letter.
    const letter = e.key.toLowerCase()
    const i = items.findIndex((it) => it.type !== 'separator' && (it as any).label.toLowerCase().includes('&' + letter))
    if (i >= 0 && usable(i)) { active = i; pick(i); return true }
    return false
  }

  let subRef: Self | undefined = $state()
</script>

<div class="menu" bind:this={el} style:left="{pos.x}px" style:top="{pos.y}px" role="menu" tabindex="-1">
  {#each items as it, i}
    {#if it.type === 'separator'}
      <div class="sep"></div>
    {:else}
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <div
        class="item"
        class:active={active === i || openSub === i}
        class:disabled={it.type === 'action' && !it.enabled}
        role="menuitem"
        tabindex="-1"
        title={it.type === 'action' ? it.tip : undefined}
        onmouseenter={(e) => { active = i; if (it.type === 'submenu') showSub(i, e.currentTarget as HTMLElement); else openSub = -1 }}
        onclick={(e) => pick(i, e)}
      >
        {#if it.type === 'action' && it.checked !== undefined}
          <span class="mark" class:on={it.checked}>{#if it.checked}<svg width="10" height="10" viewBox="0 0 24 24"><path d="M5 12.5l4.5 4.5L19 7.5" fill="none" stroke="#fff" stroke-width="3.2" stroke-linecap="round" stroke-linejoin="round"/></svg>{/if}</span>
        {:else if it.type === 'action' && it.icon}
          <Icon name={it.icon} size={16} {theme} />
        {:else}
          <span style="width:16px"></span>
        {/if}
        <span><Label text={it.label} /></span>
        {#if it.type === 'action' && it.shortcut}<span class="key">{it.shortcut}</span>{/if}
        {#if it.type === 'submenu'}<span class="chev">›</span>{/if}
      </div>
    {/if}
  {/each}
</div>
{#if openSub >= 0 && items[openSub]?.type === 'submenu'}
  <Self bind:this={subRef} items={(items[openSub] as any).items} x={subPos.x} y={subPos.y} {theme} {onpick} onclose={() => (openSub = -1)} sub />
{/if}
