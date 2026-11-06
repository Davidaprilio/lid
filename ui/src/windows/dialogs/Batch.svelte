<script lang="ts">
  // Batch download (1.x add.BatchDialog): one * in the address, replaced by numbers or letters.
  import Shell from '../../components/form/Shell.svelte'
  import Row from '../../components/form/Row.svelte'
  import Combo from '../../components/form/Combo.svelte'
  import Dialog, { type Box } from '../../components/Dialog.svelte'
  import { call, close, followTheme, fit, URL_RE } from '../../lib/dialog'
  import type { ThemeName } from '../../theme/tokens'

  let theme = $state<ThemeName>('ember')
  let address = $state('')
  let mode = $state<'numbers' | 'letters'>('numbers')
  let numFrom = $state(1)
  let numTo = $state(10)
  let width = $state(1)
  let letFrom = $state('a')
  let letTo = $state('z')
  let useAuth = $state(false)
  let login = $state('')
  let password = $state('')
  let urls = $state<string[]>([])
  let box = $state<Box | null>(null)
  followTheme((t) => (theme = t))
  /** QSpinBox ranges: a typed value outside them snaps back in when the field is left. */
  const clamp = (v: number, lo: number, hi: number) => Math.max(lo, Math.min(hi, Math.round(Number(v)) || lo))
  call<string>('clipboard_text').then((t) => { if (URL_RE.test(t.trim())) address = t.trim(); fit(620) })

  $effect(() => {
    const fill = mode === 'numbers'
      ? { mode, from: Math.max(0, Math.min(999999, numFrom | 0)), to: Math.max(0, Math.min(999999, numTo | 0)), width: Math.max(1, Math.min(9, width | 0)) }
      : { mode, from: letFrom, to: letTo }
    call<string[]>('batch_preview', { pattern: address, fill }).then((u) => (urls = u))
  })

  async function ok() {
    if (urls.length) {
      const n = await call<number>('act_add_many', { items: urls.map((u) => [u, '', '']), username: useAuth ? login : '', password: useAuth ? password : '' })
      if (n) {
        const yes = await new Promise<boolean>((resolve) => {
          box = { title: 'LID', text: `${n} file(s) were added to the "Main download queue". Start the queue now?`, icon: 'info',
            buttons: [{ id: 'yes', label: 'Yes', primary: true }, { id: 'no', label: 'No' }], resolve: (r) => { box = null; resolve(r.button === 'yes') } }
        })
        if (yes) await call('act_start_main_queue')
      }
    }
    close()
  }
</script>

<Shell {theme} keys icon="add-url" title="Batch download" subtitle="Add a numbered series of files: put one asterisk in the address, e.g. https://www.example.com/pictures/img*.jpg">
  <Row label="Address" width={70}><Combo editable link {theme} items={[]} bind:value={address} /></Row>
  <div class="panel">
    <div class="caption">Replace asterisk with</div>
    <div style="display:flex;gap:18px;align-items:center;flex-wrap:wrap">
      <label class="radio"><input type="radio" bind:group={mode} value="numbers" /> Numbers</label>
      <label class="radio"><input type="radio" bind:group={mode} value="letters" /> Letters</label>
      <span>From</span>
      {#if mode === 'numbers'}<input class="input mono nospin" type="number" min="0" max="999999" style="width:80px" bind:value={numFrom} onblur={() => (numFrom = clamp(numFrom, 0, 999999))} />
      {:else}<input class="input mono" maxlength="1" style="width:80px" bind:value={letFrom} />{/if}
      <span>To</span>
      {#if mode === 'numbers'}<input class="input mono nospin" type="number" min="0" max="999999" style="width:80px" bind:value={numTo} onblur={() => (numTo = clamp(numTo, 0, 999999))} />
      {:else}<input class="input mono" maxlength="1" style="width:80px" bind:value={letTo} />{/if}
      <span>Wildcard size</span>
      <input class="input mono nospin" type="number" min="1" max="9" style="width:80px" bind:value={width} onblur={() => (width = clamp(width, 1, 9))} disabled={mode !== 'numbers'} />
    </div>
  </div>
  <label class="check"><input type="checkbox" bind:checked={useAuth} /> Use <u>a</u>uthorization</label>
  {#if useAuth}
    <div style="display:flex;gap:10px">
      <Row label="Login" width={50}><input class="input" bind:value={login} /></Row>
      <Row label="Password" width={70}><input class="input" type="password" bind:value={password} /></Row>
    </div>
  {/if}
  <div class="panel">
    <Row label="First file" width={80}><span class="mono link" style="user-select:text">{urls[0] ?? ''}</span></Row>
    <Row label="Second file" width={80}><span class="mono link" style="user-select:text">{urls[1] ?? ''}</span></Row>
    <Row label="Last file" width={80}><span class="mono link" style="user-select:text">{urls.length ? urls[urls.length - 1] : ''}</span></Row>
  </div>
  {#snippet footer()}
    <span class="muted">{urls.length ? `${urls.length} files will be added` : 'Put one * in the address'}</span>
    <span class="grow"></span>
    <button class="btn" onclick={close}>Cancel</button>
    <button class="btn primary" onclick={ok}>OK</button>
  {/snippet}
</Shell>
{#if box}<Dialog {box} {theme} />{/if}
