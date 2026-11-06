<script lang="ts">
  // "Find file" and "LID Search settings" (1.x find.FindDialog).
  import Shell from '../../components/form/Shell.svelte'
  import Icon from '../../components/Icon.svelte'
  import Dialog, { type Box } from '../../components/Dialog.svelte'
  import { call, close, followTheme, fit, params, setTitle } from '../../lib/dialog'
  import type { ThemeName } from '../../theme/tokens'

  const showQuery = params().get('next') === '1'
  let theme = $state<ThemeName>('ember')
  let query = $state('')
  let flags = $state(4)
  let box = $state<Box | null>(null)
  let field: HTMLInputElement | undefined = $state()
  followTheme((t) => (theme = t))
  setTitle(showQuery ? 'Find file' : 'LID Search settings')
  call<any>('settings_get').then((s) => {
    query = s.search_string
    flags = s.search_flags & 0x1f
    fit(460)
    queueMicrotask(() => { field?.focus(); field?.select() })
  })

  const bit = (b: number) => (flags & b) !== 0
  const set = (b: number, on: boolean) => (flags = on ? flags | b : flags & ~b)

  async function ok() {
    if ((flags & 0x1c) === 0) {
      set(0x04, true)
      await new Promise<void>((resolve) => {
        box = { title: 'Search settings', text: 'The checkbox for at least one of the search targets must be enabled', icon: 'info',
          buttons: [{ id: 'ok', label: 'OK', primary: true }], resolve: () => { box = null; resolve() } }
      })
      return
    }
    await call('ui_search_flags', { flags, query: showQuery ? query : null })
    if (showQuery) {
      const id = await call<number | null>('ui_find_next').catch(() => null)
      if (id === null) {
        await new Promise<void>((resolve) => {
          box = { title: 'Find file', text: 'No such files found', icon: 'info', buttons: [{ id: 'ok', label: 'OK', primary: true }],
            resolve: () => { box = null; resolve() } }
        })
        return
      }
    }
    close()
  }
</script>

<svelte:window onkeydown={(e) => { if (!box && e.key === 'Enter') ok(); if (!box && e.key === 'Escape') close() }} />
<Shell {theme}>
  {#if showQuery}
    <div class="caption">Find</div>
    <div style="position:relative">
      <span style="position:absolute;left:8px;top:7px"><Icon name="search" size={16} {theme} /></span>
      <input class="input" style="width:100%;padding-left:30px" bind:this={field} bind:value={query} />
    </div>
  {/if}
  <div class="panel">
    <div class="caption">Look in</div>
    <label class="check"><input type="checkbox" checked={bit(4)} onchange={(e) => set(4, e.currentTarget.checked)} /> File name or part of the name</label>
    <label class="check"><input type="checkbox" checked={bit(8)} onchange={(e) => set(8, e.currentTarget.checked)} /> Description or part of the description</label>
    <label class="check"><input type="checkbox" checked={bit(16)} onchange={(e) => set(16, e.currentTarget.checked)} /> Site name / download link / parent web page / referer</label>
  </div>
  <div style="display:flex;gap:18px">
    <label class="check"><input type="checkbox" checked={bit(1)} onchange={(e) => set(1, e.currentTarget.checked)} /> Match case</label>
    <label class="check"><input type="checkbox" checked={bit(2)} onchange={(e) => set(2, e.currentTarget.checked)} /> Match whole string only</label>
  </div>
  {#snippet footer()}
    {#if showQuery}<span class="faint">Ctrl+F · F3 finds next</span>{/if}
    <span class="grow"></span>
    <button class="btn" onclick={close}>Cancel</button>
    <button class="btn primary" onclick={ok}>{showQuery ? 'Find' : 'OK'}</button>
  {/snippet}
</Shell>
{#if box}<Dialog {box} {theme} />{/if}
