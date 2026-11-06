// UI scenarios run inside the real window (WebKitGTK + the real backend), started with LIDER_E2E=main
// on a copy of tests/golden/v1.2. Port of the window checks of tests/test_ui.py (docs/tauri/PLAN.md §7).
import { invoke } from '@tauri-apps/api/core'

const log: string[] = []
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms))

async function waitFor<T>(what: string, f: () => T | null | undefined | false, ms = 5000): Promise<T> {
  const end = Date.now() + ms
  for (;;) {
    const v = f()
    if (v) return v as T
    if (Date.now() > end) throw new Error(`timed out waiting for ${what}`)
    await sleep(40)
  }
}

const all = (sel: string) => Array.from(document.querySelectorAll<HTMLElement>(sel))
const texts = (sel: string) => all(sel).map((e) => e.textContent!.trim())
const rowNames = () => all('.list .row').map((r) => r.querySelectorAll('.cell')[2]?.textContent?.trim() ?? '')
const node = (title: string) => all('.node').find((n) => n.querySelector('.title')?.textContent === title)!
const header = (title: string) => all('.list .header .h').find((h) => h.textContent!.startsWith(title))!

function eq(a: unknown, b: unknown, what: string) {
  const x = JSON.stringify(a)
  const y = JSON.stringify(b)
  if (x !== y) throw new Error(`${what}: got ${x}, expected ${y}`)
  log.push(`ok  ${what}`)
}

function mouse(el: Element, type: string, opts: MouseEventInit = {}) {
  el.dispatchEvent(new MouseEvent(type, { bubbles: true, button: 0, ...opts }))
}

function click(el: Element, opts: MouseEventInit = {}) {
  mouse(el, 'mousedown', opts)
  mouse(el, 'mouseup', opts)
  mouse(el, 'click', opts)
}

function key(k: string, opts: KeyboardEventInit = {}) {
  window.dispatchEvent(new KeyboardEvent('keydown', { key: k, bubbles: true, ...opts }))
}

/** Open a menu of the in-window menu bar and follow the path of item labels. */
async function menu(top: string, ...path: string[]) {
  const bar = await waitFor(`menu ${top}`, () => all('.menubar > button').find((b) => b.textContent!.trim() === top))
  mouse(bar, 'mousedown')
  for (const p of path) {
    const item = await waitFor(`menu item ${p}`, () => all('.menu .item').find((i) => i.textContent!.trim().startsWith(p)))
    if (item.querySelector('.chev')) mouse(item, 'mouseenter')
    else click(item)
    await sleep(80)
  }
}

async function main() {
  await waitFor('rows', () => all('.list .row').length === 10)
  eq(all('.list .row').length, 10, 'the golden profile shows 10 downloads')
  eq(texts('.menubar > button'), ['Tasks', 'File', 'Downloads', 'View', 'Help'], 'menu bar layout')
  const top = all('.sidebar .node[data-depth="0"]').map((n) => n.querySelector('.title')!.textContent)
  const children = all('.sidebar .node[data-depth="1"]').slice(0, 6).map((n) => n.querySelector('.title')!.textContent)
  eq(children, ['Compressed', 'Documents', 'Music', 'Programs', 'Video', 'Images'], 'All Downloads lists the categories without General')
  eq(top, ['All Downloads', 'Unfinished', 'Finished', 'Grabber projects', 'Queues'], 'category tree top nodes')

  // test_tree_filters
  click(node('Finished'))
  await waitFor('finished rows', () => all('.list .row').length === 3)
  eq(rowNames().sort(), ['bad.zip', 'doc.pdf', 'state-verifying.bin'], 'Finished shows the complete downloads')
  click(node('Nightly'))
  await waitFor('queue rows', () => rowNames()[0] === 'state-connecting.bin')
  eq(rowNames(), ['state-connecting.bin', 'state-assembling.bin', 'state-verifying.bin'], 'a queue node sorts by queue order')
  click(node('All Downloads'))
  await waitFor('all rows', () => all('.list .row').length === 10)

  // Sorting by a header
  click(header('FILE NAME'))
  await waitFor('sorted', () => rowNames()[0] === 'bad.zip')
  eq(rowNames()[0], 'bad.zip', 'File Name sorts ascending')
  click(header('FILE NAME'))
  await waitFor('sorted desc', () => rowNames()[0] === 'Video title.mp4')
  eq(rowNames()[0], 'Video title.mp4', 'a second click sorts descending')

  // test_checked_rows_drive_start_stop_delete
  click(all('.list .row')[0].querySelector('.c0')!)
  await waitFor('one check', () => all('.chip').length === 1)
  click(all('.list .row')[1].querySelector('.c0')!)
  await waitFor('chip', () => all('.chip')[0]?.textContent!.includes('2 checked'))
  eq(all('.list .header .box.partial').length, 1, 'two checks make the header box partial')
  click(all('.list .header .c0')[0])
  await waitFor('all checked', () => all('.list .header .box.on').length === 1)
  eq(all('.chip')[0].textContent!.includes('10 checked'), true, 'the header box checks every row')
  click(all('.list .header .c0')[0])
  await waitFor('none checked', () => all('.chip').length === 0)
  eq(all('.list .row .box.on').length, 0, 'a second click unchecks every row')

  // test_details_pane_follows_the_highlighted_row (the pane is off in the golden profile)
  eq(all('.details').length, 0, 'no details pane while it is turned off')
  await menu('View', 'Details pane')
  await sleep(250)
  eq(all('.details').length, 0, 'no details pane without a selection')
  click(all('.list .row').find((r) => r.textContent!.includes('big.tar.gz'))!)
  await waitFor('details', () => document.querySelector('.details')?.textContent!.includes('big.tar.gz'))
  eq(document.querySelector('.details .big')!.textContent, '30.0%', 'the details pane shows the selected download')
  key('F9')
  await waitFor('details hidden', () => all('.details').length === 0)
  log.push('ok  F9 hides the details pane')

  // test_searchbar_filter_and_find_next
  key('f', { ctrlKey: true })
  const field = await waitFor('search field', () => document.querySelector<HTMLInputElement>('.searchbar input'))
  // The golden profile saved search_flags 0x05: file names, match case.
  field.value = 'STATE'
  field.dispatchEvent(new Event('input', { bubbles: true }))
  field.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }))
  await waitFor('no match', () => all('.list .row').length === 0)
  log.push('ok  with Match case on, "STATE" matches nothing')
  field.value = 'state'
  field.dispatchEvent(new Event('input', { bubbles: true }))
  field.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }))
  await waitFor('filtered', () => all('.list .row').length === 3)
  eq(rowNames().every((n) => n.startsWith('state-')), true, 'search filters the list')
  field.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
  await waitFor('unfiltered', () => all('.list .row').length === 10)
  eq(all('.searchbar').length, 0, 'Esc closes the search bar and shows everything')
  key('F3')
  await waitFor('find next', () => all('.list .row.selected').length === 1)
  const first = all('.list .row.selected')[0].textContent!
  eq(first.includes('state-'), true, 'F3 selects the next match with the bar closed')

  // test_remove_selected_asks_and_removes
  click(all('.list .row').find((r) => r.textContent!.includes('stream.bin'))!)
  await waitFor('selected', () => all('.list .row.selected')[0]?.textContent!.includes('stream.bin'))
  key('Delete')
  const yes = await waitFor('confirm', () => all('.dialog .btn').find((b) => b.textContent === 'Yes'))
  eq(document.querySelector('.dialog .title')!.textContent, 'Confirm deletion of downloads', 'Delete asks first')
  click(yes)
  await waitFor('removed', () => all('.list .row').length === 9)
  eq(rowNames().includes('stream.bin'), false, 'Yes removes the download')

  // test_theme_choice_switches_palette
  await menu('View', 'Theme', 'Ember')
  await waitFor('ember', () => document.documentElement.dataset.theme === 'ember')
  await menu('View', 'Theme', 'Dark')
  await waitFor('dark', () => document.documentElement.dataset.theme === 'dark')
  log.push('ok  View → Theme switches the palette')

  // test_toolbar_captions (labels are off in the golden profile)
  await menu('View', 'Toolbar', 'Icons and labels')
  await waitFor('labels', () => texts('.toolbar .tb').includes('Start'))
  eq(texts('.toolbar .tb'), ['Add URL', 'Start', 'Stop', 'Delete', 'Options', 'Scheduler', 'Grabber'], 'toolbar captions')

  // test_view_menu_ends_with_appearance
  const viewButton = all('.menubar > button').find((b) => b.textContent!.trim() === 'View')!
  mouse(viewButton, 'mousedown')
  await waitFor('view menu', () => all('.menu .item').length > 5)
  eq(all('.menu > .item').pop()!.textContent!.trim(), 'Appearance...', 'the View menu ends with Appearance...')
  mouse(viewButton, 'mousedown')
}

export async function run() {
  let ok = true
  try {
    await main()
  } catch (e) {
    ok = false
    log.push(`FAIL ${e instanceof Error ? e.message : String(e)}`)
  }
  await invoke('e2e_report', { ok, log: log.join('\n') })
}
