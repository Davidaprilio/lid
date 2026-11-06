// Helpers of the dialog windows: theme, route parameters, closing and sizing the window.
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { LogicalSize } from '@tauri-apps/api/dpi'
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow'
import { applyTheme, type ThemeName } from '../theme/tokens'

export const win = () => getCurrentWebviewWindow()

/** The query of the route: `#/add?url=…` → url=… */
export function params(): URLSearchParams {
  const hash = location.hash
  const i = hash.indexOf('?')
  return new URLSearchParams(i >= 0 ? hash.slice(i + 1) : '')
}

/** Follow the app's theme; returns the current one. */
export async function followTheme(set: (t: ThemeName) => void): Promise<void> {
  const t = await invoke<ThemeName>('dialog_theme')
  applyTheme(t)
  set(t)
  listen<ThemeName>('lid://theme', (e) => {
    applyTheme(e.payload)
    set(e.payload)
  })
}

export function close() {
  return win().close()
}

/** Make the window as tall as its content (dialogs grow and shrink as parts show and hide). */
export async function fit(width?: number) {
  await new Promise((r) => requestAnimationFrame(() => r(null)))
  const el = document.querySelector<HTMLElement>('.shell') ?? document.body
  const w = width ?? Math.max(el.scrollWidth, window.innerWidth)
  await win().setSize(new LogicalSize(w, el.scrollHeight))
}

export function setTitle(title: string) {
  return win().setTitle(title)
}

export const URL_RE = /^(https?|ftp):\/\/\S+$/i

export const call = invoke
