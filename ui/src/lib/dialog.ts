// Helpers of the dialog windows: theme, route parameters, closing and sizing the window.
import { tick } from 'svelte'
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
  // Svelte's pending updates, not a frame: a hidden window gets no animation frames.
  await tick()
  const el = document.querySelector<HTMLElement>('.shell') ?? document.body
  const w = width ?? Math.max(el.scrollWidth, window.innerWidth)
  // The natural height: .shell fills the window (min-height: 100vh), so measure it without that, or a
  // window could only grow (1.x dialogs take the height of their layout).
  const min = el.style.minHeight
  el.style.minHeight = '0'
  const h = el.scrollHeight
  el.style.minHeight = min
  await win().setSize(new LogicalSize(w, h))
  mark('fitted')
  // The window was hidden until it had its size (windows.rs).
  invoke('dialog_ready').catch(() => {})
}

/** LIDER_TIMING=1: a stage of this dialog's start, logged by the backend with the time since the request. */
export function mark(stage: string) {
  invoke('dialog_mark', { stage }).catch(() => {})
}

export function setTitle(title: string) {
  return win().setTitle(title)
}

export const URL_RE = /^(https?|ftp):\/\/\S+$/i

export const call = invoke

/** Qt mnemonics ("&x"): Alt+x presses the button or check box of the top layer (the window, or the last
 * overlay) whose underlined letter is x. */
export function installMnemonics() {
  // Qt underlines the letters only while Alt is held.
  const root = document.documentElement
  window.addEventListener('keydown', (e) => { if (e.key === 'Alt') root.classList.add('show-mnemonics') })
  window.addEventListener('keyup', (e) => { if (e.key === 'Alt') root.classList.remove('show-mnemonics') })
  window.addEventListener('blur', () => root.classList.remove('show-mnemonics'))
  window.addEventListener('keydown', (e) => {
    if (e.defaultPrevented || !e.altKey || e.ctrlKey || !/^[a-z0-9]$/i.test(e.key)) return
    const overlays = document.querySelectorAll('.overlay')
    const top: ParentNode = overlays.length ? overlays[overlays.length - 1] : document
    const hit = Array.from(top.querySelectorAll<HTMLElement>('button u, label u'))
      .filter((u) => overlays.length || !u.closest('.overlay'))
      .find((u) => u.textContent!.toLowerCase() === e.key.toLowerCase())
      ?.closest<HTMLElement>('button, label')
    if (!hit || (hit as HTMLButtonElement).disabled || hit.querySelector('input:disabled')) return
    e.preventDefault()
    hit.click()
  })
}
