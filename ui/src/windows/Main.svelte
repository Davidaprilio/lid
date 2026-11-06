<script lang="ts">
  // The main window (1.x ui/main_window.py): menus, toolbar, sidebar, list, details pane, status bar.
  import { listen } from '@tauri-apps/api/event'
  import { open as openFile, save as saveFile } from '@tauri-apps/plugin-dialog'
  import { api, openDialog } from '../lib/api'
  import type { MainView, Menu as MenuT, MenuItem, TreeFilter, UiRequest } from '../lib/types'
  import { applyTheme, type ThemeName } from '../theme/tokens'
  import MenuBar from '../components/MenuBar.svelte'
  import Menu from '../components/Menu.svelte'
  import Toolbar from '../components/Toolbar.svelte'
  import Tree from '../components/Tree.svelte'
  import DownloadList from '../components/DownloadList.svelte'
  import Details from '../components/Details.svelte'
  import Dialog, { type Box } from '../components/Dialog.svelte'
  import Icon from '../components/Icon.svelte'

  const MAIN_QUEUE = 'Main download queue'
  const URL_RE = /^(https?|ftp):\/\/\S+$/i

  let v = $state<MainView | null>(null)
  let menus = $state<MenuT[]>([])
  let box = $state<Box | null>(null)
  let ctx = $state<{ items: MenuItem[]; x: number; y: number; run: (id: string) => void } | null>(null)
  let searchOpen = $state(false)
  let searchText = $state('')
  let searchField: HTMLInputElement | undefined = $state()
  let menubar: MenuBar | undefined = $state()
  let list: DownloadList | undefined = $state()
  let treeOpen = $state<Record<string, boolean>>({})
  let sidebarWidth = $state(236)
  let detailsHeight = $state(210)
  let rightWidth = $state(800)
  let rightEl: HTMLDivElement | undefined = $state()
  let altHeld = $state(false)
  let monoFamily = 'monospace'

  const theme = $derived<ThemeName>(v?.appearance.theme ?? 'ember')
  const rowHeight = $derived(v?.appearance.density === 'compact' ? 26 : 32)

  function apply(next: MainView) {
    if (next.menus) menus = next.menus
    v = next
  }

  $effect(() => applyTheme(theme))
  $effect(() => {
    const f = v?.appearance.list_font
    const root = document.documentElement.style
    root.setProperty('--list-font', f ? `'${f[0]}', var(--font)` : 'var(--font)')
    root.setProperty('--list-size', f ? `${f[1] * 1.333}px` : '13px')
    root.setProperty('--row', `${rowHeight}px`)
  })
  $effect(() => {
    if (!rightEl) return
    const ro = new ResizeObserver(() => (rightWidth = rightEl!.clientWidth))
    ro.observe(rightEl)
    return () => ro.disconnect()
  })

  // ------------------------------------------------------------------ message boxes
  function ask(b: Omit<Box, 'resolve'>): Promise<{ button: string; checked: boolean; value: string }> {
    return new Promise((resolve) => {
      box = { ...b, resolve: (r) => { box = null; resolve(r) } }
    })
  }
  const info = (title: string, text: string) => ask({ title, text, icon: 'info', buttons: [{ id: 'ok', label: 'OK', primary: true }] })
  const question = async (title: string, text: string) =>
    (await ask({ title, text, icon: 'info', buttons: [{ id: 'yes', label: 'Yes', primary: true }, { id: 'no', label: 'No' }] })).button === 'yes'

  // ------------------------------------------------------------------ the window's own actions
  async function removeTargets() {
    const { ids } = await api.targets()
    if (!ids.length) return
    const r = await ask({
      title: 'Confirm deletion of downloads',
      text: `Are you sure you want to delete ${ids.length} file(s) from the download list?`,
      icon: 'delete', badge: 'bad',
      buttons: [{ id: 'yes', label: 'Yes', primary: true }, { id: 'no', label: 'No' }],
      checkbox: { label: 'Also delete the downloaded files from disk', checked: false },
    })
    if (r.button === 'yes') await api.remove(ids, r.checked)
  }

  async function deleteCompleted() {
    const { completed } = await api.targets()
    if (completed > 0 && (await question('LID', `Remove all ${completed} completed file(s) from the list?\nThe files stay on the disk.`))) {
      await api.deleteCompleted()
    }
  }

  async function addMany(items: [string, string, string][]) {
    const n = await api.addMany(items)
    if (n && (await question('LID', `${n} file(s) were added to the "${MAIN_QUEUE}". Start the queue now?`))) {
      await api.startMainQueue()
    }
  }

  function showSearch() {
    searchOpen = true
    searchText = v?.search.query ?? ''
    queueMicrotask(() => { searchField?.focus(); searchField?.select() })
  }

  async function runFind() {
    try {
      await api.search(searchText)
    } catch (e) {
      await info('LID Search settings', String(e))
    }
  }

  async function closeSearch() {
    searchOpen = false
    await api.searchClose()
  }

  async function findNext() {
    if (!v?.search.query) {
      await openDialog('find', 'find?next=1', 'LID Search settings', 460, 340)
      return
    }
    try {
      const id = await api.findNext()
      if (id === null) await info('Find file', 'No such files found')
      else list?.scrollTo(id)
    } catch (e) {
      await info('LID Search settings', String(e))
    }
  }

  function about() {
    return ask({
      title: 'About LID',
      text: `Linux Internet Downloader ${v?.title.split(' ').pop()}\nLID · package lider\n\nA fast download manager for Linux: multi-connection downloads with dynamic segmentation, queues, scheduler, categories, YouTube, clipboard monitoring and search.\n\nMIT License.\n\nLID is an independent project, not affiliated with Tonec Inc. or Internet Download Manager.`,
      icon: 'app', buttons: [{ id: 'ok', label: 'OK', primary: true }],
    })
  }

  async function batchFromClipboard() {
    const text = await api.clipboard()
    const urls = text.split(/\s+/).filter((w) => URL_RE.test(w))
    if (!urls.length) return info('LID', 'Cannot find any web address in the clipboard!')
    await openDialog('links', `links?source=clipboard&urls=${encodeURIComponent(JSON.stringify(urls))}`, 'Download links', 720, 520)
  }

  async function exportList(kind: 'ef2' | 'text') {
    const path = await saveFile({
      title: 'Export download list',
      defaultPath: kind === 'ef2' ? 'downloads.ef2' : 'downloads.txt',
      filters: [kind === 'ef2' ? { name: 'Export files', extensions: ['ef2'] } : { name: 'Text files', extensions: ['txt'] }],
    })
    if (path) await api.exportList(path, kind)
  }

  async function importList(kind: 'ef2' | 'text') {
    const path = await openFile({
      title: 'Import',
      filters: [kind === 'ef2' ? { name: 'Export files', extensions: ['ef2'] } : { name: 'Text files', extensions: ['txt'] }, { name: 'All files', extensions: ['*'] }],
    })
    if (typeof path === 'string') await addMany(await api.importList(path, kind))
  }

  /** Every menu id: the window handles dialogs and questions, the backend everything else. */
  async function dispatch(id: string) {
    switch (id) {
      case 'add': return openDialog('add', 'add', 'Enter new address to download', 560, 300)
      case 'batch': return openDialog('batch', 'batch', 'Add batch download', 620, 520)
      case 'batch_clipboard': return batchFromClipboard()
      case 'grabber': return openDialog('grabber', 'grabber', 'Grabber', 560, 260)
      case 'export_ef2': return exportList('ef2')
      case 'export_text': return exportList('text')
      case 'import_ef2': return importList('ef2')
      case 'import_text': return importList('text')
      case 'remove': return removeTargets()
      case 'delete_completed': return deleteCompleted()
      case 'find': return showSearch()
      case 'find_next': return findNext()
      case 'scheduler': return openDialog('scheduler', `scheduler?queue=${encodeURIComponent(MAIN_QUEUE)}`, 'Scheduler', 860, 620)
      case 'limiter_settings': return openDialog('limiter', 'limiter', 'Speed Limiter', 440, 300)
      case 'options': return openDialog('options', 'options', 'LID options', 720, 712)
      case 'appearance': return openDialog('options', 'options?tab=Appearance', 'LID options', 720, 712)
      case 'columns': return openDialog('columns', 'columns', 'Customize columns', 440, 520)
      case 'font_choose': return openDialog('font', 'font', 'Choose Font', 460, 520)
      case 'about': return about()
      default: return api.menu(id)
    }
  }

  // ------------------------------------------------------------------ context menus
  function listMenu(e: MouseEvent) {
    if (!v) return
    const ids = v.selected
    const a = v.actions
    if (!ids.length) {
      ctx = { items: [{ type: 'action', id: 'add', label: '&Add new download', icon: 'add-url', enabled: true }], x: e.clientX, y: e.clientY, run: dispatch }
      return
    }
    const row = v.rows.find((r) => r.id === ids[0])
    const complete = row?.pill.kind === 'done' || row?.pill.kind === 'verified' || row?.pill.kind === 'mismatch'
    const canCheck = ids.length === 1 && !!row && (complete || row.pill.progress > 0 || row.pill.kind === 'run')
    const queued = v.rows.some((r) => ids.includes(r.id) && r.queue)
    const items: MenuItem[] = [
      { type: 'action', id: 'ctx_open', label: 'Open', enabled: complete },
      { type: 'action', id: 'ctx_open_with', label: 'Open with...', enabled: complete },
      { type: 'action', id: 'ctx_open_folder', label: 'Open folder', enabled: true },
      { type: 'separator' },
      { type: 'action', id: 'ctx_properties', label: 'Move/Rename', enabled: true },
      { type: 'action', id: 'redownload', label: 'R&edownload', enabled: a.redownload },
      { type: 'action', id: 'ctx_check_hash', label: 'Check hash…', icon: 'checksum', enabled: canCheck },
      { type: 'separator' },
      { type: 'action', id: 'resume', label: '&Download Now', icon: 'resume', enabled: a.resume },
      { type: 'action', id: 'stop', label: '&Stop Download', icon: 'stop', enabled: a.stop },
      { type: 'action', id: 'remove', label: '&Remove', icon: 'delete', shortcut: 'Del', enabled: a.remove },
      { type: 'separator' },
      { type: 'submenu', label: 'Add to queue', items: v.queues.map((q) => ({ type: 'action', id: `ctx_queue:${q}`, label: q, enabled: true })) },
      { type: 'action', id: 'ctx_unqueue', label: 'Delete from queue', enabled: queued },
      { type: 'separator' },
      { type: 'action', id: 'ctx_copy', label: 'Copy download address', enabled: true },
      { type: 'action', id: 'ctx_properties', label: 'Properties', enabled: true },
    ]
    const first = ids[0]
    ctx = {
      items, x: e.clientX, y: e.clientY,
      run: async (id) => {
        if (id === 'ctx_open') return api.open(first)
        if (id === 'ctx_open_folder') return api.openFolder(first)
        if (id === 'ctx_open_with') {
          const r = await ask({ title: 'Open with', text: 'Command (the file path is appended):', buttons: [{ id: 'ok', label: 'OK', primary: true }, { id: 'cancel', label: 'Cancel' }], input: { label: '', value: '' } })
          if (r.button === 'ok' && r.value.trim()) return api.openWith(first, r.value).catch((err) => info('Open with', String(err)))
          return
        }
        if (id === 'ctx_properties') return openDialog(`properties-${first}`, `properties?id=${first}`, 'File Properties', 560, 600, false)
        if (id === 'ctx_check_hash') return openDialog(`checkhash-${first}`, `checkhash?id=${first}`, 'Check hash', 560, 480)
        if (id.startsWith('ctx_queue:')) return api.setQueue(ids, id.slice('ctx_queue:'.length))
        if (id === 'ctx_unqueue') return api.setQueue(ids, '')
        if (id === 'ctx_copy') {
          return api.copy(await api.downloadUrl(first).catch(() => ''))
        }
        return dispatch(id)
      },
    }
  }

  function treeMenu(f: TreeFilter | null, e: MouseEvent) {
    const kind = f?.kind ?? ''
    const value = f?.value ?? null
    let items: MenuItem[]
    if (kind === 'queue' || kind === 'queues') {
      items = [
        ...(kind === 'queue' ? [
          { type: 'action', id: 'q_start', label: 'Start', enabled: true },
          { type: 'action', id: 'q_stop', label: 'Stop', enabled: true },
          { type: 'separator' },
        ] as MenuItem[] : []),
        { type: 'action', id: 'q_scheduler', label: 'Scheduler...', enabled: true },
      ]
    } else {
      items = [
        { type: 'action', id: 'cat_add', label: 'Add category', enabled: true },
        { type: 'action', id: 'cat_delete', label: 'Delete', enabled: !!value },
        { type: 'action', id: 'cat_properties', label: 'Properties', enabled: !!value },
      ]
    }
    ctx = {
      items, x: e.clientX, y: e.clientY,
      run: async (id) => {
        if (id === 'q_start') return api.queue(value!, true)
        if (id === 'q_stop') return api.queue(value!, false)
        if (id === 'q_scheduler') return openDialog('scheduler', `scheduler?queue=${encodeURIComponent(value ?? MAIN_QUEUE)}`, 'Scheduler', 860, 620)
        if (id === 'cat_add') return openDialog('category', 'category', 'Category', 520, 420)
        if (id === 'cat_properties') return openDialog('category', `category?name=${encodeURIComponent(value!)}`, 'Category', 520, 420)
        if (id === 'cat_delete') {
          if (await question('Deleting a category from LID categories list', `Do you really want to delete "${value}" category from LID categories list?`)) {
            await api.deleteCategory(value!)
          }
        }
      },
    }
  }

  // ------------------------------------------------------------------ requests from the backend
  async function onRequest(r: UiRequest) {
    switch (r.kind) {
      case 'add_url':
        return openDialog('add', `add?req=${encodeURIComponent(JSON.stringify(r))}`, 'Enter new address to download', 560, 300)
      case 'links':
        return openDialog('links', `links?req=${encodeURIComponent(JSON.stringify(r))}`, 'Download links', 720, 520)
      case 'complete':
        return openDialog(`complete-${r.id}`, `complete?id=${r.id}`, 'Download complete', 560, 360, false)
      case 'mismatch':
        return openDialog(`mismatch-${r.id}`, `mismatch?id=${r.id}`, 'Checksum mismatch', 560, 420, false)
      case 'shutdown': return countdown()
      case 'confirm_close': return confirmClose(r.running, r.background, r.exiting)
      case 'offer_menu': return offerMenu()
    }
  }

  async function countdown() {
    let left = 30
    let cancelled = false
    const timer = setInterval(() => {
      left -= 1
      if (box) box = { ...box, text: `LID has finished downloading. The computer will be turned off in ${left} seconds.` }
      if (left <= 0) {
        clearInterval(timer)
        box?.resolve({ button: 'go', checked: false, value: '' })
      }
    }, 1000)
    const r = await ask({ title: 'Shut down', text: `LID has finished downloading. The computer will be turned off in ${left} seconds.`, icon: 'info', badge: 'bad', buttons: [{ id: 'cancel', label: 'Cancel', primary: true }] })
    clearInterval(timer)
    cancelled = r.button === 'cancel' && left > 0
    if (!cancelled) await api.shutdownNow()
  }

  async function confirmClose(running: string[], background: boolean, exiting: boolean) {
    const names = running.slice(0, 5).join('\n') + (running.length > 5 ? `\n… and ${running.length - 5} more` : '')
    const buttons = [
      ...(background ? [{ id: 'background', label: 'Keep downloading in the background', primary: true }] : []),
      { id: 'stop', label: exiting ? 'Stop and exit' : 'Stop and close', primary: !background },
      { id: 'keep', label: 'Cancel' },
    ]
    const r = await ask({ title: 'Downloads are still running', text: `${running.length} download(s) are still running:\n${names}`, icon: 'info', buttons })
    await api.closeChoice((r.button === 'cancel' ? 'keep' : r.button) as 'keep' | 'background' | 'stop')
  }

  async function offerMenu() {
    const r = await ask({
      title: 'Add LID to the application menu?',
      text: 'This AppImage runs on its own, so the application menu does not list LID yet.\n\nLID keeps the entry pointed at the AppImage you start, also after you move or update it, and it disappears when the AppImage is deleted. You can change this later in Options → General.',
      icon: 'general', badge: 'accent',
      buttons: [{ id: 'add', label: 'Add to menu', primary: true }, { id: 'later', label: 'Not now' }],
    })
    await api.menuOffer(r.button === 'add').catch(() => {})
  }

  // ------------------------------------------------------------------ keyboard, drag and drop, splitters
  function keydown(e: KeyboardEvent) {
    if (e.key === 'Alt') altHeld = true
    if (box || ctx) return
    if (v?.appearance.inline_menubar && menubar?.key(e)) { e.preventDefault(); return }
    const typing = (e.target as HTMLElement)?.tagName === 'INPUT'
    if (e.key === 'Escape' && searchOpen) { e.preventDefault(); closeSearch(); return }
    // With the native menu bar its accelerators fire; in the window the keys are handled here.
    if (v?.appearance.inline_menubar) {
      const k = (e.ctrlKey ? 'Ctrl+' : '') + e.key
      const map: Record<string, string> = { 'Ctrl+n': 'add', 'Ctrl+q': 'exit', 'Ctrl+f': 'find', F3: 'find_next', F9: 'details' }
      if (map[k]) { e.preventDefault(); dispatch(map[k]); return }
      if (e.key === 'Delete' && !typing) { e.preventDefault(); dispatch('remove'); return }
    }
    if (!typing && list?.key(e)) e.preventDefault()
  }

  function dropped(e: DragEvent) {
    e.preventDefault()
    const dt = e.dataTransfer
    if (!dt) return
    const raw = dt.getData('text/uri-list') || dt.getData('text/plain') || ''
    const urls = raw.split(/\s+/).filter((w) => !w.startsWith('#') && URL_RE.test(w))
    if (urls.length === 1) onRequest({ kind: 'add_url', url: urls[0], referer: '', parent_page: '', headers: {}, filename: '', ask_address: false })
    else if (urls.length > 1) addMany(urls.map((u) => [u, '', ''] as [string, string, string]))
  }

  function drag(kind: 'sidebar' | 'details', e: MouseEvent) {
    e.preventDefault()
    const start = kind === 'sidebar' ? e.clientX : e.clientY
    const base = kind === 'sidebar' ? sidebarWidth : detailsHeight
    const move = (ev: MouseEvent) => {
      if (kind === 'sidebar') sidebarWidth = Math.max(170, Math.min(window.innerWidth - 560, base + ev.clientX - start))
      else detailsHeight = Math.max(170, Math.min(window.innerHeight - 220, base - (ev.clientY - start)))
    }
    const up = () => {
      window.removeEventListener('mousemove', move)
      window.removeEventListener('mouseup', up)
      api.paneSizes(kind === 'sidebar' ? sidebarWidth : null, kind === 'details' ? detailsHeight : null)
    }
    window.addEventListener('mousemove', move)
    window.addEventListener('mouseup', up)
  }

  // ------------------------------------------------------------------ start
  $effect(() => {
    const offs: (() => void)[] = []
    listen<MainView>('lid://view', (e) => apply(e.payload)).then((f) => offs.push(f))
    listen<string>('lid://menu', (e) => dispatch(e.payload)).then((f) => offs.push(f))
    listen<UiRequest>('lid://request', (e) => onRequest(e.payload)).then((f) => offs.push(f))
    api.appInfo().then((i) => { monoFamily = i.mono_family; document.documentElement.style.setProperty('--mono', `'${monoFamily}', monospace`) })
    api.ready().then((first) => {
      apply(first)
      sidebarWidth = first.appearance.sidebar_width
      detailsHeight = first.appearance.details_height
      searchText = first.search.query
    })
    const up = (e: KeyboardEvent) => { if (e.key === 'Alt') altHeld = false }
    window.addEventListener('keyup', up)
    return () => { offs.forEach((f) => f()); window.removeEventListener('keyup', up) }
  })
</script>

<svelte:window onkeydown={keydown} ondragover={(e) => e.preventDefault()} ondrop={dropped} oncontextmenu={(e) => e.preventDefault()} />

{#if v}
  <div class="main" class:show-mnemonics={altHeld} style="display:flex;flex-direction:column;height:100vh">
    {#if v.appearance.inline_menubar}
      <MenuBar bind:this={menubar} {menus} {theme} onpick={dispatch} />
    {/if}
    <Toolbar actions={v.actions} labels={v.appearance.toolbar_labels} {theme} onpick={dispatch} onclear={() => api.clearChecks()} />
    <div style="flex:1;display:flex;min-height:0">
      {#if v.appearance.show_categories}
        <div class="sidebar" style:width="{sidebarWidth}px">
          <div class="head">
            <span class="sectionTitle">Categories</span>
            <button class="iconbtn" title="Hide categories" onclick={() => api.menu('hide_categories')}><Icon name="close" size={13} {theme} /></button>
          </div>
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div class="tree" role="tree" tabindex="-1" oncontextmenu={(e) => { e.preventDefault(); treeMenu(null, e) }}>
            <Tree nodes={v.tree} filter={v.filter} {theme} expanded={treeOpen} onselect={(f) => api.filter(f)} onmenu={treeMenu} />
          </div>
        </div>
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div class="splitter-v" onmousedown={(e) => drag('sidebar', e)}></div>
      {/if}
      <div bind:this={rightEl} style="flex:1;display:flex;flex-direction:column;min-width:560px">
        {#if searchOpen}
          <div class="searchbar">
            <span>Find:</span>
            <input class="input" type="search" bind:this={searchField} bind:value={searchText} style="width:260px"
              onkeydown={(e) => { if (e.key === 'Enter') runFind(); if (e.key === 'Escape') closeSearch() }} />
            <button class="btn primary" onclick={runFind}>Find</button>
            <button class="btn" title="LID Search settings" onclick={() => openDialog('find', 'find', 'LID Search settings', 460, 340)}>Settings...</button>
            <span style="flex:1"></span>
            <button class="iconbtn" title="Close SearchBar" onclick={closeSearch}><Icon name="close" size={13} {theme} /></button>
          </div>
        {/if}
        <DownloadList bind:this={list} columns={v.columns} rows={v.rows} checkState={v.check_state} selected={v.selected}
          current={v.current} {rowHeight} {theme} sortKey={v.appearance.sort_key} sortDesc={v.appearance.sort_desc}
          onselect={(ids, cur) => api.select(ids, cur)} oncheck={(id, on) => api.check(id, on)} ontoggleall={() => api.toggleAll()}
          onactivate={(id) => api.activate(id)} onmenu={listMenu} onheader={(k) => api.headerClick(k)}
          onheadermenu={() => dispatch('columns')} onwidth={(k, w) => api.columnWidth(k, w)} />
        {#if v.appearance.show_details && v.details}
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div class="splitter-h" onmousedown={(e) => drag('details', e)}></div>
          <div style:height="{detailsHeight}px" style="display:flex;flex-direction:column;flex:none">
            <Details d={v.details} {theme} width={rightWidth}
              onbutton={(id, a) => api.detailsButton(id, a)} onfolder={(id) => api.openFolder(id)}
              onwindow={(id) => api.openWindow(id)} onclose={() => api.menu('details')}
              onlimit={(id, k) => api.downloadLimit(id, k)} />
          </div>
        {/if}
      </div>
    </div>
    {#if v.appearance.show_statusbar}
      <div class="statusbar">
        <span class="mono">{v.status.speed}</span>
        <span>{v.status.counts}</span>
        <span class="grow"></span>
        <span>{v.status.limiter}</span>
        <span>{v.status.clipboard}</span>
        <button class="sbLink" title="Browser integration…" onclick={async () => { await openDialog('browsers', 'browsers', 'Browser integration', 620, 560); }}>{v.browser}</button>
      </div>
    {/if}
  </div>
{/if}
{#if ctx}
  <!-- svelte-ignore a11y_no_static_element_interactions a11y_click_events_have_key_events -->
  <div style="position:fixed;inset:0;z-index:999" onmousedown={() => (ctx = null)}></div>
  <Menu items={ctx.items} x={ctx.x} y={ctx.y} {theme} onpick={(id) => { const c = ctx!; ctx = null; c.run(id) }} onclose={() => (ctx = null)} />
{/if}
{#if box}
  <Dialog {box} {theme} />
{/if}
