<script lang="ts">
  // "Checksum mismatch" (1.x progress.MismatchDialog): download again, delete (to the Trash) or keep.
  import Shell from '../../components/form/Shell.svelte'
  import Row from '../../components/form/Row.svelte'
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
      <Row label="Expected" width={80}><span class="mono" style="user-select:text;overflow-wrap:anywhere">{info.expected}</span></Row>
      <Row label="Actual" width={80}><span class="mono badText" style="user-select:text;overflow-wrap:anywhere">{info.actual}</span></Row>
      <div class="faint">{info.kind}. Expected is the value you entered; Actual is computed from the downloaded file after its parts were merged.</div>
    </div>
    <Row label="The file saved as" width={130}>
      <div style="position:relative;flex:1">
        <span style="position:absolute;left:8px;top:6px"><Icon name={info.icon} size={16} {theme} /></span>
        <input class="input" style="width:100%;padding-left:30px" readonly value={info.path} />
      </div>
    </Row>
    {#snippet footer()}
      <button class="btn dangerBtn" onclick={() => choose('delete')}><Icon name="delete" size={14} {theme} /> Delete file</button>
      <span class="grow"></span>
      <button class="btn" onclick={() => choose('keep')}>Keep file</button>
      <button class="btn primary" onclick={() => choose('again')}>Download again</button>
    {/snippet}
  </Shell>
{/if}
