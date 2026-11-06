<script lang="ts" module>
  export interface ComboItem { value: string; label?: string; icon?: string }
</script>

<script lang="ts">
  // A QComboBox: editable (a text field with a drop-down of every item, like the address history) or a plain
  // choice whose items can carry icons (the category combos). `link` adds the leading link icon of
  // widgets.link_field / add.link_combo, which also makes the text monospaced.
  import Icon from '../Icon.svelte'
  import type { ThemeName } from '../../theme/tokens'

  let {
    value = $bindable(''), items, editable = false, link = false, mono = false, disabled = false, theme = 'ember' as ThemeName,
    input = $bindable(), onchange, style = '',
  }: {
    value?: string; items: (ComboItem | string)[]; editable?: boolean; link?: boolean; mono?: boolean; disabled?: boolean
    theme?: ThemeName; input?: HTMLInputElement; onchange?: (v: string) => void; style?: string
  } = $props()

  const list = $derived(items.map((i) => (typeof i === 'string' ? { value: i } : i)))
  const current = $derived(list.find((i) => i.value === value))
  let frame: HTMLDivElement | undefined = $state()
  let popup = $state<{ x: number; y: number; w: number; h: number; up: boolean } | null>(null)
  let active = $state(-1)

  function open() {
    if (disabled || !list.length || !frame) return
    const r = frame.getBoundingClientRect()
    const below = window.innerHeight - r.bottom - 6
    const above = r.top - 6
    const want = Math.min(list.length * 30 + 12, 300)
    const up = below < want && above > below
    popup = { x: r.left, y: up ? r.top - 2 : r.bottom + 2, w: r.width, h: Math.max(60, Math.min(want, up ? above : below)), up }
    active = Math.max(0, list.findIndex((i) => i.value === value))
  }
  function pick(i: number) {
    const item = list[i]
    popup = null
    if (!item) return
    value = item.value
    onchange?.(item.value)
    input?.focus()
  }
  function key(e: KeyboardEvent) {
    if (e.altKey && e.key === 'ArrowDown') { e.preventDefault(); popup ? (popup = null) : open(); return }
    if (!popup) {
      if (!editable && (e.key === 'ArrowDown' || e.key === 'ArrowUp')) {
        e.preventDefault()
        const i = list.findIndex((x) => x.value === value) + (e.key === 'ArrowDown' ? 1 : -1)
        if (i >= 0 && i < list.length) pick(i)
      }
      return
    }
    if (e.key === 'ArrowDown') { e.preventDefault(); active = Math.min(list.length - 1, active + 1) }
    else if (e.key === 'ArrowUp') { e.preventDefault(); active = Math.max(0, active - 1) }
    else if (e.key === 'Enter') { e.preventDefault(); e.stopPropagation(); pick(active) }
    else if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); popup = null }
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="combo" class:disabled bind:this={frame} {style} onkeydown={key}>
  {#if link}<span class="lead"><Icon name="link" size={15} {theme} /></span>{/if}
  {#if editable}
    <input class="comboText" class:mono={mono || link} bind:this={input} bind:value {disabled}
      oninput={() => onchange?.(value)} />
  {:else}
    <button type="button" class="comboText choice" class:mono {disabled} onclick={() => (popup ? (popup = null) : open())}>
      {#if current?.icon}<Icon name={current.icon} size={16} {theme} />{/if}
      <span>{current?.label ?? current?.value ?? value}</span>
    </button>
  {/if}
  <button type="button" class="caret" tabindex="-1" {disabled} aria-label="Show the list" onclick={() => (popup ? (popup = null) : open())}>
    <span class="tri">▾</span>
  </button>
</div>
{#if popup}
  <!-- svelte-ignore a11y_no_static_element_interactions a11y_click_events_have_key_events -->
  <div style="position:fixed;inset:0;z-index:999" onmousedown={() => (popup = null)}></div>
  <div class="menu comboList" role="listbox"
    style:left="{popup.x}px" style:top={popup.up ? null : `${popup.y}px`} style:bottom={popup.up ? `${window.innerHeight - popup.y}px` : null}
    style:min-width="{popup.w}px" style:max-height="{popup.h}px">
    {#each list as item, i}
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <div class="item" class:active={i === active} class:mono={mono || link} role="option" tabindex="-1" aria-selected={item.value === value}
        onmouseenter={() => (active = i)} onmousedown={(e) => e.preventDefault()} onclick={() => pick(i)}>
        {#if item.icon}<Icon name={item.icon} size={16} {theme} />{/if}
        <span>{item.label ?? item.value}</span>
      </div>
    {/each}
  </div>
{/if}

<style>
  .combo {
    display: flex; align-items: center; flex: 1; min-width: 0; background: var(--input); border: 1px solid var(--line2);
    border-radius: 8px; min-height: 28px; padding-left: 4px;
  }
  .combo:focus-within { border-color: var(--ac); }
  .combo.disabled { background: var(--win); color: var(--fa); }
  .lead { display: inline-flex; padding: 0 2px 0 4px; }
  .comboText { flex: 1; min-width: 0; border: 0; background: none; padding: 4px 5px; outline: none; color: inherit; font: inherit; }
  .comboText.mono { font-family: var(--mono); }
  .choice { display: flex; align-items: center; gap: 8px; text-align: left; cursor: pointer; }
  .choice span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .caret { border: 0; background: none; padding: 0 9px; align-self: stretch; display: inline-flex; align-items: center; cursor: pointer; }
  .tri { color: var(--mu); font-size: 0.9em; }
  .comboList { overflow-y: auto; padding: 4px; min-width: 0; }
  .comboList .item { padding: 5px 12px; }
  .comboList .item.mono { font-family: var(--mono); }
</style>
