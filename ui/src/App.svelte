<script lang="ts">
  import type { Component } from 'svelte'
  import { invoke } from '@tauri-apps/api/core'
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

  // One bundle for every window; the route after '#' picks the page.
  const route = location.hash.replace(/^#\/?/, '') || 'main'
  const page = route.split('?')[0]
  const PAGES: Record<string, Component<any>> = {
    add: Add, batch: Batch, links: Links, limiter: Limiter, find: Find, columns: Columns, category: Category, font: Font,
    progress: Progress, complete: Complete, mismatch: Mismatch, checkhash: CheckHash, properties: Properties,
    options: Options, scheduler: Scheduler, browsers: Browsers,
  }
  const Page = PAGES[page]
  if (page !== 'main') {
    invoke<string | null>('e2e_script').then((name) => {
      if (name === 'dialogs') import('./dev/e2e-dialogs').then((m) => { m.watchErrors(); m.runDialog(page) })
    })
  }
</script>

{#if page === 'main'}
  <Main />
{:else if Page}
  <Page />
{:else}
  <Pending {route} />
{/if}
