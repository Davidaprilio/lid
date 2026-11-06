<script lang="ts">
  // "Choose Font..." for the download list: a family from fontconfig and a size.
  import Shell from '../../components/form/Shell.svelte'
  import Row from '../../components/form/Row.svelte'
  import { call, close, followTheme, fit } from '../../lib/dialog'
  import type { ThemeName } from '../../theme/tokens'

  let theme = $state<ThemeName>('ember')
  let families = $state<string[]>([])
  let family = $state('')
  let size = $state(10)
  let filter = $state('')
  followTheme((t) => (theme = t))
  Promise.all([call<string[]>('system_fonts'), call<any>('settings_get')]).then(([f, s]) => {
    families = f
    const parts = (s.list_font as string).split(',')
    family = parts[0] || f.find((x) => /^(Noto Sans|Cantarell|DejaVu Sans)$/.test(x)) || f[0] || ''
    size = Number(parts[1]) || 10
    fit(460)
  })
  const shown = $derived(families.filter((f) => f.toLowerCase().includes(filter.toLowerCase())))

  async function ok() {
    if (family) await call('set_list_font', { family, size })
    close()
  }
</script>

<Shell {theme} icon="general" title="Choose Font" subtitle="The font of the download list">
  <input class="input" placeholder="Search fonts" bind:value={filter} />
  <div class="list2" style="height:260px">
    {#each shown as f (f)}
      <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
      <div class="r" role="option" aria-selected={f === family} tabindex="-1" class:sel={f === family} style:font-family="'{f}'" onclick={() => (family = f)}>{f}</div>
    {/each}
  </div>
  <Row label="Size" width={60}><input class="input mono" type="number" min="6" max="48" step="0.5" style="width:90px" bind:value={size} /><span class="formLabel">pt</span></Row>
  <div class="noteBox" style:font-family="'{family}'" style:font-size="{size * 1.333}px">AaBbCc 0123 · ubuntu-24.04.iso</div>
  {#snippet footer()}
    <span class="grow"></span>
    <button class="btn" onclick={close}>Cancel</button>
    <button class="btn primary" onclick={ok}>OK</button>
  {/snippet}
</Shell>
