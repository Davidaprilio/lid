<script lang="ts">
  // The progress window of one download (1.x progress.ProgressDialog), refreshed every 500 ms.
  import TabStrip from '../../components/form/TabStrip.svelte'
  import Row from '../../components/form/Row.svelte'
  import Pill from '../../components/form/Pill.svelte'
  import Icon from '../../components/Icon.svelte'
  import SegmentBar from '../../components/SegmentBar.svelte'
  import { call, close, followTheme, fit, params, setTitle } from '../../lib/dialog'
  import type { ThemeName } from '../../theme/tokens'

  const id = Number(params().get('id'))
  let theme = $state<ThemeName>('ember')
  let tab = $state(0)
  let info = $state<any>(null)
  let details = $state(true)
  let limitOn = $state(false)
  let limitKbps = $state(200)
  let rememberLimit = $state(true)
  let showComplete = $state(true)
  let exitDone = $state(false)
  let shutdownDone = $state(false)
  let started = false

  followTheme((t) => (theme = t))

  async function refresh() {
    const i = await call<any>('progress_info', { id })
    if (!i) return close()
    info = i
    setTitle(i.title)
    if (!started) {
      started = true
      limitOn = i.limit_on
      limitKbps = i.limit_kbps
      showComplete = i.show_complete
      sendOptions()
      fit(640)
    }
  }
  refresh()
  const timer = setInterval(refresh, 500)
  $effect(() => () => clearInterval(timer))

  function sendOptions() {
    call('set_completion_options', { options: { show_complete: showComplete, exit: exitDone, shutdown: shutdownDone } })
  }

  function applyLimit() {
    const k = Math.max(1, Math.min(10_000_000, Math.round(limitKbps || 1)))
    limitKbps = k
    call('progress_limit', { id, kbps: limitOn ? k : 0, remember: rememberLimit })
  }

  async function toggleDetails() {
    details = !details
    await fit(640)
  }

  async function cancel() {
    await call('progress_button', { id, action: 'pause' })
    close()
  }
</script>

{#if info}
  <div class="shell">
    <TabStrip names={['Download status', 'Speed Limiter', 'Options on completion']} bind:active={tab} />
    <div class="shell-body">
      {#if tab === 0}
        <div class="mono" style="user-select:text;overflow-wrap:anywhere">{info.url}</div>
        <Row label="Status" width={130}><Pill text={info.status.text} tone={info.status.tone} /></Row>
        <Row label="File size" width={130}><span class="mono">{info.size}</span></Row>
        <Row label="Downloaded" width={130}><span class="mono">{info.downloaded}</span></Row>
        <Row label="Transfer rate" width={130}><span class="mono">{info.rate}</span></Row>
        <Row label="Time left" width={130}><span class="mono">{info.time_left}</span></Row>
        <Row label="Resume capability" width={130}><Pill text={info.resume.text} tone={info.resume.tone} /></Row>
      {:else if tab === 1}
        <Row label="Transfer rate" width={130}><span class="mono">{info.rate}</span></Row>
        <label class="check"><input type="checkbox" bind:checked={limitOn} onchange={applyLimit} /> Use Speed Limiter</label>
        <Row label="Maximum speed" width={130}>
          <input class="input mono" type="number" min="1" max="10000000" style="width:120px" bind:value={limitKbps} onchange={applyLimit} />
          <span class="formLabel">KBytes/sec</span>
        </Row>
        <label class="check"><input type="checkbox" bind:checked={rememberLimit} onchange={applyLimit} /> Remember Speed Limiter settings for this file on download start</label>
      {:else}
        <label class="check"><input type="checkbox" bind:checked={showComplete} onchange={sendOptions} /> Show download complete dialog</label>
        <Row label="Save To" width={70}><input class="input mono" readonly value={info.path} /></Row>
        <div class="caption">Power options</div>
        <label class="check"><input type="checkbox" bind:checked={exitDone} onchange={sendOptions} /> Exit LID when done</label>
        <label class="check"><input type="checkbox" bind:checked={shutdownDone} onchange={sendOptions} /> Turn off computer when done</label>
      {/if}
      <div style="display:flex;align-items:center;gap:12px;margin-top:6px">
        <span class="big" style="font-size:1.67em">{info.percent}</span>
        <div class="bar" style="flex:1"><div style:width="{Math.round(info.bar * 100)}%"></div></div>
      </div>
      {#if details}
        <div class="faint">Start positions and download progress by connections</div>
        <SegmentBar size={info.size_total} segments={info.segments} />
        <div class="list2" style="min-height:136px;max-height:200px">
          <table class="small">
            <thead><tr><th style="width:50px">N.</th><th style="width:110px">DOWNLOADED</th><th style="width:100px">SPEED</th><th>INFO</th></tr></thead>
            <tbody>
              {#each info.connections as c}
                <tr><td>{c.n}</td><td class="mono">{c.downloaded}</td><td class="mono">{c.speed}</td>
                  <td>{#if c.info === 'Finished'}<span class="infopill">Finished</span>{:else}{c.info}{/if}</td></tr>
              {/each}
            </tbody>
          </table>
        </div>
      {/if}
    </div>
    <div class="shell-footer">
      <button class="ghostBtn" onclick={toggleDetails}>{details ? '<< Hide details' : 'Show details >>'}</button>
      <span class="grow"></span>
      <button class="btn" style="min-width:96px" disabled={info.complete} onclick={() => call('progress_button', { id, action: info.button.action })}>
        {#if info.button.icon}<Icon name={info.button.icon} size={14} {theme} />{/if}{info.button.label}
      </button>
      <button class="btn" onclick={cancel}>Cancel</button>
    </div>
  </div>
{/if}
