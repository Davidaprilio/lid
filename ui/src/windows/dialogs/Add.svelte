<script lang="ts">
  // Adding one download (1.x main_window.add_url with AddUrlDialog, DuplicateDialog, FileInfoDialog):
  // address → duplicate rule → probe (or yt-dlp for YouTube) → Download File Info → add.
  import { save as saveFile } from '@tauri-apps/plugin-dialog'
  import Shell from '../../components/form/Shell.svelte'
  import Row from '../../components/form/Row.svelte'
  import Pill from '../../components/form/Pill.svelte'
  import ChecksumEdit from '../../components/form/ChecksumEdit.svelte'
  import Combo from '../../components/form/Combo.svelte'
  import Icon from '../../components/Icon.svelte'
  import Menu from '../../components/Menu.svelte'
  import Dialog, { type Box } from '../../components/Dialog.svelte'
  import CategoryForm from './CategoryForm.svelte'
  import { call, close, followTheme, fit, params, setTitle, URL_RE } from '../../lib/dialog'
  import type { ThemeName } from '../../theme/tokens'

  interface Category { name: string; extensions: string[]; save_dir: string; sites: string[]; remember_last: boolean }
  interface Variant { kind: string; quality: string; label: string; ext: string; video_format: string; audio_format: string; size: number; height: number; convert: string }
  interface MediaInfo { url: string; id: string; title: string; uploader: string; duration: number; variants: Variant[] }

  const req = JSON.parse(params().get('req') ?? 'null') as null | {
    url: string; referer: string; parent_page: string; headers: Record<string, string>; filename: string; ask_address: boolean
  }
  let theme = $state<ThemeName>('ember')
  let page = $state<'address' | 'duplicate' | 'busy' | 'tool' | 'info' | ''>('')
  let box = $state<Box | null>(null)
  let settings = $state<any>(null)

  // address page
  let address = $state('')
  let history = $state<string[]>([])
  let useAuth = $state(false)
  let login = $state('')
  let password = $state('')
  let addressField: HTMLInputElement | undefined = $state()
  const youtube = $derived(/^https?:\/\/(?:(?:www|m|music)\.)?(?:youtube\.com\/(?:watch\?|shorts\/|live\/|embed\/)|youtu\.be\/)\S+/i.test(address.trim()))

  // duplicate page
  let dupChoice = $state('number')
  let dupRemember = $state(false)
  let dup: { id: number; complete: boolean } | null = null

  // file info page
  let url = $state('')
  let size = $state(-1)
  let resumable = $state<boolean | null>(null)
  let category = $state('General')
  let path = $state('')
  let paths = $state<string[]>([])
  let remember = $state(false)
  let description = $state('')
  let checksumText = $state('')
  let checksumAlgo = $state('auto')
  let checksumValid = $state(true)
  let checksumValue = $state('')
  let queues = $state<string[]>([])
  let later = $state<{ x: number; y: number } | null>(null)
  let newCategory = $state(false)
  let media = $state<MediaInfo | null>(null)
  let kind = $state('video')
  let variant = $state<Variant | null>(null)
  let replace: number | null = null
  let flowUrl = $state('')
  let auth: [string, string] = ['', '']
  let fileName = ''
  let checksumEdit: ReturnType<typeof ChecksumEdit> | undefined = $state()
  let startButton: HTMLButtonElement | undefined = $state()

  const categories = $derived<Category[]>(settings?.categories ?? [])
  const catOf = (name: string) => categories.find((c) => c.name === name) ?? categories[0]
  const lower = (n: string) => n.toLowerCase()
  const catIcon = (name: string) => (['compressed', 'documents', 'music', 'programs', 'video'].includes(lower(name)) ? `category-${lower(name)}` : 'folder')
  const basename = (p: string) => p.split('/').pop() ?? ''
  const dirname = (p: string) => p.split('/').slice(0, -1).join('/')

  function ask(b: Omit<Box, 'resolve'>): Promise<string> {
    return new Promise((resolve) => { box = { ...b, resolve: (r) => { box = null; resolve(r.button) } } })
  }
  const warn = (text: string) => ask({ title: 'LID', text, icon: 'info', badge: 'bad', buttons: [{ id: 'ok', label: 'OK', primary: true }] })

  followTheme((t) => (theme = t))
  ;(async () => {
    settings = await call('settings_get')
    queues = (await call<any>('view')).queues
    if (!req || (req.ask_address && !req.url)) {
      history = await call('url_history')
      address = req?.url || (await clipboardUrl())
      page = 'address'
      setTitle('Enter new address to download')
      await fit(560)
      addressField?.focus()
      addressField?.select()
    } else {
      await go(req.url)
    }
  })()

  async function clipboardUrl() {
    const t = (await call<string>('clipboard_text')).trim()
    return URL_RE.test(t) ? t : ''
  }

  async function okAddress() {
    const u = address.trim()
    if (!URL_RE.test(u)) { addressField?.focus(); addressField?.select(); return }
    await call('remember_url', { url: u })
    auth = useAuth ? [login, password] : ['', '']
    await go(u)
  }

  async function go(u: string) {
    flowUrl = u
    const check = await call<any>('add_check', { url: u })
    if (check.duplicate) {
      dup = check.duplicate
      if (check.action === 'ask') {
        page = 'duplicate'
        setTitle('Duplicate download link')
        await fit(600)
        return
      }
      return afterDuplicate(check.action, check)
    }
    return afterDuplicate('', check)
  }

  async function okDuplicate() {
    if (dupRemember) await call('settings_patch', { patch: { duplicate_action: dupChoice } })
    afterDuplicate(dupChoice, await call<any>('add_check', { url: flowUrl }))
  }

  async function afterDuplicate(action: string, check: any) {
    if (action === 'complete' && dup) {
      await call('add_duplicate_existing', { id: dup.id })
      return close()
    }
    replace = action === 'overwrite' && dup ? dup.id : null
    page = 'busy'
    document.body.style.cursor = 'progress'
    if (check.youtube) return youtubeFlow(check)
    let name: string
    try {
      const p = await call<any>('probe_url', {
        url: flowUrl, referer: req?.referer ?? '', username: auth[0], password: auth[1], headers: req?.headers ?? {},
      })
      name = await call('name_choice', { url: flowUrl, probed: p.filename, browser: req?.filename ?? '' })
      size = p.size
      resumable = p.resumable
    } catch (e) {
      document.body.style.cursor = ''
      const r = await ask({ title: 'LID', text: `Cannot get information about the file:\n${e}\n\nAdd it anyway?`, icon: 'info',
        buttons: [{ id: 'yes', label: 'Yes', primary: true }, { id: 'no', label: 'No' }] })
      if (r !== 'yes') return close()
      name = await call('name_choice', { url: flowUrl, probed: null, browser: req?.filename ?? '' })
      size = -1
      resumable = null
    }
    document.body.style.cursor = ''
    const prep = await call<any>('add_prepare', { url: flowUrl, filename: name, replacing: replace !== null })
    url = flowUrl
    description = ''
    if (!check.show_start_dialog) {
      await submit(check.only_add_to_queue ? 'later' : 'start', '', `${prep.save_dir}/${prep.filename}`, prep.category)
      return
    }
    fileName = prep.filename
    await showInfo(prep.category, `${prep.save_dir}/${prep.filename}`)
  }

  // yt-dlp on demand: offered once YouTube needs it, downloaded from yt-dlp's own releases.
  let tool = $state<{ done: number; total: number } | null>(null)
  async function getYtDlp(): Promise<boolean> {
    document.body.style.cursor = ''
    const offer = await call<any>('ytdlp_offer')
    if (!offer.asset) {
      await warn('Downloading from YouTube needs yt-dlp, which has no program for this system.\n\nInstall it with: pip install yt-dlp')
      return false
    }
    const r = await ask({
      title: 'Download yt-dlp?', icon: 'category-video',
      text: `LID reads YouTube pages with yt-dlp, which is not on this computer yet.\n\nLID can download it now (${offer.asset}) from its official releases:\n${offer.project}\n\nThe file is checked against the release's checksums and kept in LID's own folder.`,
      buttons: [{ id: 'get', label: 'Download', primary: true }, { id: 'no', label: 'Cancel' }],
    })
    if (r !== 'get') return false
    page = 'tool'
    setTitle('Downloading yt-dlp')
    await fit(520)
    tool = { done: 0, total: -1 }
    const timer = setInterval(async () => {
      const [done, total] = await call<[number, number]>('ytdlp_progress')
      tool = { done, total }
    }, 200)
    try {
      await call('ytdlp_install')
      return true
    } catch (e) {
      if (String(e) !== 'cancelled') await warn(`Cannot download yt-dlp:\n${e}`)
      return false
    } finally {
      clearInterval(timer)
      tool = null
      page = 'busy'
      document.body.style.cursor = 'progress'
    }
  }

  async function youtubeFlow(check: any) {
    if (!check.yt_dlp && !(await getYtDlp())) return close()
    let info: MediaInfo
    try {
      info = await call('probe_media', { url: flowUrl })
    } catch (e) {
      document.body.style.cursor = ''
      await warn(`Cannot read this YouTube video:\n${String(e).replaceAll('ERROR: ', '')}`)
      return close()
    }
    document.body.style.cursor = ''
    if (!info.variants.length) {
      await warn('This YouTube video has no downloadable formats.')
      return close()
    }
    const [v, name] = (await call<[Variant, string]>('media_default', { info }))!
    media = info
    variant = v
    kind = v.kind
    size = v.size
    resumable = null
    url = info.url
    fileName = name
    const prep = await call<any>('add_prepare', { url: info.url, filename: name, replacing: true })
    await showInfo(prep.category, `${prep.save_dir}/${name}`)
    const target = kind === 'video' ? 'Video' : 'Music'
    if (categories.some((c) => c.name === target)) setCategory(target)
  }

  async function showInfo(cat: string, p: string) {
    category = cat
    path = p
    rebuildPaths()
    if (settings.checksum_from_clipboard && !media) {
      const r = await call<any>('checksum_parse', { text: await call('clipboard_text'), algorithm: 'auto' })
      if (r.value) checksumText = r.value.split(':')[1]
    }
    page = 'info'
    setTitle('Download File Info')
    await fit(720)
    startButton?.focus()
  }

  function rebuildPaths() {
    const base = basename(path) || fileName
    const own = `${catOf(category).save_dir}/${base}`
    const others = categories.filter((c) => c.name !== category && c.save_dir !== catOf(category).save_dir).map((c) => `${c.save_dir}/${base}`)
    paths = [own, ...others.filter((p, i) => others.indexOf(p) === i)]
    path = own
  }

  function setCategory(name: string) {
    category = name
    rebuildPaths()
  }

  async function setKind(k: string) {
    kind = k
    const d = await call<Variant | null>('media_variant', { info: media, kind: k })
    variant = media!.variants.find((v) => JSON.stringify(v) === JSON.stringify(d)) ?? media!.variants.find((v) => v.kind === k) ?? null
    const cat = k === 'video' ? 'Video' : 'Music'
    if (categories.some((c) => c.name === cat)) category = cat
    quality()
  }

  function quality() {
    if (!variant) return
    size = variant.size
    path = path.replace(/\.[^./]*$/, '') + '.' + variant.ext
    rebuildPaths()
  }

  async function browse() {
    const p = await saveFile({ title: 'Save As', defaultPath: path })
    if (p) path = p
  }

  async function addCategory(c: Category) {
    newCategory = false
    if (await call('category_save', { old: null, category: c })) {
      settings = await call('settings_get')
      setCategory(c.name)
    }
  }

  async function submit(action: string, queue: string, p = path, cat = category) {
    if (!p.trim()) return
    if (!checksumValid) { checksumEdit?.focus(); return }
    await call('add_download', {
      req: {
        url, path: p.trim(), category: cat, description, referer: media ? '' : req?.referer ?? '',
        parent_page: media ? url : req?.parent_page ?? '', username: auth[0], password: auth[1], size, resumable,
        headers: media ? {} : req?.headers ?? {}, checksum: media ? '' : checksumValue, action, queue, replace,
        remember_path: remember, media: media && variant ? [media, variant] : null,
      },
    })
    close()
  }

  function length(s: number) {
    const h = Math.floor(s / 3600), m = Math.floor((s % 3600) / 60), x = s % 60
    const pad = (n: number) => String(n).padStart(2, '0')
    return h ? `${h}:${pad(m)}:${pad(x)}` : `${m}:${pad(x)}`
  }
  const fmtSize = (n: number) => {
    if (n < 0) return '(unknown)'
    if (n < 1024) return `${n} Bytes`
    let v = n
    for (const u of ['KB', 'MB', 'GB', 'TB']) { v /= 1024; if (v < 1024 || u === 'TB') return `${v.toFixed(2)} ${u}` }
    return ''
  }
</script>

<svelte:window onkeydown={(e) => {
  if (box || later || newCategory) return
  if (e.key === 'Escape') close()
  if (e.key === 'Enter' && page === 'address') okAddress()
  if (e.key === 'Enter' && page === 'duplicate') okDuplicate()
  if (e.key === 'Enter' && page === 'info' && checksumValid && (e.target as HTMLElement).tagName !== 'SELECT') submit('start', '')
}} />

{#if page === 'address'}
  <Shell {theme} icon="add-url" title="Enter new address to download" subtitle="Paste a link. LID reads the file name and size next.">
    <Row label="Address" width={70}>
      <Combo editable link {theme} items={history} bind:value={address} bind:input={addressField} />
    </Row>
    {#if youtube}
      <div class="accentBox"><Icon name="category-video" size={18} {theme} /><span><b>YouTube video detected</b> — choose the resolution or audio only in the next window.</span></div>
    {:else}
      <div class="panel">
        <label class="check"><input type="checkbox" bind:checked={useAuth} /> <span>Use <u>a</u>uthorization</span></label>
        <div style="display:flex;gap:10px">
          <Row label="Login" width={50}><input class="input" bind:value={login} disabled={!useAuth} /></Row>
          <Row label="Password" width={70}><input class="input" type="password" bind:value={password} disabled={!useAuth} /></Row>
        </div>
      </div>
    {/if}
    {#snippet footer()}
      <span class="grow"></span>
      <button class="btn" onclick={close}><u>C</u>ancel</button>
      <button class="btn primary" onclick={okAddress}>O<u>K</u></button>
    {/snippet}
  </Shell>
{:else if page === 'duplicate'}
  <Shell {theme} icon="queue" title="Duplicate download link" subtitle="This file is already in your download list.">
    <div class="combo-like"><Icon name="link" size={15} {theme} /><input class="mono" readonly tabindex="-1" value={flowUrl} /></div>
    <div class="panel">
      <label class="radio"><input type="radio" bind:group={dupChoice} value="number" /> Add the duplicate with a numbered file name</label>
      <label class="radio"><input type="radio" bind:group={dupChoice} value="overwrite" /> Add the duplicate and overwrite the existing file</label>
      <label class="radio"><input type="radio" bind:group={dupChoice} value="complete" /> If existing file is complete, show download complete dialog; otherwise resume it</label>
    </div>
    <label class="check"><input type="checkbox" bind:checked={dupRemember} /> Remember my selection and don't ask again (change it in Options → Downloads)</label>
    {#snippet footer()}
      <span class="grow"></span>
      <button class="btn" onclick={close}>Cancel</button>
      <button class="btn primary" onclick={okDuplicate}>OK</button>
    {/snippet}
  </Shell>
{:else if page === 'tool'}
  <Shell {theme} icon="category-video" title="Downloading yt-dlp" subtitle="From the official yt-dlp releases on GitHub">
    <div class="bar"><div style:width="{tool && tool.total > 0 ? Math.round((tool.done / tool.total) * 100) : 0}%"></div></div>
    <div class="mono muted">{tool ? fmtSize(tool.done) : ''}{tool && tool.total > 0 ? ` of ${fmtSize(tool.total)}` : ''}</div>
    {#snippet footer()}
      <span class="grow"></span>
      <button class="btn" onclick={() => call('ytdlp_cancel')}>Cancel</button>
    {/snippet}
  </Shell>
{:else if page === 'busy'}
  <div class="shell" style="align-items:center;justify-content:center"><div class="muted">Reading the address…</div></div>
{:else if page === 'info'}
  <Shell {theme} icon={catIcon(category)} tile title="Download File Info" subtitle={basename(path)}>
    <div style="display:flex;gap:16px">
      <div style="flex:1;display:flex;flex-direction:column;gap:10px;min-width:0">
        <Row label="URL" width={90}><input class="input mono" value={url} readonly={!!media} oninput={(e) => (url = e.currentTarget.value)} /></Row>
        <Row label="Category" width={90}>
          <Combo {theme} style="flex:0 1 auto;min-width:200px" value={category} items={categories.map((c) => ({ value: c.name, icon: catIcon(c.name) }))}
            onchange={setCategory} />
          <span class="grow"></span>
          <button class="roundBtn" title="Add category" onclick={() => (newCategory = true)}>+</button>
        </Row>
        <Row label="Save As" width={90}>
          <Combo editable mono {theme} items={paths} bind:value={path} />
          <button class="btn small" style="min-width:36px" onclick={browse}>...</button>
        </Row>
        <div style="padding-left:100px"><label class="check"><input type="checkbox" bind:checked={remember} /> Remember this path for "{category}" category</label></div>
        {#if media}
          <div class="panel">
            <div class="caption">YouTube video</div>
            <div style="font-weight:700" class="wrap">{media.title}</div>
            <div class="muted">{[media.uploader, media.duration ? length(media.duration) : ''].filter(Boolean).join(' · ')}</div>
            <div style="display:flex;gap:16px;align-items:center">
              <label class="radio"><input type="radio" checked={kind === 'video'} disabled={!media.variants.some((v) => v.kind === 'video')} onchange={() => setKind('video')} /> Video</label>
              <label class="radio"><input type="radio" checked={kind === 'audio'} disabled={!media.variants.some((v) => v.kind === 'audio')} onchange={() => setKind('audio')} /> Audio only</label>
              <span class="formLabel" style="margin-left:10px">Quality</span>
              <select class="input" style="flex:1" value={variant ? media.variants.indexOf(variant) : 0}
                onchange={(e) => { variant = media!.variants[Number(e.currentTarget.value)]; quality() }}>
                {#each media.variants as v, i}{#if v.kind === kind}<option value={i}>{v.label}</option>{/if}{/each}
              </select>
            </div>
          </div>
        {/if}
        <Row label="Description" width={90}><input class="input" bind:value={description} /></Row>
        {#if !media}
          <Row label="Checksum" width={90} top>
            <div style="flex:1;display:flex;flex-direction:column;gap:4px">
              <ChecksumEdit bind:this={checksumEdit} bind:text={checksumText} bind:algorithm={checksumAlgo} bind:valid={checksumValid} bind:value={checksumValue} />
            </div>
          </Row>
        {/if}
      </div>
      <div style="width:116px;flex:none;display:flex;flex-direction:column;align-items:center;gap:8px;padding-top:4px">
        <div class="tile large"><Icon name={catIcon(category)} size={39} {theme} /></div>
        <div class="mono" style="font-weight:700">{variant?.convert === 'mp3' ? '~' : ''}{fmtSize(size)}</div>
        {#if resumable !== null}<Pill text={resumable ? '✓ Resumable' : 'Not resumable'} tone={resumable ? 'ok' : 'muted'} />{/if}
      </div>
    </div>
    {#snippet footer()}
      <span class="grow"></span>
      <button class="btn" disabled={!checksumValid} onclick={(e) => { const r = (e.currentTarget as HTMLElement).getBoundingClientRect(); const h = queues.length * 32 + 14; later = { x: r.left, y: r.bottom + 2 + h <= window.innerHeight ? r.bottom + 2 : r.top - 2 - h } }}>
        <Icon name="queue" size={15} {theme} /> Download <u>L</u>ater
      </button>
      <button class="btn primary" bind:this={startButton} disabled={!checksumValid} onclick={() => submit('start', '')}><Icon name="add-white" size={15} {theme} /> <u>S</u>tart Download</button>
      <button class="btn" onclick={close}><u>C</u>ancel</button>
    {/snippet}
  </Shell>
{/if}
{#if later}
  <!-- svelte-ignore a11y_no_static_element_interactions a11y_click_events_have_key_events -->
  <div style="position:fixed;inset:0;z-index:999" onmousedown={() => (later = null)}></div>
  <Menu items={queues.map((q) => ({ type: 'action', id: q, label: q, enabled: true }))} x={later.x} y={later.y} {theme}
    onpick={(q) => { later = null; submit('later', q) }} onclose={() => (later = null)} />
{/if}
{#if newCategory}
  <div class="overlay"><div class="dialog" style="max-width:640px;min-width:600px">
    <CategoryForm category={null} {theme} onok={addCategory} oncancel={() => (newCategory = false)} />
  </div></div>
{/if}
{#if box}<Dialog {box} {theme} />{/if}
