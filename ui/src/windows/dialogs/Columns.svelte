<script lang="ts">
  // Customize columns (1.x options.ColumnsDialog): visible, order, width; Reset writes the defaults.
  import Shell from '../../components/form/Shell.svelte'
  import Row from '../../components/form/Row.svelte'
  import { call, close, followTheme, fit } from '../../lib/dialog'
  import type { ThemeName } from '../../theme/tokens'

  const TITLES: Record<string, string> = {
    queue: 'Q', filename: 'File Name', size: 'Size', status: 'Status', time_left: 'Time left', rate: 'Transfer rate',
    last_try: 'Last Try Date', description: 'Description', date_added: 'Date Added', save_to: 'Save To', referer: 'Referer',
    parent_page: 'Parent Web Page',
  }
  let theme = $state<ThemeName>('ember')
  let cols = $state<[string, boolean, number][]>([])
  let current = $state(0)
  let dragFrom: number | null = null
  followTheme((t) => (theme = t))
  call<any>('settings_get').then((s) => { cols = s.columns; fit(440) })

  function move(d: number) {
    const j = current + d
    if (j < 0 || j >= cols.length) return
    const c = [...cols]
    ;[c[current], c[j]] = [c[j], c[current]]
    cols = c
    current = j
  }

  function drop(i: number) {
    if (dragFrom === null || dragFrom === i) return
    const c = [...cols]
    const [x] = c.splice(dragFrom, 1)
    c.splice(i, 0, x)
    cols = c
    current = i
    dragFrom = null
  }

  async function ok() {
    await call('columns_apply', { columns: cols })
    close()
  }

  async function reset() {
    await call('columns_apply', { columns: null })
    close()
  }
</script>

<Shell {theme} keys>
  <div class="muted">Check the columns to show. Drag or use the buttons to change the order.</div>
  <div style="display:flex;gap:10px">
    <div class="list2" style="flex:1;height:{12 * 30 + 4}px">
      {#each cols as c, i (c[0])}
        <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
        <div class="r" role="option" aria-selected={current === i} tabindex="-1" class:sel={current === i} style="height:30px" draggable="true"
          ondragstart={() => (dragFrom = i)} ondragover={(e) => e.preventDefault()} ondrop={() => drop(i)} onclick={() => (current = i)}>
          <input type="checkbox" bind:checked={c[1]} />
          {TITLES[c[0]] ?? c[0]}
        </div>
      {/each}
    </div>
    <div style="display:flex;flex-direction:column;gap:6px">
      <button class="btn" onclick={() => move(-1)}>Move up</button>
      <button class="btn" onclick={() => move(1)}>Move down</button>
      <span style="flex:1"></span>
      <button class="ghostBtn" title="Back to the default columns" onclick={reset}>Reset</button>
    </div>
  </div>
  {#if cols[current]}
    <Row label="Selected column width" width={150}>
      <input class="input mono" type="number" min="10" max="2000" style="width:100px" bind:value={cols[current][2]} />
      <span class="formLabel">px</span>
    </Row>
  {/if}
  {#snippet footer()}
    <span class="grow"></span>
    <button class="btn" onclick={close}>Cancel</button>
    <button class="btn primary" onclick={ok}>OK</button>
  {/snippet}
</Shell>
