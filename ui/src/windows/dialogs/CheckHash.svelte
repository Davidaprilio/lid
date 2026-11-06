<script lang="ts">
  // "Check hash" (1.x checkhash.CheckHashDialog): compare a finished file with a pasted value, or only
  // compute its hash; for an unfinished download, check it as soon as it finishes.
  import Shell from '../../components/form/Shell.svelte'
  import Row from '../../components/form/Row.svelte'
  import ChecksumEdit from '../../components/form/ChecksumEdit.svelte'
  import Icon from '../../components/Icon.svelte'
  import { call, close, followTheme, fit, params } from '../../lib/dialog'
  import type { ThemeName } from '../../theme/tokens'

  const READY = "Pre-filled with the checksum stored for this download. Paste the value published by the site, or leave it empty to only compute the file's hash."
  const RUNNING = 'Paste the value published by the site. Computing a hash without a value is available once the file is complete.'
  const id = Number(params().get('id'))
  type St = 'ready' | 'reading' | 'match' | 'mismatch' | 'computed' | 'running'
  let theme = $state<ThemeName>('ember')
  let info = $state<any>(null)
  let st = $state<St>('ready')
  let text = $state('')
  let algorithm = $state('auto')
  let valid = $state(true)
  let value = $state('')
  let saveIt = $state(true)
  let token: number | null = null
  let poll = $state<any>(null)
  let readError = $state('')
  let saved = $state(false)
  let lastText = ''

  followTheme((t) => (theme = t))
  call<any>('checkhash_info', { id }).then((i) => {
    if (!i) return close()
    info = i
    if (i.checksum) text = i.checksum.split(':')[1] ?? ''
    lastText = text
    st = i.finished ? 'ready' : 'running'
    fit(640)
  })

  $effect(() => {
    // Editing the value while a result shows returns to "ready".
    if (text !== lastText) {
      lastText = text
      if (st === 'match' || st === 'mismatch' || st === 'computed') st = 'ready'
    }
  })
  $effect(() => { void st; fit(640) })

  const runLabel = $derived(text.trim() ? 'Check now' : 'Compute hash')
  const canRun = $derived(valid && st === 'ready' && !!info?.exists)
  const canSave = $derived(valid && !!value && (st === 'ready' || st === 'reading'))

  async function run() {
    const algo = value ? value.split(':')[0] : algorithm === 'auto' ? 'sha256' : algorithm
    readError = ''
    token = await call<number>('checkhash_start', { id, checksum: value || `${algo}:` })
    st = 'reading'
    const timer = setInterval(async () => {
      if (token === null) return clearInterval(timer)
      const p = await call<any>('checkhash_poll', { token })
      if (!p) return clearInterval(timer)
      poll = p
      if (!p.done) return
      clearInterval(timer)
      call('checkhash_stop', { token })
      token = null
      if (p.result === 'error') { readError = `Could not read the file: ${p.error}`; st = 'ready'; return }
      if (p.result === 'skipped') { st = 'ready'; return }
      saved = false
      if (saveIt && value && (p.result === 'verified' || p.result === 'mismatch')) {
        await call('checkhash_save', { id, checksum: value, result: p.result, actual: p.actual, seconds: p.seconds })
        saved = true
      }
      st = p.result === 'verified' ? 'match' : p.result === 'mismatch' ? 'mismatch' : 'computed'
    }, 100)
  }

  function stop() {
    if (token !== null) call('checkhash_stop', { token })
    token = null
    st = 'ready'
  }

  async function choose(choice: string) {
    await call('mismatch_choice', { id, choice })
    close()
  }

  async function later() {
    await call('checkhash_later', { id, checksum: value })
    close()
  }

  function quit() {
    if (token !== null) call('checkhash_stop', { token })
    close()
  }

  const kind = $derived(poll?.algorithm || 'SHA-256')
  // The default button of each state takes the focus (paste into the field with a click).
  $effect(() => {
    void st
    queueMicrotask(() => document.querySelector<HTMLButtonElement>('.shell-footer button.primary')?.focus())
  })
</script>

<svelte:window onkeydown={(e) => {
  if (e.key === 'Escape') quit()
  // Enter presses the default button; a disabled one does nothing (1.x QDialog).
  if (e.key === 'Enter' && (e.target as HTMLElement).tagName !== 'BUTTON') { if (st === 'ready') { if (canRun) run() } else if (st === 'running') { if (valid && value) later() } else if (st === 'mismatch') choose('again'); else if (st === 'match' || st === 'computed') quit() }
}} />
{#if info}
  <Shell {theme} icon="checksum" badge="accent" title="Check hash" subtitle={info.subtitle}>
    <Row label="Expected checksum" width={140} top>
      <div style="flex:1;display:flex;flex-direction:column;gap:4px">
        <ChecksumEdit bind:text bind:algorithm bind:valid bind:value hint={st === 'running' ? RUNNING : READY} disabled={st === 'reading'} />
      </div>
    </Row>
    {#if st !== 'running'}
      <label class="check"><input type="checkbox" bind:checked={saveIt} disabled={!canSave} /> Save this checksum to the download</label>
    {/if}
    {#if st === 'running'}
      <div class="accentBox"><Icon name="clock" size={16} {theme} />The file is still downloading{info.percent ? ` (${info.percent})` : ''}. LID checks it as soon as the download finishes.</div>
    {/if}
    {#if st === 'ready' && info.finished && !info.exists}<div class="errorText">File not found at {info.path}</div>{/if}
    {#if st === 'ready' && readError}<div class="errorText">{readError}</div>{/if}
    {#if st === 'reading' && poll}
      <div class="panel">
        <div style="display:flex;align-items:center;gap:8px">
          <b>Reading the file… {Math.round(poll.progress * 100)}%</b>
          <span class="grow" style="flex:1"></span>
          <button class="btn small" style="min-width:60px" onclick={stop}>Stop</button>
        </div>
        <div class="mono muted">{poll.read} of {poll.total}</div>
        <div class="bar"><div style:width="{Math.round(poll.progress * 100)}%"></div></div>
        <div class="faint">{kind} · runs in the background; the list stays usable.</div>
      </div>
    {/if}
    {#if st === 'match' && poll}
      <div class="matchBox">
        <div style="display:flex;align-items:center;gap:8px"><Icon name="shield-check" size={18} {theme} /><b class="okText">Matches — the file is intact</b><span style="flex:1"></span><span class="muted">{kind} · {poll.seconds.toFixed(1)} s</span></div>
        <div class="caption">Computed {kind}</div>
        <div style="display:flex;gap:8px"><input class="input mono" style="flex:1" readonly value={poll.actual} /><button class="btn" onclick={() => call('copy_text', { text: poll.actual })}>Copy</button></div>
      </div>
      <div class="faint">{saved ? 'Saved to the download — the list shows ✓ Verified.' : 'Not saved — the download is unchanged.'}</div>
    {/if}
    {#if st === 'mismatch' && poll}
      <div class="mismatchBox">
        <Row label="Expected" width={80}><span class="mono" style="user-select:text;overflow-wrap:anywhere">{value.split(':')[1]}</span></Row>
        <Row label="Actual" width={80}><span class="mono badText" style="user-select:text;overflow-wrap:anywhere">{poll.actual}</span></Row>
        <div class="faint">{kind}. Expected is the value above; Actual is computed from the file on disk.{saved ? ' Saved to the download — the list shows Checksum mismatch.' : ''}</div>
      </div>
    {/if}
    {#if st === 'computed' && poll}
      <div class="panel">
        <div class="caption">{kind} of the file</div>
        <div style="display:flex;gap:8px"><input class="input mono" style="flex:1" readonly value={poll.actual} /><button class="btn" onclick={() => call('copy_text', { text: poll.actual })}>Copy</button></div>
      </div>
      <div class="faint">No expected value, so there is no verdict and nothing is saved. Pick MD5, SHA-1 or SHA-512 above to compute another type.</div>
    {/if}
    {#snippet footer()}
      {#if st === 'mismatch'}
        <button class="btn dangerBtn" onclick={() => choose('delete')}><Icon name="delete" size={14} {theme} /> Delete file</button>
        <span class="grow"></span>
        <button class="btn primary" onclick={() => choose('again')}>Download again</button>
        <button class="btn" onclick={() => choose('keep')}>Keep file</button>
      {:else if st === 'match' || st === 'computed'}
        <span class="grow"></span>
        <button class="btn" onclick={() => (st = 'ready')}>Check again</button>
        <button class="btn primary" onclick={quit}>Close</button>
      {:else if st === 'running'}
        <span class="grow"></span>
        <button class="btn" onclick={quit}>Close</button>
        <button class="btn primary" disabled={!(valid && value)} onclick={later}>Check when finished</button>
      {:else}
        <span class="grow"></span>
        <button class="btn" onclick={quit}>Close</button>
        <button class="btn primary" disabled={!canRun} onclick={run}>{runLabel}</button>
      {/if}
    {/snippet}
  </Shell>
{/if}
