<script lang="ts">
  import { iconSvg } from '../lib/icons'
  import { token, type ThemeName } from '../theme/tokens'

  let { name, size = 16, theme = 'ember' as ThemeName, color = '' }: { name: string; size?: number; theme?: ThemeName; color?: string } = $props()
  const html = $derived(iconSvg(name, size, token(theme, 'icon_ac')))
</script>

<span class="icon" style:width="{size}px" style:height="{size}px" style:color={color || null}>{@html html}</span>
