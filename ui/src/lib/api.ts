// Calls into the backend.
import { invoke } from '@tauri-apps/api/core'
import type { MainView, TreeFilter } from './types'

export const api = {
  ready: () => invoke<MainView>('ready'),
  view: () => invoke<MainView>('view'),
  menu: (id: string) => invoke('menu_action', { id }),
  filter: (filter: TreeFilter) => invoke('ui_filter', { filter }),
  select: (ids: number[], current: number | null) => invoke('ui_select', { ids, current }),
  check: (id: number, on: boolean) => invoke('ui_check', { id, on }),
  toggleAll: () => invoke('ui_toggle_all'),
  clearChecks: (ids?: number[]) => invoke('ui_clear_checks', { ids: ids ?? null }),
  headerClick: (key: string) => invoke('ui_header_click', { key }),
  columnWidth: (key: string, width: number) => invoke('ui_column_width', { key, width }),
  paneSizes: (sidebar: number | null, details: number | null) => invoke('ui_pane_sizes', { sidebar, details }),
  search: (query: string) => invoke('ui_search', { query }),
  searchClose: () => invoke('ui_search_close'),
  searchFlags: (flags: number, query?: string) => invoke('ui_search_flags', { flags, query: query ?? null }),
  findNext: () => invoke<number | null>('ui_find_next'),
  targets: () => invoke<{ ids: number[]; completed: number }>('targets_info'),
  remove: (ids: number[], deleteFiles: boolean) => invoke('act_remove', { ids, deleteFiles }),
  deleteCompleted: () => invoke<number>('act_delete_completed'),
  activate: (id: number) => invoke('act_activate', { id }),
  detailsButton: (id: number, action: string) => invoke('act_details_button', { id, action }),
  openWindow: (id: number) => invoke('act_open_window', { id }),
  downloadLimit: (id: number, kbps: number) => invoke('act_download_limit', { id, kbps }),
  open: (id: number) => invoke('act_open', { id }),
  openFolder: (id: number) => invoke('act_open_folder', { id }),
  openWith: (id: number, command: string) => invoke('act_open_with', { id, command }),
  setQueue: (ids: number[], name: string) => invoke('act_set_queue', { ids, name }),
  queue: (name: string, start: boolean) => invoke('act_queue', { name, start }),
  addMany: (items: [string, string, string][]) => invoke<number>('act_add_many', { items }),
  startMainQueue: () => invoke('act_start_main_queue'),
  copy: (text: string) => invoke('copy_text', { text }),
  clipboard: () => invoke<string>('clipboard_text'),
  exportList: (path: string, kind: string) => invoke('export_list', { path, kind }),
  importList: (path: string, kind: string) => invoke<[string, string, string][]>('import_list', { path, kind }),
  setFont: (family: string, size: number) => invoke('set_list_font', { family, size }),
  fonts: () => invoke<string[]>('system_fonts'),
  closeChoice: (choice: 'keep' | 'background' | 'stop') => invoke('close_choice', { choice }),
  shutdownNow: () => invoke('shutdown_now'),
  menuOffer: (add: boolean) => invoke('menu_offer_answer', { add }),
  recheckBrowser: () => invoke('recheck_browser'),
  downloadUrl: (id: number) => invoke<string>('download_url', { id }),
  deleteCategory: (name: string) => invoke('delete_category', { name }),
  appInfo: () => invoke<{ name: string; long_name: string; version: string; mono_family: string }>('app_info'),
}

export function openDialog(label: string, route: string, title: string, width = 560, height = 480, modal = true) {
  return invoke('open_dialog', { label, route, title, width, height, modal })
}
