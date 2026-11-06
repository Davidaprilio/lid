// Dialog scenarios (LIDER_E2E=dialogs). The main window opens each dialog the way a user does; the
// dialog window runs its part and reports back with the "lid://e2e" event.
import { invoke } from '@tauri-apps/api/core'
import { emit, listen } from '@tauri-apps/api/event'

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms))
const all = (sel: string) => Array.from(document.querySelectorAll<HTMLElement>(sel))
const texts = (sel: string) => all(sel).map((e) => e.textContent!.trim())

async function waitFor<T>(what: string, f: () => T | null | undefined | false, ms = 6000): Promise<T> {
  const end = Date.now() + ms
  for (;;) {
    const v = f()
    if (v) return v as T
    if (Date.now() > end) throw new Error(`timed out waiting for ${what}`)
    await sleep(40)
  }
}

function eq(log: string[], a: unknown, b: unknown, what: string) {
  const x = JSON.stringify(a)
  const y = JSON.stringify(b)
  if (x !== y) throw new Error(`${what}: got ${x}, expected ${y}`)
  log.push(`ok  ${what}`)
}

function mouse(el: Element, type: string) {
  el.dispatchEvent(new MouseEvent(type, { bubbles: true, button: 0 }))
}
function click(el: Element) {
  mouse(el, 'mousedown')
  mouse(el, 'mouseup')
  mouse(el, 'click')
}
const button = (label: string) => waitFor(`button ${label}`, () => all('button').find((b) => b.textContent!.trim() === label))
function type(el: HTMLInputElement, value: string) {
  el.value = value
  el.dispatchEvent(new Event('input', { bubbles: true }))
  el.dispatchEvent(new Event('change', { bubbles: true }))
}
const checkbox = (label: string) =>
  waitFor(`checkbox ${label}`, () => all('label.check').find((l) => l.textContent!.trim().startsWith(label))?.querySelector('input'))

// ------------------------------------------------------------------ the dialog side

/** Each scenario checks its window and returns the last action (the one that closes the window), which
 * runs after the report is sent. */
type Final = (() => Promise<unknown> | unknown) | void
const DIALOG: Record<string, (log: string[]) => Promise<Final>> = {
  async options(log) {
    await waitFor('tabs', () => all('.tabstrip .tab').length === 7)
    eq(log, texts('.tabstrip .tab'), ['General', 'File types', 'Save to', 'Downloads', 'Connection', 'Proxy / Socks', 'Appearance'], 'Options has 7 tabs in order')
    await waitFor('appearance tab', () => all('.tabstrip .tab.on')[0]?.textContent === 'Appearance')
    log.push('ok  View → Appearance... opens the Appearance tab')
    click(await button('Compact'))
    const ok = await button('OK')
    return () => click(ok)
  },
  async limiter(log) {
    const field = await waitFor('speed', () => document.querySelector<HTMLInputElement>('input[type=number]'))
    eq(log, field.value, '512', 'the limiter shows the saved speed')
    type(field, '300')
    const ok = await button('OK')
    return () => click(ok)
  },
  async columns(log) {
    await waitFor('columns', () => all('.list2 .r').length === 12)
    const q = all('.list2 .r').find((r) => r.textContent!.trim() === 'Q')!
    click(q.querySelector('input')!)
    log.push('ok  the Columns dialog lists the 12 columns')
    const ok = await button('OK')
    return () => click(ok)
  },
  async batch(log) {
    const address = await waitFor('address', () => document.querySelector<HTMLInputElement>('.formrow input'))
    type(address, 'https://lid-e2e.invalid/f*.zip')
    const [from, to] = all('input[type=number]') as HTMLInputElement[]
    type(from, '1')
    type(to, '3')
    await waitFor('count', () => texts('.shell-footer .muted')[0] === '3 files will be added')
    log.push('ok  batch preview: 3 files will be added')
    click(await button('OK'))
    const no = await button('No')
    return () => click(no)
  },
  async find(log) {
    const mc = await checkbox('Match case')
    if (mc.checked) click(mc)
    log.push('ok  Search settings: Match case off')
    const ok = await button('OK')
    return () => click(ok)
  },
  async scheduler(log) {
    await waitFor('queues', () => all('.list2 .r').length >= 4)
    eq(log, texts('.list2 .r').slice(0, 2).map((t) => t.replace(/\d+$/, '').trim()), ['Main download queue', 'Synchronization queue'], 'Scheduler lists Main and Synchronization first')
    click(await button('New queue'))
    const field = await waitFor('name field', () => document.querySelector<HTMLInputElement>('.dialog input'))
    type(field, 'E2E queue')
    click(await button('OK'))
    await waitFor('new queue', () => texts('.list2 .r').some((t) => t.startsWith('E2E queue')))
    log.push('ok  New queue adds and selects it')
    const c = await button('Close')
    return () => click(c)
  },
  async add(log) {
    const field = await waitFor('address', () => document.querySelector<HTMLInputElement>('.formrow input'))
    type(field, 'https://lid-e2e.invalid/archive.zip')
    click(await button('OK'))
    click(await button('Yes')) // "Cannot get information about the file … Add it anyway?"
    await waitFor('file info', () => document.querySelector('.dialogTitle')?.textContent === 'Download File Info')
    eq(log, document.querySelector('.shell-titles .muted')!.textContent, 'archive.zip', 'Download File Info names the file')
    const cat = document.querySelector<HTMLSelectElement>('select')!
    eq(log, cat.value, 'Compressed', 'the category comes from the file type')
    const hash = document.querySelector<HTMLInputElement>('input[placeholder="Paste a hash (optional)"]')!
    type(hash, 'abc')
    await waitFor('invalid hash', () => document.querySelector('.errorText'))
    eq(log, (await button('Start Download')).hasAttribute('disabled'), true, 'an invalid checksum holds Start')
    type(hash, '')
    await waitFor('valid again', () => !document.querySelector('.errorText'))
    click(await button('Download Later'))
    const main = await waitFor('queue menu', () => all('.menu .item').find((i) => i.textContent!.includes('Main download queue')))
    return () => click(main)
  },
  async checkhash(log) {
    await waitFor('missing', () => document.querySelector('.errorText')?.textContent?.startsWith('File not found at'))
    eq(log, (await button('Check now')).hasAttribute('disabled'), true, 'Check hash: a missing file cannot be checked')
    const c = await button('Close')
    return () => click(c)
  },
  async properties(log) {
    const desc = await waitFor('description', () => all('.formrow').find((r) => r.textContent!.startsWith('Description'))?.querySelector('input'))
    type(desc as HTMLInputElement, 'changed in e2e')
    log.push('ok  Properties edits the description')
    const ok = await button('OK')
    return () => click(ok)
  },
  async category(log) {
    const name = await waitFor('name', () => document.querySelector<HTMLInputElement>('.formrow input'))
    type(name, 'E2E files')
    log.push('ok  the category editor opens')
    const ok = await button('OK')
    return () => click(ok)
  },
}

const errors: string[] = []
/** Catch script errors of the dialog window from the start, so a broken page reports why. */
export function watchErrors() {
  window.addEventListener('error', (e) => errors.push(`FAIL script error: ${e.message} at ${e.filename}:${e.lineno}`))
  window.addEventListener('unhandledrejection', (e) => errors.push(`FAIL unhandled: ${String(e.reason)}`))
}

export async function runDialog(page: string) {
  const f = DIALOG[page]
  if (!f) return
  const early = (window as unknown as { __lidErrors?: string[] }).__lidErrors ?? []
  const log: string[] = [...early.map((e) => `FAIL ${e}`), ...errors]
  let ok = log.length === 0
  let final: Final = undefined
  try {
    await sleep(300)
    final = await f(log)
  } catch (e) {
    ok = false
    log.push(`FAIL ${page}: ${e instanceof Error ? e.message : String(e)}`)
  }
  for (const e of (window as unknown as { __lidErrors?: string[] }).__lidErrors ?? []) if (!log.includes(`FAIL ${e}`)) log.push(`FAIL ${e}`)
  await emit('lid://e2e', { page, ok, log })
  if (final) await final()
}

/** Wait until an async check holds (the dialog's last action takes effect after its report). */
async function until(what: string, f: () => Promise<boolean>, ms = 6000) {
  const end = Date.now() + ms
  while (!(await f())) {
    if (Date.now() > end) throw new Error(`timed out waiting for ${what}`)
    await sleep(80)
  }
}

// ------------------------------------------------------------------ the main window side

export async function runMain(dispatch: (id: string) => Promise<unknown>, select: (name: string) => void) {
  const log: string[] = []
  let ok = true
  let waiting: ((r: { page: string; ok: boolean; log: string[] }) => void) | null = null
  await listen<{ page: string; ok: boolean; log: string[] }>('lid://e2e', (e) => waiting?.(e.payload))
  const step = (open: () => unknown) =>
    new Promise<void>((resolve, reject) => {
      const timer = setTimeout(() => reject(new Error('a dialog did not report back')), 20000)
      waiting = (r) => {
        clearTimeout(timer)
        log.push(...r.log)
        r.ok ? resolve() : reject(new Error(r.log[r.log.length - 1]))
      }
      open()
    })
  const settings = () => invoke<any>('settings_get')
  try {
    await waitFor('rows', () => all('.list .row').length === 10)
    await step(() => dispatch('appearance'))
    await until('density', async () => (await settings()).density === 'compact')
    log.push('ok  Options OK writes the density')
    await step(() => dispatch('limiter_settings'))
    await until('speed', async () => (await settings()).speed_limit_kbps === 300)
    log.push('ok  the limiter writes its speed')
    await step(() => dispatch('columns'))
    await waitFor('no Q column', () => !texts('.list .header .h').includes('Q'))
    log.push('ok  Customize columns hides the Q column')
    await step(() => dispatch('batch'))
    await waitFor('batch rows', () => all('.list .row').length === 13)
    log.push('ok  the batch adds 3 downloads')
    await step(() => dispatch('find_settings'))
    await until('flags', async () => ((await settings()).search_flags & 1) === 0)
    log.push('ok  Search settings write the flags')
    await step(() => dispatch('scheduler'))
    await waitFor('queue node', () => all('.node .title').some((t) => t.textContent === 'E2E queue'))
    log.push('ok  the new queue shows in the sidebar')
    await step(() => dispatch('add'))
    await waitFor('added', () => all('.list .row').length === 14 && all('.list .row').some((r) => r.textContent!.includes('archive.zip')))
    log.push('ok  Download Later adds the download')
    select('doc.pdf')
    await sleep(300)
    await step(() => dispatch('ctx_check_hash'))
    await step(() => dispatch('ctx_properties'))
    const docId = (await invoke<any>('view')).selected[0]
    await until('description', async () => (await invoke<any>('download_get', { id: docId })).description === 'changed in e2e')
    log.push('ok  Properties OK writes the description')
    await step(() => dispatch('cat_add'))
    await waitFor('category node', () => all('.node .title').some((t) => t.textContent === 'E2E files'))
    log.push('ok  a new category shows in the sidebar')
  } catch (e) {
    ok = false
    log.push(`FAIL ${e instanceof Error ? e.message : String(e)}`)
  }
  await invoke('e2e_report', { ok, log: log.join('\n') })
}
