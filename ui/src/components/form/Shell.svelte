<script lang="ts">
  // The frame of every dialog (1.x widgets.shell): header (icon, title, subtitle), body, footer.
  import type { Snippet } from 'svelte'
  import Icon from '../Icon.svelte'
  import { close } from '../../lib/dialog'
  import type { ThemeName } from '../../theme/tokens'

  let { icon = '', title = '', subtitle = '', tile = false, badge = '', theme, iconSize = 0, children, footer, keys = false, oncancel }: {
    icon?: string; title?: string; subtitle?: string; tile?: boolean; badge?: string; theme: ThemeName; iconSize?: number
    children: Snippet; footer?: Snippet
    /** QDialog keys: Enter presses the primary footer button, Escape cancels (`oncancel`, or closes the window). */
    keys?: boolean; oncancel?: () => void
  } = $props()
  let root: HTMLDivElement | undefined = $state()

  /** Only the top frame answers keys: a frame under an overlay (a nested dialog or a message box) does not. */
  function onTop() {
    if (!root) return false
    const overlays = Array.from(document.querySelectorAll('.overlay'))
    return overlays.length === 0 || root.closest('.overlay') === overlays[overlays.length - 1]
  }

  function key(e: KeyboardEvent) {
    if (e.defaultPrevented || !onTop()) return
    if (!keys || e.altKey || e.ctrlKey) return
    if (e.key === 'Escape') {
      e.preventDefault()
      oncancel ? oncancel() : close()
    } else if (e.key === 'Enter') {
      const t = e.target as HTMLElement
      if (t.tagName === 'TEXTAREA' || t.tagName === 'BUTTON' || t.tagName === 'SELECT') return
      const ok = root!.querySelector<HTMLButtonElement>('.shell-footer button.primary')
      if (ok && !ok.disabled) {
        e.preventDefault()
        ok.click()
      }
    }
  }
</script>

<svelte:window onkeydown={key} />

<div class="shell" bind:this={root}>
  <div class="shell-body">
    {#if title}
      <div class="shell-head">
        {#if icon}
          {#if badge}
            <div class="badgecircle {badge === 'accent' ? '' : badge}"><Icon name={icon} size={iconSize || 22} {theme} /></div>
          {:else if tile}
            <div class="tile small"><Icon name={icon} size={iconSize || 22} {theme} /></div>
          {:else}
            <Icon name={icon} size={iconSize || 30} {theme} />
          {/if}
        {/if}
        <div class="shell-titles">
          <div class="dialogTitle">{title}</div>
          {#if subtitle}<div class="muted wrap">{subtitle}</div>{/if}
        </div>
      </div>
    {/if}
    {@render children()}
  </div>
  {#if footer}<div class="shell-footer">{@render footer()}</div>{/if}
</div>
