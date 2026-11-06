<script lang="ts">
  // A dialog that is ported in phase 6 of docs/tauri/PLAN.md; shows what it was opened with.
  import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow'
  let { route }: { route: string } = $props()
</script>

<div style="padding:24px;display:flex;flex-direction:column;gap:12px;height:100vh">
  <div class="title" style="font-weight:700;font-size:1.25em">{route.split('?')[0]}</div>
  <div class="muted">This dialog comes in phase 6 of the port.</div>
  <pre class="mono faint" style="white-space:pre-wrap;word-break:break-all;flex:1;overflow:auto">{decodeURIComponent(route)}</pre>
  <div style="display:flex;justify-content:flex-end"><button class="btn primary" onclick={() => getCurrentWebviewWindow().close()}>Close</button></div>
</div>
