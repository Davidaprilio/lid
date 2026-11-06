<script lang="ts">
  import Main from './windows/Main.svelte'
  import Pending from './windows/Pending.svelte'
  import { applyTheme } from './theme/tokens'

  // One bundle for every window; the route after '#' picks the page.
  const route = location.hash.replace(/^#\/?/, '') || 'main'
  const page = route.split('?')[0]
  if (page !== 'main') applyTheme('ember')
</script>

{#if page === 'main'}
  <Main />
{:else}
  <Pending {route} />
{/if}
