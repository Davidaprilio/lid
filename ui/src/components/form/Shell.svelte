<script lang="ts">
  // The frame of every dialog (1.x widgets.shell): header (icon, title, subtitle), body, footer.
  import type { Snippet } from 'svelte'
  import Icon from '../Icon.svelte'
  import type { ThemeName } from '../../theme/tokens'

  let { icon = '', title = '', subtitle = '', tile = false, badge = '', theme, iconSize = 0, children, footer }: {
    icon?: string; title?: string; subtitle?: string; tile?: boolean; badge?: string; theme: ThemeName; iconSize?: number
    children: Snippet; footer?: Snippet
  } = $props()
</script>

<div class="shell">
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
