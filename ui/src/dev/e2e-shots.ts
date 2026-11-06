// Screenshot scenario (LIDER_E2E=shots, tests/e2e/shots.sh under Xvfb): every window and message box in the
// three themes, saved by the backend (`e2e_shot`) for the side-by-side review with 1.x (PLAN.md phase 8).
import { invoke } from '@tauri-apps/api/core'
import { Window } from '@tauri-apps/api/window'
import { openDialog } from '../lib/api'

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms))
const THEMES = ['ember', 'ember-dark', 'dark']

type Shot = [name: string, label: string, route: string, title: string, w: number, h: number]
function windows(port: string): Shot[] {
  const fileUrl = `http://127.0.0.1:${port}/archive.zip`
  const req = (o: object) => encodeURIComponent(JSON.stringify(o))
  const add = { url: fileUrl, referer: '', parent_page: '', headers: {}, filename: '', ask_address: false }
  const links = JSON.stringify(['https://example.org/a/setup-1.2.exe', 'https://example.org/a/manual.pdf', 'https://example.org/a/photo.jpg', 'https://example.org/a/index.html'])
  const tabs = ['General', 'File types', 'Save to', 'Downloads', 'Connection', 'Proxy / Socks', 'Appearance']
  return [
    ['add-address', 'shot-add', 'add', 'Enter new address to download', 560, 300],
    ['add-file-info', 'shot-info', `add?req=${req(add)}`, 'Download File Info', 720, 420],
    ['batch', 'batch', 'batch', 'Batch download', 620, 520],
    ['links', 'links', `links?urls=${encodeURIComponent(links)}`, 'Download All Links', 840, 640],
    ['grabber', 'grabber', 'links?grabber=1', 'Grabber', 560, 200],
    ...tabs.map((t, i): Shot => [`options-${i + 1}-${t.replace(/\W+/g, '-').toLowerCase()}`, 'options', `options?tab=${encodeURIComponent(t)}`, 'LID options', 720, 712]),
    ['scheduler-schedule', 'scheduler', 'scheduler?queue=Main%20download%20queue', 'Scheduler', 860, 620],
    ['scheduler-files', 'scheduler', 'scheduler?queue=Main%20download%20queue&tab=files', 'Scheduler', 860, 620],
    ['limiter', 'limiter', 'limiter', 'Speed Limiter settings', 440, 300],
    ['columns', 'columns', 'columns', 'Columns', 440, 520],
    ['find', 'find', 'find?next=1', 'Find file', 460, 340],
    ['search-settings', 'find', 'find', 'LID Search settings', 460, 340],
    ['font', 'font', 'font', 'Choose Font', 460, 520],
    ['category-new', 'category', 'category', 'LID categories', 520, 420],
    ['category-edit', 'category', 'category?name=Video', 'LID categories', 520, 420],
    ['browsers', 'browsers', 'browsers', 'Browser integration', 620, 560],
    ['progress', 'progress-2', 'progress?id=2', '0% big.tar.gz', 640, 520],
    ['complete', 'complete-4', 'complete?id=4', 'Download complete', 560, 360],
    ['mismatch', 'mismatch-5', 'mismatch?id=5', 'Checksum mismatch', 560, 420],
    ['check-hash', 'checkhash-4', 'checkhash?id=4', 'Check hash', 560, 480],
    ['properties', 'properties-4', 'properties?id=4', 'File Properties', 560, 600],
  ]
}

export async function run(dispatch: (id: string) => Promise<unknown>, select: (name: string) => void) {
  const log: string[] = []
  let ok = true
  const port = (await invoke<string | null>('e2e_env', { name: 'LIDER_SHOTS_PORT' })) ?? '0'
  try {
    for (const theme of THEMES) {
      await invoke('settings_patch', { patch: { theme, follow_system: false, proxy: '' } })
      await sleep(800)
      await invoke('e2e_shot', { label: 'main', name: `${theme}--main` })
      select('doc.pdf')
      await sleep(500)
      await invoke('e2e_shot', { label: 'main', name: `${theme}--main-details` })
      void dispatch('about') // resolves only when the box closes
      await sleep(500)
      await invoke('e2e_shot', { label: 'main', name: `${theme}--box-about` })
      window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
      document.querySelector<HTMLElement>('.overlay')?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
      await sleep(300)
      for (const [name, label, route, title, w, h] of windows(port)) {
        await openDialog(label, route, title, w, h, false)
        await sleep(name === 'add-file-info' ? 3500 : 1500)
        await invoke('e2e_shot', { label, name: `${theme}--${name}` })
        await (await Window.getByLabel(label))?.close()
        await sleep(300)
      }
      log.push(`ok  ${theme}`)
    }
  } catch (e) {
    ok = false
    log.push(`FAIL ${e instanceof Error ? e.message : String(e)}`)
  }
  await invoke('e2e_report', { ok, log: log.join('\n') })
}
