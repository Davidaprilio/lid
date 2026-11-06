import { mount } from 'svelte'
import App from './App.svelte'

// One bundle for every window; the route after '#' picks what to show (main, add, options, …).
const app = mount(App, { target: document.getElementById('app')! })

export default app
