<script lang="ts">
  // The toolbar (1.x `_build_toolbar`): Add URL | Start · Stop | Delete ▾ | "N checked ✕" | Options · Scheduler · Grabber.
  import type { Actions } from '../lib/types'
  import Icon from './Icon.svelte'
  import Menu from './Menu.svelte'
  import type { ThemeName } from '../theme/tokens'

  let { actions, labels, theme, onpick, onclear }: {
    actions: Actions; labels: boolean; theme: ThemeName; onpick: (id: string) => void; onclear: () => void
  } = $props()
  let deleteMenu = $state<{ x: number; y: number } | null>(null)

  function openDelete(e: MouseEvent) {
    e.stopPropagation()
    const r = (e.currentTarget as HTMLElement).closest('button')!.getBoundingClientRect()
    deleteMenu = { x: r.left, y: r.bottom + 2 }
  }
</script>

<div class="toolbar">
  <button class="tb primary" title="Add new download address manually" onclick={() => onpick('add')}>
    <Icon name="add-white" size={17} {theme} />Add URL
  </button>
  <span class="sep"></span>
  <button class="tb pill" disabled={!actions.resume} title="Start or resume the checked (or selected) downloads" onclick={() => onpick('resume')}>
    <Icon name="resume" size={17} {theme} />{#if labels}Start{/if}
  </button>
  <button class="tb pill" disabled={!actions.stop} title="Stop the checked (or selected) downloads" onclick={() => onpick('stop')}>
    <Icon name="stop" size={17} {theme} />{#if labels}Stop{/if}
  </button>
  <span class="sep"></span>
  <button class="tb danger" disabled={!actions.remove} title="Remove the checked (or selected) files from the list" onclick={() => onpick('remove')}>
    <Icon name="delete" size={17} {theme} />{#if labels}Delete{/if}
    <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
    <span class="split" role="button" tabindex="-1" onclick={openDelete}><svg width="11" height="11" viewBox="0 0 24 24"><path d="M6 9l6 6 6-6" fill="none" stroke="var(--fa)" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round"/></svg></span>
  </button>
  {#if actions.checked > 0}
    <span class="sep"></span>
    <span class="chip">
      <span class="box partial" style="width:14px;height:14px"><svg width="10" height="10" viewBox="0 0 24 24"><path d="M7 12h10" stroke="#fff" stroke-width="3" stroke-linecap="round"/></svg></span>
      {actions.checked} checked
      <button class="x" title="Clear the checks" onclick={onclear}><svg width="12" height="12" viewBox="0 0 24 24"><path d="M6 6l12 12M18 6L6 18" stroke="currentColor" stroke-width="2.4" stroke-linecap="round"/></svg></button>
    </span>
  {/if}
  <span class="spacer"></span>
  <button class="tb ghost" title="Integration, file types, save folders, connections, proxy and others" onclick={() => onpick('options')}>
    <Icon name="options" size={20} {theme} />{#if labels}Options{/if}
  </button>
  <button class="tb ghost" title="Scheduler and download queues" onclick={() => onpick('scheduler')}>
    <Icon name="scheduler" size={20} {theme} />{#if labels}Scheduler{/if}
  </button>
  <button class="tb ghost" title="Run site grabber" onclick={() => onpick('grabber')}>
    <Icon name="grabber" size={20} {theme} />{#if labels}Grabber{/if}
  </button>
</div>
{#if deleteMenu}
  <!-- svelte-ignore a11y_no_static_element_interactions a11y_click_events_have_key_events -->
  <div style="position:fixed;inset:0;z-index:999" onmousedown={() => (deleteMenu = null)}></div>
  <Menu
    items={[
      { type: 'action', id: 'remove', label: 'Delete checked or selected', icon: 'delete', enabled: actions.remove },
      { type: 'action', id: 'delete_completed', label: 'Delete all completed', icon: 'delete-completed', enabled: actions.delete_completed },
    ]}
    x={deleteMenu.x} y={deleteMenu.y} {theme}
    onpick={(id) => { deleteMenu = null; onpick(id) }}
    onclose={() => (deleteMenu = null)}
  />
{/if}
