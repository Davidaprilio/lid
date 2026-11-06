import { mount } from 'svelte'

// Script errors from the very start, for the UI scenarios (ui/src/dev/e2e-dialogs.ts).
const w = window as unknown as { __lidErrors: string[] }
w.__lidErrors = []
window.addEventListener('error', (e) => w.__lidErrors.push(`script error: ${e.message} at ${e.filename}:${e.lineno}`))
window.addEventListener('unhandledrejection', (e) => w.__lidErrors.push(`unhandled: ${String(e.reason)}`))
import './theme/base.css'
import App from './App.svelte'

if (!('__TAURI_INTERNALS__' in window)) await import('./dev/mock')

// One bundle for every window; the route after '#' picks what to show (main, add, options, …).
const app = mount(App, { target: document.getElementById('app')! })

export default app
