<script lang="ts">
  import type { Component } from 'svelte'
  import { invoke } from '@tauri-apps/api/core'
  import { listen } from '@tauri-apps/api/event'
  import { untrack } from 'svelte'
  import { installMnemonics, mark } from './lib/dialog'
  import Main from './windows/Main.svelte'
  import Pending from './windows/Pending.svelte'
  import Add from './windows/dialogs/Add.svelte'
  import Batch from './windows/dialogs/Batch.svelte'
  import Links from './windows/dialogs/Links.svelte'
  import Limiter from './windows/dialogs/Limiter.svelte'
  import Find from './windows/dialogs/Find.svelte'
  import Columns from './windows/dialogs/Columns.svelte'
  import Category from './windows/dialogs/Category.svelte'
  import Font from './windows/dialogs/Font.svelte'
  import Progress from './windows/dialogs/Progress.svelte'
  import Complete from './windows/dialogs/Complete.svelte'
  import Mismatch from './windows/dialogs/Mismatch.svelte'
  import CheckHash from './windows/dialogs/CheckHash.svelte'
  import Properties from './windows/dialogs/Properties.svelte'
  import Options from './windows/dialogs/Options.svelte'
  import Scheduler from './windows/dialogs/Scheduler.svelte'
  import Browsers from './windows/dialogs/Browsers.svelte'

  // One bundle for every window; the route after '#' picks the page. A spare window (windows.rs) loads
  // with #/spare and gets its route later, when a dialog takes it over.
  let route = $state(location.hash.replace(/^#\/?/, '') || 'main')
  const page = $derived(route.split('?')[0])
  const PAGES: Record<string, Component<any>> = {
    add: Add, batch: Batch, links: Links, limiter: Limiter, find: Find, columns: Columns, category: Category, font: Font,
    progress: Progress, complete: Complete, mismatch: Mismatch, checkhash: CheckHash, properties: Properties,
    options: Options, scheduler: Scheduler, browsers: Browsers,
  }
  const Page = $derived(PAGES[page])

  /** A dialog page starts: mnemonics, timing, the UI scenario, and showing the (hidden) window. */
  function startDialog(name: string) {
    mark('script')
    requestAnimationFrame(() => mark('painted'))
    installMnemonics()
    // Pages that size themselves call fit(), which shows the window; the others are shown here.
    setTimeout(() => invoke('dialog_ready').catch(() => {}), 250)
    invoke<string | null>('e2e_script').then((s) => {
      if (s === 'dialogs') import('./dev/e2e-dialogs').then((m) => { m.watchErrors(); m.runDialog(name) })
    })
  }

  if (untrack(() => route) === 'spare') {
    const off = listen<string>('lid://route', (e) => {
      off.then((f) => f()) // a spare is taken over once
      history.replaceState(null, '', `#/${e.payload}`)
      route = e.payload
      startDialog(e.payload.split('?')[0])
    })
    off.then(() => invoke('spare_ready').catch(() => {}))
  } else if (untrack(() => page) !== 'main') {
    startDialog(untrack(() => page))
  }
</script>

{#if page === 'main'}
  <Main />
{:else if page === 'spare'}
  <!-- a loaded window waiting for its dialog -->
{:else if Page}
  {#key route}<Page />{/key}
{:else}
  <Pending {route} />
{/if}
