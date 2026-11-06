<script lang="ts">
  // A message box over the window: a question, a note, a text input or a countdown.
  import Icon from './Icon.svelte'
  import type { ThemeName } from '../theme/tokens'

  export interface Box {
    title: string
    text: string
    icon?: string
    badge?: 'accent' | 'bad' | 'ok'
    buttons: { id: string; label: string; primary?: boolean; danger?: boolean; left?: boolean; icon?: string; default?: boolean }[]
    /** A list in a panel under the text: icon, name, a muted value. */
    items?: [string, string, string][]
    more?: string
    note?: string
    checkbox?: { label: string; checked: boolean }
    input?: { label: string; value: string }
    resolve: (r: { button: string; checked: boolean; value: string }) => void
  }
  let { box, theme }: { box: Box; theme: ThemeName } = $props()
  let checked = $state(false)
  let value = $state('')
  let field: HTMLInputElement | undefined = $state()

  $effect(() => {
    checked = box.checkbox?.checked ?? false
    value = box.input?.value ?? ''
    queueMicrotask(() => {
      if (field) { field.focus(); field.select() }
      else root?.querySelector<HTMLButtonElement>(`.footer button[data-id="${defaultButton().id}"]`)?.focus()
    })
  })

  /** The button Enter presses (and the one with focus): QMessageBox's default button. */
  function defaultButton() {
    return box.buttons.find((b) => b.default) ?? box.buttons.find((b) => b.primary && !b.left) ?? box.buttons.find((b) => b.primary) ?? box.buttons[0]
  }
  let root: HTMLDivElement | undefined = $state()

  function done(button: string) {
    box.resolve({ button, checked, value })
  }

  function key(e: KeyboardEvent) {
    if (e.key === 'Escape') { e.stopPropagation(); done('cancel') }
    if (e.key === 'Enter') {
      e.stopPropagation()
      e.preventDefault()
      const f = document.activeElement as HTMLElement | null
      done(f?.dataset.id && root?.contains(f) ? f.dataset.id : defaultButton().id)
    }
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="overlay" bind:this={root} onkeydown={key}>
  <div class="dialog" role="dialog" aria-modal="true">
    <div class="body">
      <div class="head">
        {#if box.icon}<div class="badgecircle {box.badge ?? ''}"><Icon name={box.icon} size={22} {theme} /></div>{/if}
        <div style="display:flex;flex-direction:column;gap:6px;min-width:0">
          <div class="title">{box.title}</div>
          <div class="text">{box.text}</div>
        </div>
      </div>
      {#if box.items?.length}
        <div class="panel" style="gap:6px">
          {#each box.items as [icon, name, extra]}
            <div style="display:flex;gap:8px;align-items:center"><Icon name={icon} size={16} {theme} /><span style="flex:1;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">{name}</span><span class="muted mono">{extra}</span></div>
          {/each}
          {#if box.more}<div class="faint">{box.more}</div>{/if}
        </div>
      {/if}
      {#if box.note}<div class="noteBox">{box.note}</div>{/if}
      {#if box.input}
        <label style="display:flex;flex-direction:column;gap:4px">
          <span class="muted">{box.input.label}</span>
          <input class="input" bind:this={field} bind:value />
        </label>
      {/if}
      {#if box.checkbox}
        <label class="check"><input type="checkbox" bind:checked /> {box.checkbox.label}</label>
      {/if}
    </div>
    <div class="footer">
      {#each box.buttons.filter((b) => b.left) as b}
        <button class="btn" data-id={b.id} class:primary={b.primary} class:dangerBtn={b.danger} onclick={() => done(b.id)}>{#if b.icon}<Icon name={b.icon} size={14} {theme} />{/if} {b.label}</button>
      {/each}
      <span style="flex:1"></span>
      {#each box.buttons.filter((b) => !b.left) as b}
        <button class="btn" data-id={b.id} class:primary={b.primary} class:dangerBtn={b.danger} onclick={() => done(b.id)}>{b.label}</button>
      {/each}
    </div>
  </div>
</div>
