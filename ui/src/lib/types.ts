// Shapes sent by the backend (src-tauri/src/app.rs, crates/lider-core/src/view.rs).
export type NodeKind = 'all' | 'unfinished' | 'finished' | 'grabber' | 'queues' | 'queue'
export interface TreeFilter { kind: NodeKind; value: string | null }
export interface TreeNode { title: string; icon: string; filter: TreeFilter; count: number; expanded: boolean; children: TreeNode[] }
export interface Pill { kind: 'run' | 'pause' | 'done' | 'verified' | 'mismatch' | 'error' | 'queue'; progress: number; text: string }
export interface Row { id: number; checked: boolean; cells: string[]; icon: string; pill: Pill; error: string; queue: string }
export interface Column { key: string; title: string; width: number; min: number; mono: boolean }
export interface Actions { resume: boolean; stop: boolean; remove: boolean; redownload: boolean; stop_all: boolean; delete_completed: boolean; checked: number }
export interface StatusBar { speed: string; counts: string; limiter: string; clipboard: string }
export interface Segment { start: number; end: number; pos: number }
export interface Details {
  id: number; filename: string; icon: string; url: string; url_full: string; state: string
  action: string; action_label: string; action_icon: string; percent: string; of: string; bar: number; caption: string
  size: number; segments: Segment[]; rate: string; time_left: string; done: string; connections: string; resume: string
  checksum: string; checksum_tone: string; rows: { n: string; downloaded: string; speed: string; info: string }[]
  limit_on: boolean; limit_kbps: number
}
export interface Appearance {
  theme: 'ember' | 'ember-dark' | 'dark'; density: string; toolbar_labels: boolean; show_categories: boolean
  show_details: boolean; show_statusbar: boolean; inline_menubar: boolean; list_font: [string, number] | null
  sidebar_width: number; details_height: number; sort_key: string; sort_desc: boolean
}
export type MenuItem =
  | { type: 'action'; id: string; label: string; shortcut?: string; icon?: string; tip?: string; enabled: boolean; checked?: boolean }
  | { type: 'separator' }
  | { type: 'submenu'; label: string; items: MenuItem[] }
export interface Menu { label: string; items: MenuItem[] }
export interface MainView {
  title: string; columns: Column[]; rows: Row[]; check_state: 'unchecked' | 'partial' | 'checked'; tree: TreeNode[]
  filter: TreeFilter; actions: Actions; status: StatusBar; browser: string; details: Details | null
  appearance: Appearance; search: { query: string; flags: number; active: boolean }; selected: number[]
  current: number | null; queues: string[]; menus?: Menu[]
}
export type UiRequest =
  | { kind: 'add_url'; url: string; referer: string; parent_page: string; headers: Record<string, string>; filename: string; ask_address: boolean }
  | { kind: 'links'; items: [string, string, string][]; referer: string; parent_page: string; headers: Record<string, string> }
  | { kind: 'complete'; id: number }
  | { kind: 'mismatch'; id: number }
  | { kind: 'shutdown' }
  | { kind: 'confirm_close'; running: [string, string, string][]; background: boolean; exiting: boolean }
  | { kind: 'offer_menu' }
