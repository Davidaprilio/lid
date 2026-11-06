<script lang="ts">
  // The details pane under the list (1.x ui/details.py): status, connections, per-download speed limit.
  import type { Details } from '../lib/types'
  import Icon from './Icon.svelte'
  import SegmentBar from './SegmentBar.svelte'
  import type { ThemeName } from '../theme/tokens'

  let { d, theme, width, onbutton, onfolder, onwindow, onclose, onlimit }: {
    d: Details | null; theme: ThemeName; width: number
    onbutton: (id: number, action: string) => void; onfolder: (id: number) => void; onwindow: (id: number) => void
    onclose: () => void; onlimit: (id: number, kbps: number) => void
  } = $props()
  let tab = $state(0)
  let limitOn = $state(false)
  let limitValue = $state(200)
  let lastId: number | null = null

  $effect(() => {
    if (d && d.id !== lastId) {
      lastId = d.id
      limitOn = d.limit_on
      limitValue = d.limit_kbps
    }
  })

  function applyLimit() {
    if (!d) return
    const v = Math.max(1, Math.min(10_000_000, Math.round(limitValue || 1)))
    limitValue = v
    onlimit(d.id, limitOn ? v : 0)
  }
</script>

<div class="details">
  <div class="tabs">
    {#each ['Download status', 'Connections', 'Speed Limiter'] as t, i}
      <button class="tab" class:on={tab === i} onclick={() => (tab = i)}>{t}</button>
    {/each}
    <span class="hint">View → Details pane (F9)</span>
    <button class="iconbtn" title="Open in a window" disabled={!d} onclick={() => d && onwindow(d.id)}><Icon name="open-window" size={13} {theme} /></button>
    <button class="iconbtn" title="Hide the details pane" onclick={onclose}><Icon name="close" size={13} {theme} /></button>
  </div>
  {#if !d}
    <div class="page"><div class="emptytext">Select a download to see its progress, connections and speed limit here.</div></div>
  {:else if tab === 0}
    <div class="page">
      <div style="width:300px;flex:none;display:flex;flex-direction:column;gap:8px;min-width:0">
        <div style="display:flex;gap:12px;align-items:center;min-width:0">
          <div class="tile"><Icon name={d.icon} size={30} {theme} /></div>
          <div style="min-width:0">
            <div style="font-weight:700;font-size:1.125em;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">{d.filename}</div>
            <div class="mono link" style="font-size:0.92em;overflow:hidden;text-overflow:ellipsis;white-space:nowrap" title={d.url_full}>{d.url}</div>
          </div>
        </div>
        <div class="muted" style="font-size:0.96em;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">{d.state}</div>
        <div style="display:flex;gap:6px">
          <button class="compact" onclick={() => onbutton(d!.id, d!.action)}>
            {#if d.action_icon}<Icon name={d.action_icon} size={13} {theme} />{/if}{d.action_label}
          </button>
          <button class="compact" onclick={() => onfolder(d!.id)}>Open folder</button>
        </div>
      </div>
      <div style="flex:1;display:flex;flex-direction:column;gap:6px;min-width:0">
        <div class="big">{d.percent}</div>
        <div class="muted">{d.of}</div>
        <div class="bar"><div style:width="{Math.round(d.bar * 100)}%"></div></div>
        <div class="faint" style="margin-top:6px">{d.caption}</div>
        <SegmentBar size={d.size} segments={d.segments} />
      </div>
      {#if width >= 960}
        <div class="stats">
          <div><div class="cap">Transfer rate</div><div class="val">{d.rate}</div></div>
          <div><div class="cap">Time left</div><div class="val">{d.time_left}</div></div>
          <div><div class="cap">Downloaded</div><div class="val">{d.done}</div></div>
          <div><div class="cap">Connections</div><div class="val">{d.connections}</div></div>
          <div><div class="cap">Resume</div><div class="val okText">{d.resume}</div></div>
          <div><div class="cap">Checksum</div><div class="val {d.checksum_tone}">{d.checksum}</div></div>
        </div>
      {/if}
    </div>
  {:else if tab === 1}
    <div class="page" style="padding:0">
      <table class="small">
        <thead><tr><th style="width:50px">N.</th><th style="width:120px">DOWNLOADED</th><th style="width:110px">SPEED</th><th>INFO</th></tr></thead>
        <tbody>
          {#each d.rows as c}
            <tr><td>{c.n}</td><td class="mono">{c.downloaded}</td><td class="mono">{c.speed}</td>
              <td>{#if c.info === 'Finished'}<span class="infopill">Finished</span>{:else}{c.info}{/if}</td></tr>
          {/each}
        </tbody>
      </table>
    </div>
  {:else}
    <div class="page" style="flex-direction:column;gap:12px">
      <label class="check"><input type="checkbox" bind:checked={limitOn} onchange={applyLimit} /> Use Speed Limiter for this download</label>
      <div style="display:flex;align-items:center;gap:8px">
        <span>Maximum download speed:</span>
        <input class="input" type="number" min="1" max="10000000" style="width:110px" bind:value={limitValue} onchange={applyLimit} disabled={!limitOn} />
        <span>KBytes/sec</span>
      </div>
    </div>
  {/if}
</div>
