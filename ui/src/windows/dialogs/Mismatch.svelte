<script lang="ts">
  // "Checksum mismatch" (1.x progress.MismatchDialog): download again, delete (to the Trash) or keep.
  import Shell from '../../components/form/Shell.svelte'
  import Icon from '../../components/Icon.svelte'
  import { call, close, followTheme, fit, params } from '../../lib/dialog'
  import type { ThemeName } from '../../theme/tokens'

  const id = Number(params().get('id'))
  let theme = $state<ThemeName>('ember')
  let info = $state<any>(null)
  followTheme((t) => (theme = t))
  call<any>('complete_info', { id }).then((i) => { if (!i) return close(); info = i; fit(640) })

  async function choose(choice: string) {
    await call('mismatch_choice', { id, choice })
    close()
  }
</script>

<svelte:window onkeydown={(e) => { if (e.key === 'Escape') close(); if (e.key === 'Enter') choose('again') }} />
{#if info}
  <Shell {theme} icon="shield-alert" badge="bad" title="The file doesn't match its checksum"
    subtitle="{info.filename} · {info.size}. It may be corrupt or changed on the server.">
    <div class="mismatchBox">
      <div class="pairGrid">
        <span class="caption">Expected</span><span class="mono" style="user-select:text;overflow-wrap:anywhere">{info.expected}</span>
        <span class="caption">Actual</span><span class="mono badText" style="user-select:text;overflow-wrap:anywhere">{info.actual}</span>
      </div>
      <div class="faint">{info.kind}. Expected is the value you entered; Actual is computed from the downloaded file after its parts were merged.</div>
    </div>
    <div class="caption" style="margin-top:4px">The file saved as</div>
    <div class="combo-like"><Icon name={info.icon} size={16} {theme} /><input class="mono" readonly tabindex="-1" value={info.path} /></div>
    {#snippet footer()}
      <button class="btn dangerBtn" onclick={() => choose('delete')}><Icon name="delete" size={14} {theme} /> Delete file</button>
      <span class="grow"></span>
      <button class="btn primary" onclick={() => choose('again')}>Download again</button>
      <button class="btn" onclick={() => choose('keep')}>Keep file</button>
    {/snippet}
  </Shell>
{/if}
