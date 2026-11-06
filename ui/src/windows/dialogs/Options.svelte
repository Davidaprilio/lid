<script lang="ts">
  // "LID options" (1.x options.OptionsDialog): seven tabs over a working copy; OK writes it all.
  import { open as openDir } from '@tauri-apps/plugin-dialog'
  import TabStrip from '../../components/form/TabStrip.svelte'
  import Row from '../../components/form/Row.svelte'
  import Pill from '../../components/form/Pill.svelte'
  import TagInput from '../../components/form/TagInput.svelte'
  import Icon from '../../components/Icon.svelte'
  import CategoryForm from './CategoryForm.svelte'
  import { listen } from '@tauri-apps/api/event'
  import Combo from '../../components/form/Combo.svelte'
  import { call, close, followTheme, params } from '../../lib/dialog'
  import { openDialog } from '../../lib/api'
  import { token, TITLES, type ThemeName } from '../../theme/tokens'

  interface Category { name: string; extensions: string[]; save_dir: string; sites: string[]; remember_last: boolean }
  const TABS = ['General', 'File types', 'Save to', 'Downloads', 'Connection', 'Proxy / Socks', 'Appearance']
  const CHOICES = [1, 2, 4, 8, 16, 24, 32]
  let theme = $state<ThemeName>('ember')
  let tab = $state(Math.max(0, TABS.indexOf(params().get('tab') ?? 'General')))
  let s = $state<any>(null)
  let desk = $state<any>(null)
  let inMenu = $state(false)
  let launch = $state(false)
  let fileTypes = $state<string[]>([])
  let noSites = $state<string[]>([])
  let cats = $state<Category[]>([])
  let catIndex = $state(0)
  let catExts = $state<string[]>([])
  let catDir = $state('')
  let limits = $state<[string, number][]>([])
  let limitSel = $state(-1)
  let useProxy = $state(false)
  let proxy = $state('http://127.0.0.1:8080')
  let editCat = $state<{ cat: Category | null; index: number } | null>(null)
  let editLimit = $state<{ index: number; pattern: string; n: number } | null>(null)
  let fontLabel = $state('')

  followTheme((t) => (theme = t))
  ;(async () => {
    // Both before anything draws: the Appearance tab reads the desktop state.
    const [sv, dv] = await Promise.all([call<any>('settings_get'), call<any>('desktop_state')])
    desk = dv
    s = sv
    inMenu = desk.system_installed || desk.in_menu
    launch = desk.autostart
    fileTypes = s.auto_capture_types.split(/\s+/).filter(Boolean).map((x: string) => x.toLowerCase())
    noSites = s.no_auto_sites.split(/\s+/).filter(Boolean)
    cats = JSON.parse(JSON.stringify(s.categories))
    selectCat(0)
    limits = Object.entries(s.server_limits as Record<string, number>).sort((a, b) => a[0].localeCompare(b[0]))
    useProxy = !!s.proxy
    if (s.proxy) proxy = s.proxy
    updateFont()
  })()

  function updateFont() {
    const parts = (s.list_font as string).split(',')
    // The list's own default: the window font (13 px), in points.
    const pt = Math.round(parseFloat(getComputedStyle(document.body).fontSize) * 0.75)
    fontLabel = parts[0] ? `${parts[0]} · ${parts[1] ?? pt} pt` : `System default · ${pt} pt`
  }

  // Save to: switching category keeps the edits of the previous one.
  function storeCat() {
    if (!cats[catIndex]) return
    cats[catIndex].extensions = catExts
    if (catDir.trim()) cats[catIndex].save_dir = catDir.trim()
  }
  function selectCat(i: number) {
    storeCat()
    catIndex = i
    catExts = [...cats[i].extensions]
    catDir = cats[i].save_dir
  }
  function savedCat(c: Category) {
    const ed = editCat!
    editCat = null
    if (ed.cat === null) {
      if (cats.some((x) => x.name === c.name)) return
      cats = [...cats, c]
      selectCat(cats.length - 1)
    } else {
      cats[ed.index] = c
      catExts = [...c.extensions]
      catDir = c.save_dir
    }
  }

  async function browse(set: (v: string) => void, from: string) {
    const d = await openDir({ directory: true, title: 'Select a Folder', defaultPath: from })
    if (typeof d === 'string') set(d)
  }

  function saveLimit() {
    const e = editLimit!
    editLimit = null
    const next = limits.filter((_, i) => i !== e.index)
    if (e.pattern.trim()) next.push([e.pattern.trim(), e.n])
    limits = next.sort((a, b) => a[0].localeCompare(b[0]))
  }

  // Only the keys this dialog edits are written (sorting or column widths may change meanwhile).
  const KEYS = ['monitor_clipboard', 'browser_capture', 'temp_dir', 'show_progress_dialog', 'show_start_dialog', 'only_add_to_queue',
    'show_complete_dialog', 'duplicate_action', 'user_agent', 'verify_checksum', 'checksum_from_clipboard', 'checksum_mismatch',
    'max_connections', 'retries', 'theme', 'follow_system', 'dark_theme', 'density', 'toolbar_labels', 'menubar', 'show_categories',
    'show_details', 'show_statusbar', 'tray_icon', 'close_to_tray', 'start_minimized']

  async function ok() {
    storeCat()
    const patch: Record<string, unknown> = Object.fromEntries(KEYS.map((k) => [k, s[k]]))
    Object.assign(patch, {
      launch_on_startup: launch,
      auto_capture_types: fileTypes.join(' '),
      no_auto_sites: noSites.join(' '),
      categories: cats,
      server_limits: Object.fromEntries(limits),
      proxy: useProxy ? proxy.trim() : '',
      retries: Math.max(0, Math.min(100, Math.round(Number(s.retries) || 0))),
    })
    patch.list_font = s.list_font
    if (!String(s.temp_dir).trim()) delete patch.temp_dir
    if (!String(s.user_agent).trim()) delete patch.user_agent
    await call('options_apply', { settings: patch, menu: desk.system_installed ? null : inMenu })
    close()
  }

  // The font window sends its choice back; like the other fields it is written on OK.
  $effect(() => {
    const off = listen<string>('lid://font-chosen', (e) => { if (s) { s.list_font = e.payload; updateFont() } })
    return () => { off.then((f) => f()) }
  })

  const catIcon = (name: string) => (['compressed', 'documents', 'music', 'programs', 'video'].includes(name.toLowerCase()) ? `category-${name.toLowerCase()}` : 'folder')
  const preview = (t: ThemeName) => ({ win: token(t, 'win'), pane: token(t, 'pane'), ac: token(t, 'ac'), line: token(t, 'line'), tx: token(t, 'tx'), track: token(t, 'track') })
  const header = (icon: string, title: string) => ({ icon, title })
  const HEAD = [header('general', 'Desktop Integration'), header('filetypes', 'Downloaded file types'), header('saveto', 'Categories, file types, folders'),
    header('folder-download', 'Default download settings'), header('connection', 'Connections and Limits'), header('proxy', 'Proxy'), header('general', 'Appearance')]
</script>

<svelte:window onkeydown={(e) => {
  if (editCat || e.defaultPrevented) return
  if (editLimit) {
    if (e.key === 'Escape') editLimit = null
    else if (e.key === 'Enter' && (e.target as HTMLElement).tagName !== 'BUTTON') { e.preventDefault(); saveLimit() }
    return
  }
  if (e.key === 'Escape') close()
}} />
{#if s}
  <div class="shell">
    <div style="padding-top:10px"><TabStrip names={TABS} bind:active={tab} /></div>
    <div class="shell-body" style="overflow:auto">
      <div style="display:flex;gap:10px;align-items:center"><Icon name={HEAD[tab].icon} size={22} {theme} /><b style="font-size:1.17em">{HEAD[tab].title}</b></div>
      <div class="hr"></div>
      {#if tab === 0}
        <label class="check"><input type="checkbox" bind:checked={launch} /> Launch LID on startup</label>
        <label class="check" title={desk.system_installed ? 'Added by the installed LID package' : null}>
          <input type="checkbox" bind:checked={inMenu} disabled={desk.system_installed} /> Show LID in the application menu</label>
        <label class="check"><input type="checkbox" bind:checked={s.monitor_clipboard} /> Automatically start downloading of URLs placed to clipboard</label>
        <fieldset class="group">
          <legend>Browser integration</legend>
          <label class="check"><input type="checkbox" bind:checked={s.browser_capture} /> Capture downloads from browsers (needs the LID extension)</label>
          <div class="faint">The extension takes over downloads of the file types on the File types tab, and adds “Download with LID”, “Download all links with LID” and a YouTube button.</div>
          <div style="display:flex;gap:10px;align-items:center">
            <button class="btn" onclick={() => openDialog('browsers', 'browsers', 'Browser integration', 680, 600)}>Set up browsers...</button>
            {#if desk.browsers_connected > 0}<Pill tone="ok" text="✓ {desk.browsers_connected} browser{desk.browsers_connected === 1 ? '' : 's'} connected" />
            {:else}<Pill text="Not set up yet" />{/if}
          </div>
        </fieldset>
      {:else if tab === 1}
        <div>Automatically start downloading the following file types:</div>
        <TagInput bind:values={fileTypes} placeholder="Add a file type…" kind="extension" noun="types" />
        <div style="display:flex;justify-content:flex-end">
          <button class="btn" title="Back to the default file types" onclick={() => (fileTypes = desk.default_file_types.split(' ').map((x: string) => x.toLowerCase()))}>Default</button>
        </div>
        <div>Don't start downloading automatically from the following sites:</div>
        <TagInput bind:values={noSites} placeholder="Add a site, e.g. *.example.com" kind="site" noun="sites" />
      {:else if tab === 2}
        <fieldset class="group">
          <legend>Save to...</legend>
          <Row label="Category" width={80}>
            <Combo {theme} style="flex:0 1 auto;min-width:200px" value={String(catIndex)} onchange={(v) => selectCat(Number(v))}
              items={cats.map((c, i) => ({ value: String(i), label: c.name, icon: catIcon(c.name) }))} />
            <button class="btn" onclick={() => { storeCat(); editCat = { cat: null, index: -1 } }}>New</button>
            <button class="btn" onclick={() => { storeCat(); editCat = { cat: cats[catIndex], index: catIndex } }}>Edit...</button>
          </Row>
          <div>Automatically put in “{cats[catIndex]?.name}” the following file types:</div>
          <TagInput bind:values={catExts} placeholder="Add a type…" kind="extension" noun="types" />
          <div>Default download directory for “{cats[catIndex]?.name}”</div>
          <div style="display:flex;gap:8px"><input class="input mono" style="flex:1" bind:value={catDir} /><button class="btn" onclick={() => browse((v) => (catDir = v), catDir)}>Browse</button></div>
        </fieldset>
        <fieldset class="group">
          <legend>Temporary directory</legend>
          <div style="display:flex;gap:8px"><input class="input mono" style="flex:1" bind:value={s.temp_dir} /><button class="btn" onclick={() => browse((v) => (s.temp_dir = v), s.temp_dir)}>Browse</button></div>
          <div class="faint">Parts are stored here while downloading. The finished file moves to its category folder.</div>
        </fieldset>
      {:else if tab === 3}
        <label class="check"><input type="checkbox" bind:checked={s.show_progress_dialog} /> Show "Download progress" dialog</label>
        <label class="check"><input type="checkbox" bind:checked={s.show_start_dialog} /> Show start download dialog</label>
        <label class="check" style="padding-left:24px"><input type="checkbox" bind:checked={s.only_add_to_queue} disabled={!s.show_start_dialog} /> Do not start downloading, only add files to the queue</label>
        <label class="check"><input type="checkbox" bind:checked={s.show_complete_dialog} /> Show download complete dialog</label>
        <Row label="If a duplicate download link is added" width={250}>
          <select class="input" bind:value={s.duplicate_action}>
            <option value="ask">Show the dialog and ask me</option>
            <option value="number">Add the duplicate with a numbered file name</option>
            <option value="overwrite">Add the duplicate and overwrite the existing file</option>
            <option value="complete">If existing file is complete, show download complete dialog; otherwise resume it</option>
          </select>
        </Row>
        <Row label="User-Agent for manual downloads" width={250}><input class="input mono" bind:value={s.user_agent} /></Row>
        <div style="display:flex;gap:8px;align-items:center;margin-top:6px"><Icon name="checksum" size={18} {theme} /><b>Checksum verification</b></div>
        <div class="hr"></div>
        <label class="check"><input type="checkbox" bind:checked={s.verify_checksum} /> Verify the checksum when a download finishes (when one was entered)</label>
        <div class="faint" style="padding-left:24px">Reads the whole file once, after its parts are merged. Large files take a few seconds; the list shows “Verifying”.</div>
        <label class="check"><input type="checkbox" bind:checked={s.checksum_from_clipboard} /> Fill in the checksum from the clipboard when it holds a hash</label>
        <Row label="If the checksum doesn't match" width={250}>
          <select class="input" bind:value={s.checksum_mismatch}>
            <option value="ask">Ask me — show the mismatch dialog</option>
            <option value="keep">Keep the file, mark it “Checksum mismatch”</option>
            <option value="redownload">Delete the file and download it again</option>
          </select>
        </Row>
      {:else if tab === 4}
        <fieldset class="group">
          <legend>Max. connections number</legend>
          <Row label="Default max. conn. number" width={190}>
            <select class="input" style="width:100px" bind:value={s.max_connections}>{#each CHOICES as c}<option value={c}>{c}</option>{/each}</select>
          </Row>
          <div>Exceptions</div>
          <div style="display:flex;gap:10px">
            <div class="list2" style="flex:1;height:160px">
              <div class="h"><span style="width:220px">SERVER</span><span>MAX. CONNECTIONS</span></div>
              {#each limits as l, i}
                <!-- svelte-ignore a11y_click_events_have_key_events -->
                <div class="r mono" class:sel={limitSel === i} role="option" aria-selected={limitSel === i} tabindex="-1" onclick={() => (limitSel = i)}>
                  <span style="width:220px">{l[0]}</span><span>{l[1]}</span>
                </div>
              {/each}
            </div>
            <div style="display:flex;flex-direction:column;gap:6px">
              <button class="btn" onclick={() => (editLimit = { index: -1, pattern: '', n: 2 })}>New</button>
              <button class="btn" disabled={limitSel < 0} onclick={() => { limits = limits.filter((_, i) => i !== limitSel); limitSel = -1 }}>Delete</button>
              <button class="btn" disabled={limitSel < 0} onclick={() => (editLimit = { index: limitSel, pattern: limits[limitSel][0], n: limits[limitSel][1] })}>Edit</button>
            </div>
          </div>
        </fieldset>
        <Row label="Reconnection attempts for each connection" width={280}><input class="input mono" type="number" min="0" max="100" style="width:90px" bind:value={s.retries} /></Row>
      {:else if tab === 5}
        <label class="check"><input type="checkbox" bind:checked={useProxy} /> Use proxy</label>
        <Row label="Proxy address" width={110}><input class="input mono" bind:value={proxy} disabled={!useProxy} /></Row>
        <div class="faint">Format: http://[user:password@]host:port — used for HTTP and HTTPS.</div>
      {:else}
        <div class="caption">Theme</div>
        <div style="display:flex;gap:10px">
          {#each ['ember', 'ember-dark', 'dark'] as t}
            {@const c = preview(t as ThemeName)}
            <!-- svelte-ignore a11y_click_events_have_key_events -->
            <div class="themeCard" role="radio" aria-checked={s.theme === t} tabindex="-1" class:selected={s.theme === t} onclick={() => (s.theme = t)}>
              <div style="height:78px;border-radius:7px;overflow:hidden;border:1px solid {c.line};background:{c.win};display:flex;flex-direction:column">
                <div style="height:16px;display:flex;align-items:center;gap:4px;padding:0 6px;border-bottom:1px solid {c.line}"><span style="width:22px;height:8px;border-radius:4px;background:{c.ac}"></span><span style="width:12px;height:6px;border-radius:3px;background:{c.track}"></span></div>
                <div style="flex:1;display:flex"><div style="width:24%;border-right:1px solid {c.line}"></div>
                  <div style="flex:1;background:{c.pane};display:flex;flex-direction:column;gap:5px;padding:6px">
                    {#each [0.7, 0.45, 0.85] as w}<div style="height:6px;border-radius:3px;background:{c.track};width:{w * 100}%"></div>{/each}
                  </div></div>
              </div>
              <label class="radio" style="font-weight:600"><input type="radio" checked={s.theme === t} onchange={() => (s.theme = t)} /> {TITLES[t as ThemeName]}</label>
            </div>
          {/each}
        </div>
        <div style="display:flex;gap:12px;align-items:center">
          <label class="check"><input type="checkbox" bind:checked={s.follow_system} /> Follow the system light / dark setting</label>
          <span class="muted">When dark, use</span>
          <select class="input" bind:value={s.dark_theme} disabled={!s.follow_system}>
            <option value="ember-dark">Ember Dark</option><option value="dark">Dark</option>
          </select>
        </div>
        <Row label="List density" width={110}><span class="seg"><button class:on={s.density === 'comfortable'} onclick={() => (s.density = 'comfortable')}>Comfortable</button><button class:on={s.density === 'compact'} onclick={() => (s.density = 'compact')}>Compact</button></span></Row>
        <Row label="Toolbar" width={110}><span class="seg"><button class:on={s.toolbar_labels} onclick={() => (s.toolbar_labels = true)}>Icons and labels</button><button class:on={!s.toolbar_labels} onclick={() => (s.toolbar_labels = false)}>Icons only</button></span></Row>
        <Row label="Menu bar" width={110}>
          <span class="seg">{#each [['native', 'Native'], ['custom', 'In window'], ['both', 'Both']] as [k, l]}<button class:on={s.menubar === k} onclick={() => (s.menubar = k)}>{l}</button>{/each}</span>
          <span class="faint">{desk.global_menu ? 'Native = desktop global menu' : 'No global menu here: Native shows it in the window'}</span>
        </Row>
        <Row label="List font" width={110}>
          <input class="input" readonly value={fontLabel} />
          <button class="btn" onclick={() => openDialog('font-options', `font?for=options&font=${encodeURIComponent(s.list_font)}`, 'Choose Font', 460, 520)}>Change...</button>
          <button class="ghostBtn" onclick={() => { s.list_font = ''; updateFont() }}>Reset</button>
        </Row>
        <div style="display:grid;grid-template-columns:1fr 1fr;gap:8px 18px">
          <label class="check"><input type="checkbox" bind:checked={s.show_categories} /> Show Categories panel</label>
          <label class="check"><input type="checkbox" bind:checked={s.show_details} /> Show details pane (F9)</label>
          <label class="check"><input type="checkbox" bind:checked={s.show_statusbar} /> Show status bar</label>
          <label class="check"><input type="checkbox" bind:checked={s.tray_icon} /> Show icon in system tray</label>
          <label class="check"><input type="checkbox" bind:checked={s.close_to_tray} /> Close button hides to tray</label>
          <label class="check"><input type="checkbox" bind:checked={s.start_minimized} /> Start minimized to tray</label>
        </div>
      {/if}
    </div>
    <div class="shell-footer">
      <span class="grow"></span>
      <button class="btn" onclick={close}>Cancel</button>
      <button class="btn primary" onclick={ok}>OK</button>
    </div>
  </div>
{/if}
{#if editCat}
  <div class="overlay"><div class="dialog" style="max-width:640px;min-width:600px">
    <CategoryForm category={editCat.cat} {theme} onok={savedCat} oncancel={() => (editCat = null)} />
  </div></div>
{/if}
{#if editLimit}
  <div class="overlay"><div class="dialog" style="min-width:480px">
    <div class="body">
      <div class="dialogTitle" style="font-size:1.05em">Max. connections number for a server</div>
      <Row label="Server" width={170}><input class="input mono" bind:value={editLimit.pattern} /></Row>
      <div class="faint" style="padding-left:180px">You may use an asterisk as a wildcard.</div>
      <Row label="Max. connections number" width={170}>
        <select class="input" style="width:100px" bind:value={editLimit.n}>{#each CHOICES as c}<option value={c}>{c}</option>{/each}</select>
      </Row>
    </div>
    <div class="footer"><button class="btn" onclick={() => (editLimit = null)}>Cancel</button><button class="btn primary" onclick={saveLimit}>OK</button></div>
  </div></div>
{/if}
