<script lang="ts">
  // Speed Limiter settings (1.x options.SpeedLimiterDialog).
  import Shell from '../../components/form/Shell.svelte'
  import Row from '../../components/form/Row.svelte'
  import { call, close, followTheme, fit } from '../../lib/dialog'
  import type { ThemeName } from '../../theme/tokens'

  let theme = $state<ThemeName>('ember')
  let kbps = $state(200)
  let onStartup = $state(false)
  followTheme((t) => (theme = t))
  call<any>('settings_get').then((s) => { kbps = s.speed_limit_kbps; onStartup = s.speed_limiter_on_startup; fit(520) })

  async function ok() {
    await call('limiter_apply', { kbps: Math.round(kbps), onStartup })
    close()
  }
</script>

<Shell {theme} icon="speed" badge="accent" title="Speed Limiter settings" subtitle="Applies to all downloads while the limiter is on">
  <Row label="Maximum speed" width={110}>
    <input class="input mono" type="number" min="1" max="10000000" style="width:120px" bind:value={kbps} />
    <span class="formLabel">KBytes/sec</span>
  </Row>
  <div class="noteBox">Note: servers may close the connection when the speed is very low.</div>
  <label class="check"><input type="checkbox" bind:checked={onStartup} /> Always turn on Speed Limiter on LID startup</label>
  {#snippet footer()}
    <span class="grow"></span>
    <button class="btn" onclick={close}>Cancel</button>
    <button class="btn primary" onclick={ok}>OK</button>
  {/snippet}
</Shell>
