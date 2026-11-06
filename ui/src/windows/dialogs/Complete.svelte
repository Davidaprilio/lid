<script lang="ts">
  // "Download complete" (1.x progress.CompleteDialog).
  import Row from '../../components/form/Row.svelte'
  import Icon from '../../components/Icon.svelte'
  import Dialog, { type Box } from '../../components/Dialog.svelte'
  import { call, close, followTheme, fit, params } from '../../lib/dialog'
  import type { ThemeName } from '../../theme/tokens'

  const id = Number(params().get('id'))
  let theme = $state<ThemeName>('ember')
  let info = $state<any>(null)
  let dontShow = $state(false)
  let box = $state<Box | null>(null)
  followTheme((t) => (theme = t))
  call<any>('complete_info', { id }).then((i) => { if (!i) return close(); info = i; fit(640) })

  // Rust applies the box when the window closes, also through its title bar.
  $effect(() => { call('complete_dont_show', { on: dontShow }) })
  function done() {
    close()
  }

  async function openWith() {
    const r = await new Promise<{ button: string; value: string }>((resolve) => {
      box = { title: 'Open with', text: 'Command (the file path is appended):', buttons: [{ id: 'ok', label: 'OK', primary: true }, { id: 'cancel', label: 'Cancel' }],
        input: { label: '', value: '' }, resolve: (x) => { box = null; resolve(x) } }
    })
    if (r.button === 'ok' && r.value.trim()) {
      await call('act_open_with', { id, command: r.value }).catch(() => {})
      done()
    }
  }
</script>

<svelte:window onkeydown={(e) => { if (!box && (e.key === 'Escape' || e.key === 'Enter')) done() }} />
{#if info}
  <div class="shell">
    <div class="shell-body">
      <div class="shell-head">
        <div class="badgecircle ok">
          {#if info.verified}<Icon name="shield-check" size={22} {theme} />{:else}
            <svg width="22" height="22" viewBox="0 0 24 24"><path d="M5 12.5l4.5 4.5L19 7.5" fill="none" stroke="var(--ok)" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"/></svg>{/if}
        </div>
        <div class="shell-titles">
          <div class="dialogTitle">Download complete</div>
          <div class="muted">{info.downloaded}</div>
        </div>
        {#if info.verified}<span class="tpill ok" style="margin-left:auto">{info.kind} verified</span>{/if}
      </div>
      <Row label="Address" width={130}><input class="input mono" readonly value={info.url} /></Row>
      <Row label="The file saved as" width={130}>
        <div style="position:relative;flex:1">
          <span style="position:absolute;left:8px;top:6px"><Icon name={info.icon} size={16} {theme} /></span>
          <input class="input" style="width:100%;padding-left:30px" readonly value={info.path} />
        </div>
      </Row>
      {#if info.verified}
        <div class="caption">Checksum ({info.kind}) · matches</div>
        <div style="position:relative">
          <input class="input mono" style="width:100%;padding-right:30px" readonly value={info.actual} />
          <span class="okText" style="position:absolute;right:9px;top:50%;transform:translateY(-50%);font-weight:700">✓</span>
        </div>
      {/if}
    </div>
    <div class="shell-footer">
      <label class="check"><input type="checkbox" bind:checked={dontShow} /> Don't show this dialog again</label>
      <span class="grow"></span>
      <button class="btn primary" onclick={async () => { await call('act_open', { id }).catch(() => {}); done() }}>Open</button>
      <button class="btn" onclick={openWith}>Open with...</button>
      <button class="btn" onclick={async () => { await call('act_open_folder', { id }).catch(() => {}); done() }}>Open folder</button>
      <button class="btn" onclick={done}>Close</button>
    </div>
  </div>
{/if}
{#if box}<Dialog {box} {theme} />{/if}
