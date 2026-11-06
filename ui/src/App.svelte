<script lang="ts">
  import { invoke } from '@tauri-apps/api/core'

  type AppInfo = { name: string; long_name: string; version: string }

  const route = location.hash.replace(/^#\/?/, '') || 'main'
  let info = $state<AppInfo | null>(null)

  invoke<AppInfo>('app_info').then((i) => (info = i)).catch(() => (info = null))
</script>

<main>
  <h1>{info ? `${info.long_name} ${info.version}` : 'LID'}</h1>
  <p>Window: {route}. The port is in progress; see docs/tauri/PLAN.md.</p>
</main>

<style>
  main {
    font-family: system-ui, sans-serif;
    padding: 24px;
  }
</style>
