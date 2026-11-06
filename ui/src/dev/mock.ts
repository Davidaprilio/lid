// Outside Tauri (a plain browser, for layout checks): answer the backend calls with a saved view.
// Save one with LIDER_DUMP_VIEW=<file> and put it next to the page as view.json.
import { mockIPC } from '@tauri-apps/api/mocks'

const params = new URLSearchParams(location.search)
const view = await (await fetch(params.get('view') ?? './view.json')).json()
if (params.get('theme')) view.appearance.theme = params.get('theme')
if (params.get('select')) {
  const id = Number(params.get('select'))
  view.selected = [id]
  view.current = id
}

mockIPC(
  (cmd) => {
    switch (cmd) {
      case 'ready':
      case 'view':
        return view
      case 'app_info':
        return { name: 'LID', long_name: 'Linux Internet Downloader', version: 'mock', mono_family: 'monospace' }
      case 'targets_info':
        return { ids: [], completed: 0 }
      default:
        return null
    }
  },
  { shouldMockEvents: true },
)
