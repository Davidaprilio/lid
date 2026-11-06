<script lang="ts">
  import type { Component } from 'svelte'
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

  // One bundle for every window; the route after '#' picks the page.
  const route = location.hash.replace(/^#\/?/, '') || 'main'
  const page = route.split('?')[0]
  const PAGES: Record<string, Component<any>> = {
    add: Add, batch: Batch, links: Links, limiter: Limiter, find: Find, columns: Columns, category: Category, font: Font,
  }
  const Page = PAGES[page]
</script>

{#if page === 'main'}
  <Main />
{:else if Page}
  <Page />
{:else}
  <Pending {route} />
{/if}
