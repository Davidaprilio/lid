<script lang="ts">
  // "File Properties" (1.x properties.PropertiesDialog): rename/move, address, description, checksum,
  // referer and login of one download.
  import { open as openDir } from '@tauri-apps/plugin-dialog'
  import Row from '../../components/form/Row.svelte'
  import Pill from '../../components/form/Pill.svelte'
  import ChecksumEdit from '../../components/form/ChecksumEdit.svelte'
  import Icon from '../../components/Icon.svelte'
  import Dialog, { type Box } from '../../components/Dialog.svelte'
  import { call, close, followTheme, fit, params } from '../../lib/dialog'
  import type { ThemeName } from '../../theme/tokens'

  const id = Number(params().get('id'))
  let theme = $state<ThemeName>('ember')
  let info = $state<any>(null)
  let filename = $state('')
  let saveDir = $state('')
  let url = $state('')
  let description = $state('')
  let referer = $state('')
  let username = $state('')
  let password = $state('')
  let text = $state('')
  let algorithm = $state('auto')
  let valid = $state(true)
  let value = $state('')
  let box = $state<Box | null>(null)
  let okButton: HTMLButtonElement | undefined = $state()

  followTheme((t) => (theme = t))
  async function load(first: boolean) {
    const i = await call<any>('properties_info', { id })
    if (!i) return close()
    info = i
    if (first) {
      const d = i.download
      filename = d.filename
      saveDir = d.save_dir
      url = d.url
      description = d.description
      referer = d.referer
      username = d.username
      password = d.password
      text = d.checksum ? d.checksum.split(':')[1] : ''
      fit(680).then(() => okButton?.focus())
    }
  }
  load(true)
  const timer = setInterval(() => load(false), 300)
  $effect(() => () => clearInterval(timer))

  function warn(t: string) {
    return new Promise<void>((resolve) => {
      box = { title: 'File Properties', text: t, icon: 'info', badge: 'bad', buttons: [{ id: 'ok', label: 'OK', primary: true }], resolve: () => { box = null; resolve() } }
    })
  }

  async function browse() {
    const d = await openDir({ directory: true, title: 'Save To', defaultPath: saveDir })
    if (typeof d === 'string') saveDir = d
  }

  async function verifyNow() {
    await call('properties_verify', { id, checksum: value })
  }

  async function ok() {
    if (!valid) {
      const r = await call<any>('checksum_parse', { text, algorithm })
      return warn(r.error)
    }
    try {
      await call('properties_apply', { id, patch: { filename, save_dir: saveDir, url, description, referer: referer.trim(), username, password, checksum: value } })
      close()
    } catch (e) {
      warn(String(e))
    }
  }
  const canVerify = $derived(valid && !!value && !info?.verifying && info?.complete)
</script>

<svelte:window onkeydown={(e) => { if (box) return; if (e.key === 'Escape') close(); if (e.key === 'Enter' && (e.target as HTMLElement).tagName !== 'BUTTON') ok() }} />
{#if info}
  <div class="shell">
    <div class="shell-body">
      <div style="display:flex;gap:12px;align-items:center">
        <div class="tile"><Icon name={info.icon} size={30} {theme} /></div>
        <div style="flex:1;display:flex;flex-direction:column;gap:6px;min-width:0">
          <input class="input" style="font-weight:700" bind:value={filename} />
          <div style="display:flex;gap:6px"><Pill text={info.status.text} tone={info.status.tone} /><Pill text={info.download.category} /></div>
        </div>
      </div>
      <div class="panel">
        <Row label="Type" width={100}><span>{info.type}</span></Row>
        <Row label="Size" width={100}><span class="mono">{info.size}</span></Row>
        {#if info.variant}<Row label="YouTube" width={100}><span>{info.variant}</span></Row>{/if}
        <Row label="Save To" width={100}><input class="input mono" bind:value={saveDir} /><button class="btn" onclick={browse}><u>B</u>rowse...</button></Row>
        <Row label="Address" width={100}><input class="input mono" bind:value={url} readonly={info.youtube} /></Row>
        <Row label="Description" width={100}><input class="input" bind:value={description} /></Row>
      </div>
      <div class="panel">
        <div style="display:flex;gap:8px;align-items:center"><span class="caption">Checksum</span><Pill text={info.checksum_pill.text} tone={info.checksum_pill.tone} /></div>
        <Row label="Expected" width={100} top>
          <div style="flex:1;display:flex;flex-direction:column;gap:4px"><ChecksumEdit bind:text bind:algorithm bind:valid bind:value hint="" /></div>
        </Row>
        <Row label="Result" width={100}>
          <span class={info.checksum_tone} style="flex:1">{info.checksum_text}</span>
          <button class="btn" disabled={!canVerify} onclick={verifyNow}>Verify now</button>
        </Row>
      </div>
      <div class="panel">
        <div class="caption">Web page and authorization</div>
        <Row label="Parent page" width={100}>
          <span class="mono {info.download.parent_page ? 'link' : 'muted'}" style="user-select:text;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;min-width:0">{info.download.parent_page || 'Unknown'}</span>
        </Row>
        <Row label="Referer" width={100}><input class="input mono" bind:value={referer} /></Row>
        <div style="display:flex;gap:10px">
          <Row label="Login" width={100}><input class="input" bind:value={username} /></Row>
          <Row label="Password" width={70}><input class="input" type="password" bind:value={password} /></Row>
        </div>
      </div>
    </div>
    <div class="shell-footer">
      <span class="faint" style="flex:1;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">{info.footer}</span>
      <button class="btn" disabled={!info.complete} onclick={() => call('act_open', { id })}>Open</button>
      <button class="btn primary" bind:this={okButton} onclick={ok}>OK</button>
    </div>
  </div>
{/if}
{#if box}<Dialog {box} {theme} />{/if}
