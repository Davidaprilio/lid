<script lang="ts">
  // "Browser integration" (1.x browsers.BrowserIntegrationDialog): register the helper, then load the
  // extension.
  import Shell from '../../components/form/Shell.svelte'
  import Pill from '../../components/form/Pill.svelte'
  import { call, close, followTheme, fit } from '../../lib/dialog'
  import type { ThemeName } from '../../theme/tokens'

  let theme = $state<ThemeName>('ember')
  let data = $state<any>(null)
  let checked = $state<Set<string>>(new Set())
  followTheme((t) => (theme = t))
  async function load(first: boolean) {
    data = await call('browsers_list')
    if (first) {
      checked = new Set(data.rows.filter((r: any) => r.detected).map((r: any) => r.name))
      fit(680)
    }
  }
  load(true)

  const enabled = $derived(data ? data.rows.filter((r: any) => r.detected).map((r: any) => r.name as string) : [])
  const allOn = $derived(enabled.length > 0 && enabled.every((n: string) => checked.has(n)))
  const toggle = (n: string) => { const c = new Set(checked); if (c.has(n)) c.delete(n); else c.add(n); checked = c }

  async function apply(install: boolean) {
    await call('browsers_apply', { names: [...checked], install })
    await load(false)
  }

  async function done() {
    await call('recheck_browser')
    close()
  }

  const firefoxLink = $derived(
    !data ? null : data.firefox_addons ? ['Firefox Add-ons', data.firefox_addons] : data.firefox_signed ? ['Firefox (.xpi)', data.firefox_xpi] : ['Firefox (.zip)', data.firefox_zip],
  )
</script>

<svelte:window onkeydown={(e) => { if (e.key === 'Escape' || e.key === 'Enter') done() }} />
{#if data}
  <Shell {theme} icon="connection" title="Browser integration" subtitle="LID talks to its extension through native messaging — no network port is opened.">
    <div class="list2" style="min-height:{5 * 27 + 30}px">
      <div class="h">
        <span style="width:24px"><input type="checkbox" checked={allOn} onchange={() => (checked = allOn ? new Set() : new Set(enabled))} /></span>
        <span style="width:260px">BROWSER</span><span>LID HELPER</span>
      </div>
      {#each data.rows as r}
        <div class="r" style:opacity={r.detected ? 1 : 0.5}>
          <span style="width:24px"><input type="checkbox" disabled={!r.detected} checked={checked.has(r.name)} onchange={() => toggle(r.name)} /></span>
          <span style="width:260px">{r.name}</span>
          <span>{#if r.state === 'installed'}<Pill text="Installed" tone="ok" />{:else if r.state === 'not installed'}<Pill text="Not installed" />{:else}<span class="muted">{r.state}</span>{/if}</span>
        </div>
      {/each}
    </div>
    <div style="display:flex;gap:8px">
      <button class="btn primary" onclick={() => apply(true)}>Install for checked browsers</button>
      <button class="btn" onclick={() => apply(false)}>Remove</button>
    </div>
    <div class="panel">
      <div class="caption">Then add the LID extension to the browser</div>
      <div><b>Chrome, Chromium, Brave, Edge, Vivaldi, Opera:</b> open <span class="mono">chrome://extensions</span>, turn on <i>Developer mode</i>, click <i>Load unpacked</i> and choose the folder below.</div>
      <div style="display:flex;gap:8px">
        <input class="input mono" style="flex:1" readonly placeholder="Install for a browser first" value={data.chromium_folder} />
        <button class="btn" disabled={!data.chromium_folder} onclick={() => call('copy_text', { text: data.chromium_folder })}>Copy</button>
        <button class="btn" disabled={!data.chromium_folder} onclick={() => call('open_url', { url: data.chromium_folder })}>Open folder</button>
      </div>
      <div><b>Firefox:</b> open <span class="mono">about:debugging#/runtime/this-firefox</span>, click <i>Load Temporary Add-on</i> and choose <span class="mono">manifest.json</span> in the folder below. Then allow it to access all websites in the add-on's permissions.</div>
      <div style="display:flex;gap:8px">
        <input class="input mono" style="flex:1" readonly placeholder="Install for a browser first" value={data.firefox_folder} />
        <button class="btn" disabled={!data.firefox_folder} onclick={() => call('copy_text', { text: data.firefox_folder })}>Copy</button>
        <button class="btn" disabled={!data.firefox_folder} onclick={() => call('open_url', { url: data.firefox_folder })}>Open folder</button>
      </div>
    </div>
    <div style="display:flex;gap:8px;align-items:center;flex-wrap:wrap">
      <span>Get the extension:</span>
      {#if data.chrome_store}<button class="ghostBtn link" title={data.chrome_store} onclick={() => call('open_url', { url: data.chrome_store })}>Chrome Web Store</button>{/if}
      {#if firefoxLink}<button class="ghostBtn link" title={firefoxLink[1]} onclick={() => call('open_url', { url: firefoxLink![1] })}>{firefoxLink[0]}</button>{/if}
      <button class="ghostBtn link" title={data.releases} onclick={() => call('open_url', { url: data.releases })}>All downloads</button>
    </div>
    {#snippet footer()}
      <span class="grow"></span>
      <button class="btn primary" onclick={done}>Close</button>
    {/snippet}
  </Shell>
{/if}
