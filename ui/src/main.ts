import { mount } from 'svelte'
import './theme/base.css'
import App from './App.svelte'

if (!('__TAURI_INTERNALS__' in window)) await import('./dev/mock')

// One bundle for every window; the route after '#' picks what to show (main, add, options, …).
const app = mount(App, { target: document.getElementById('app')! })

export default app
