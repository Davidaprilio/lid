<script lang="ts">
  // "Download All Links" (1.x add.LinksDialog), for the grabber, batch from clipboard and the browser.
  // The grabber first asks for the page address (`#/links?grabber=1`).
  import { open as openDir } from '@tauri-apps/plugin-dialog'
  import Shell from '../../components/form/Shell.svelte'
  import Row from '../../components/form/Row.svelte'
  import Icon from '../../components/Icon.svelte'
  import Dialog, { type Box } from '../../components/Dialog.svelte'
  import { call, close, followTheme, fit, params, setTitle, URL_RE } from '../../lib/dialog'
  import type { ThemeName } from '../../theme/tokens'

  type Link = [string, string, string]
  interface LinkRow { url: string; text: string; name: string; kind: string; address: string; icon: string; category: string; checked: boolean }

  const p = params()
  const req = JSON.parse(p.get('req') ?? 'null') as null | { items: Link[]; referer: string; parent_page: string; headers: Record<string, string> }
  let theme = $state<ThemeName>('ember')
  let page = $state<'grab' | 'busy' | 'links' | ''>('')
  let grabUrl = $state('https://')
  let links = $state<Link[]>([])
  let referer = ''
  let parent = ''
  let headers: Record<string, string> = {}
  let rows = $state<LinkRow[]>([])
  let checked = $state<Set<number>>(new Set())
  let hideHtml = $state(true)
  let hideImages = $state(false)
  let hideRepeated = $state(true)
  let saveMode = $state<'category' | 'one_category' | 'one_dir'>('category')
  let oneCategory = $state('')
  let dir = $state('')
  let useText = $state(true)
  let categories = $state<{ name: string; save_dir: string }[]>([])
  let box = $state<Box | null>(null)
  let field: HTMLInputElement | undefined = $state()
  let selected = $state<Set<number>>(new Set())
  let anchor = -1
  let fileWidth = $state(240)

  followTheme((t) => (theme = t))
  ;(async () => {
    const s = await call<any>('settings_get')
    categories = s.categories
    oneCategory = categories[0].name
    dir = categories[0].save_dir
    if (p.get('grabber') === '1') {
      const clip = (await call<string>('clipboard_text')).trim()
      grabUrl = URL_RE.test(clip) ? clip : 'https://'
      page = 'grab'
      setTitle('Grabber')
      await fit(560)
      field?.focus()
      field?.select()
      return
    }
    if (req) {
      links = req.items
      referer = req.referer
      parent = req.parent_page
      headers = req.headers
    } else {
      links = (JSON.parse(p.get('urls') ?? '[]') as string[]).map((u) => [u, '', 'a'])
    }
    await showLinks()
  })()

  function info(text: string) {
    return new Promise<void>((resolve) => {
      box = { title: 'Grabber', text, icon: 'info', badge: 'bad', buttons: [{ id: 'ok', label: 'OK', primary: true }], resolve: () => { box = null; resolve() } }
    })
  }

  async function grab() {
    const u = grabUrl.trim()
    if (!URL_RE.test(u)) return close()
    page = 'busy'
    document.body.style.cursor = 'progress'
    try {
      const r = await call<{ base: string; links: Link[] }>('grab_page', { url: u })
      document.body.style.cursor = ''
      if (!r.links.length) { await info('Cannot find the links!'); return close() }
      links = r.links
      referer = parent = r.base
      await showLinks()
    } catch (e) {
      document.body.style.cursor = ''
      await info(`Cannot open the web page:\n${e}`)
      close()
    }
  }

  async function showLinks() {
    page = 'links'
    setTitle('Download All Links')
    await refill()
    await fit(840)
  }

  async function refill() {
    rows = await call('links_rows', { links, filters: { hide_html: hideHtml, hide_images: hideImages, hide_repeated: hideRepeated } })
    checked = new Set(rows.flatMap((r, i) => (r.checked ? [i] : [])))
    selected = new Set()
    anchor = -1
    // resizeColumnToContents(FILE): icon, gap and the longest name.
    const g = document.createElement('canvas').getContext('2d')!
    g.font = getComputedStyle(document.body).font
    fileWidth = Math.min(480, Math.ceil(Math.max(60, ...rows.map((r) => g.measureText(r.name).width)) + 16 + 6 + 14))
  }

  /** ExtendedSelection: click, Ctrl+click, Shift+click; Space toggles the checks of the selected rows. */
  function select(i: number, e: MouseEvent) {
    if (e.shiftKey && anchor >= 0) {
      const [a, b] = [Math.min(anchor, i), Math.max(anchor, i)]
      selected = new Set([...(e.ctrlKey ? selected : []), ...Array.from({ length: b - a + 1 }, (_, k) => a + k)])
      return
    }
    if (e.ctrlKey) { const c = new Set(selected); if (c.has(i)) c.delete(i); else c.add(i); selected = c }
    else selected = new Set([i])
    anchor = i
  }
  function listKey(e: KeyboardEvent) {
    if (e.key === ' ' && selected.size) {
      e.preventDefault()
      const on = ![...selected].every((i) => checked.has(i))
      const c = new Set(checked)
      for (const i of selected) if (on) c.add(i); else c.delete(i)
      checked = c
    } else if ((e.key === 'ArrowDown' || e.key === 'ArrowUp') && rows.length) {
      e.preventDefault()
      const i = Math.max(0, Math.min(rows.length - 1, anchor + (e.key === 'ArrowDown' ? 1 : -1)))
      selected = new Set([i])
      anchor = i
    } else if (e.key === 'a' && e.ctrlKey) {
      e.preventDefault()
      selected = new Set(rows.map((_, i) => i))
    }
  }

  const allState = $derived(checked.size === 0 ? 'none' : checked.size === rows.length ? 'all' : 'some')
  const toggle = (i: number) => { const c = new Set(checked); if (c.has(i)) c.delete(i); else c.add(i); checked = c }
  const setAll = (on: boolean) => (checked = on ? new Set(rows.map((_, i) => i)) : new Set())

  async function browse() {
    const d = await openDir({ directory: true, title: 'Save To', defaultPath: dir })
    if (typeof d === 'string') { dir = d; saveMode = 'one_dir' }
  }

  async function ok() {
    const choices = rows.filter((_, i) => checked.has(i)).map((r) => {
      const cat = saveMode === 'one_category' ? categories.find((c) => c.name === oneCategory)! : categories.find((c) => c.name === r.category) ?? categories[0]
      return { url: r.url, filename: r.name, description: useText ? r.text : '', category: cat.name, save_dir: saveMode === 'one_dir' ? dir : cat.save_dir }
    })
    await call('add_choices', { choices, referer, parentPage: parent, headers })
    close()
  }
</script>

<svelte:window onkeydown={(e) => { if (box) return; if (e.key === 'Escape') close(); if (e.key === 'Enter') { if (page === 'grab') grab(); else if (page === 'links') ok() } }} />

{#if page === 'grab'}
  <Shell {theme} icon="grabber" title="Grabber" subtitle="Enter the address of the web page to explore:">
    <input class="input mono" bind:this={field} bind:value={grabUrl} />
    {#snippet footer()}
      <span class="grow"></span>
      <button class="btn" onclick={close}>Cancel</button>
      <button class="btn primary" onclick={grab}>OK</button>
    {/snippet}
  </Shell>
{:else if page === 'busy'}
  <div class="shell" style="align-items:center;justify-content:center"><div class="muted">Reading the web page…</div></div>
{:else if page === 'links'}
  <Shell {theme} icon="grabber" title="Download All Links" subtitle="{links.length} links found · check the ones to add">
    <!-- svelte-ignore a11y_no_noninteractive_tabindex a11y_no_static_element_interactions -->
    <div class="list2" style="height:300px" role="listbox" aria-multiselectable="true" tabindex="0" onkeydown={listKey}>
      <div class="h">
        <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
        <span style="width:24px" role="button" tabindex="-1" onclick={() => setAll(allState !== 'all')}><input type="checkbox" checked={allState === 'all'} indeterminate={allState === 'some'} /></span>
        <span style="width:{fileWidth}px;flex:none">FILE</span><span style="width:70px">TYPE</span><span>ADDRESS</span>
      </div>
      {#each rows as r, i (i)}
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <div class="r" class:sel={selected.has(i)} role="option" aria-selected={selected.has(i)} tabindex="-1" onclick={(e) => select(i, e)}>
          <span style="width:24px"><input type="checkbox" checked={checked.has(i)} onchange={() => toggle(i)} /></span>
          <span style="width:{fileWidth}px;flex:none;display:flex;gap:6px;overflow:hidden" title={r.text}><Icon name={r.icon} size={16} {theme} /><span style="overflow:hidden;text-overflow:ellipsis;white-space:nowrap">{r.name}</span></span>
          <span style="width:70px">{#if r.kind}<span class="tpill muted">{r.kind}</span>{/if}</span>
          <span class="mono muted" style="flex:1;overflow:hidden;text-overflow:ellipsis;white-space:nowrap" title={r.url}>{r.address}</span>
        </div>
      {/each}
    </div>
    <div style="display:flex;gap:8px;align-items:center;flex-wrap:wrap">
      <button class="btn" onclick={() => setAll(true)}>Check All</button>
      <button class="btn" onclick={() => setAll(false)}>Uncheck All</button>
      <span style="flex:1"></span>
      <label class="check"><input type="checkbox" bind:checked={hideHtml} onchange={refill} /> Hide HTML files</label>
      <label class="check" title="Hide images located on this web page"><input type="checkbox" bind:checked={hideImages} onchange={refill} /> Hide images</label>
      <label class="check"><input type="checkbox" bind:checked={hideRepeated} onchange={refill} /> Hide repeated files</label>
    </div>
    <div class="panel">
      <div class="caption">Save to</div>
      <label class="radio"><input type="radio" bind:group={saveMode} value="category" /> Every file to the folder of its category</label>
      <div style="display:flex;gap:8px;align-items:center">
        <label class="radio"><input type="radio" bind:group={saveMode} value="one_category" /> All files to one category</label>
        <select class="input" style="min-width:180px" bind:value={oneCategory}>{#each categories as c}<option>{c.name}</option>{/each}</select>
      </div>
      <div style="display:flex;gap:8px;align-items:center">
        <label class="radio"><input type="radio" bind:group={saveMode} value="one_dir" /> All files to one directory</label>
        <input class="input mono" style="flex:1" bind:value={dir} />
        <button class="btn" onclick={browse}>Browse...</button>
      </div>
    </div>
    <label class="check"><input type="checkbox" bind:checked={useText} /> Use link texts as download descriptions</label>
    {#snippet footer()}
      <span class="tpill accent">{checked.size} checked</span>
      <span class="grow"></span>
      <button class="btn" onclick={close}>Cancel</button>
      <button class="btn primary" onclick={ok}>OK</button>
    {/snippet}
  </Shell>
{/if}
{#if box}<Dialog {box} {theme} />{/if}
