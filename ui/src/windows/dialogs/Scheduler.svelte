<script lang="ts">
  // "Scheduler" (1.x scheduler.SchedulerDialog): queues on the left, the schedule and the files of the
  // selected queue on the right. Edits are always kept (switching queue, Start now, Apply, Close).
  import { open as openFile } from '@tauri-apps/plugin-dialog'
  import TabStrip from '../../components/form/TabStrip.svelte'
  import Row from '../../components/form/Row.svelte'
  import Pill from '../../components/form/Pill.svelte'
  import Icon from '../../components/Icon.svelte'
  import Dialog, { type Box } from '../../components/Dialog.svelte'
  import { call, close, followTheme, params } from '../../lib/dialog'
  import type { ThemeName } from '../../theme/tokens'

  const DAYS = ['Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday', 'Sunday']
  const MAIN = 'Main download queue'
  let theme = $state<ThemeName>('ember')
  let queue = $state(params().get('queue') || MAIN)
  let data = $state<any>(null)
  let sch = $state<any>(null)
  let tab = $state(0)
  let openOn = $state(false)
  let date = $state('')
  let nextRun = $state<string | null>(null)
  let selItem = $state<number | null>(null)
  let box = $state<Box | null>(null)

  followTheme((t) => (theme = t))
  load(params().get('queue') || MAIN)

  async function load(name: string) {
    queue = name
    data = await call('scheduler_data', { queue: name })
    sch = data.schedule
    openOn = !!sch.open_when_done
    date = sch.once_date || data.today
  }

  async function refreshList() {
    const d = await call<any>('scheduler_data', { queue })
    data = { ...data, items: d.items, queues: d.queues, running: d.running }
  }

  // The schedule as the tab shows it (once_date is "" when Daily is chosen).
  function stored() {
    return { ...sch, once_date: sch.daily ? '' : date, open_when_done: openOn ? sch.open_when_done : '', max_parallel: Math.max(1, Math.min(64, Math.round(sch.max_parallel) || 1)) }
  }

  async function apply() {
    if (sch) await call('scheduler_apply', { queue, schedule: stored() })
  }

  $effect(() => {
    if (!sch) return
    const s = stored()
    call<string | null>('next_run', { schedule: s }).then((t) => (nextRun = t))
  })

  async function pick(name: string) {
    await apply()
    await load(name)
  }

  function ask(b: Omit<Box, 'resolve'>): Promise<{ button: string; value: string }> {
    return new Promise((resolve) => { box = { ...b, resolve: (r) => { box = null; resolve(r) } } })
  }

  async function newQueue() {
    const r = await ask({ title: 'Enter queue name', text: 'Enter a name for the new queue that will be displayed in the list of queues', input: { label: '', value: '' },
      buttons: [{ id: 'ok', label: 'OK', primary: true }, { id: 'cancel', label: 'Cancel' }] })
    if (r.button !== 'ok' || !r.value.trim()) return
    await apply()
    if (await call('queue_new', { name: r.value.trim() })) await load(r.value.trim())
  }

  async function deleteQueue() {
    if (queue === MAIN || queue === 'Synchronization queue') {
      await ask({ title: 'LID', text: 'The default queues cannot be deleted.', icon: 'info', buttons: [{ id: 'ok', label: 'OK', primary: true }] })
      return
    }
    const r = await ask({ title: 'LID', text: `Delete queue "${queue}"?`, icon: 'info', buttons: [{ id: 'yes', label: 'Yes', primary: true }, { id: 'no', label: 'No' }] })
    if (r.button !== 'yes') return
    await call('queue_delete', { name: queue })
    await load(MAIN)
  }

  async function startNow() {
    await apply()
    await call('act_queue', { name: queue, start: true })
    await refreshList()
  }

  async function stopNow() {
    await call('act_queue', { name: queue, start: false })
    await refreshList()
  }

  async function chooseFile() {
    const f = await openFile({ title: 'Open the following file when done', defaultPath: sch.open_when_done || undefined })
    if (typeof f === 'string') { sch.open_when_done = f; openOn = true }
  }

  async function moveItem(d: number) {
    if (selItem === null) return
    await call('queue_move', { id: selItem, delta: d })
    await refreshList()
  }

  async function removeItem() {
    if (selItem === null) return
    await call('act_set_queue', { ids: [selItem], name: '' })
    selItem = null
    await refreshList()
  }
</script>

<svelte:window onkeydown={(e) => { if (!box && e.key === 'Escape') { apply().then(close) } }} />
{#if data && sch}
  <div class="shell">
    <div class="shell-body" style="flex-direction:row;gap:14px;min-height:0">
      <div style="width:210px;flex:none;display:flex;flex-direction:column;gap:8px">
        <div class="caption">Queues</div>
        <div class="list2" style="flex:1">
          {#each data.queues as q}
            <!-- svelte-ignore a11y_click_events_have_key_events -->
            <div class="r" class:sel={q.name === queue} role="option" aria-selected={q.name === queue} tabindex="-1" onclick={() => pick(q.name)}>
              <Icon name={q.icon} size={16} {theme} /><span style="flex:1;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">{q.name}</span>
              {#if q.count}<span class="badge">{q.count}</span>{/if}
            </div>
          {/each}
        </div>
        <button class="btn" onclick={newQueue}>New queue</button>
        <button class="btn" onclick={deleteQueue}>Delete</button>
      </div>
      <div style="flex:1;display:flex;flex-direction:column;gap:8px;min-width:0">
        <div style="display:flex;gap:10px;align-items:center">
          <b style="font-size:1.17em">{queue}</b>
          <Pill text={data.running ? 'Running' : 'Stopped'} tone={data.running ? 'accent' : 'muted'} />
        </div>
        <div class="panel" style="padding:0;gap:0;flex:1">
          <TabStrip names={['Schedule', 'Files in the queue']} bind:active={tab} />
          <div style="padding:12px 14px;display:flex;flex-direction:column;gap:8px">
            {#if tab === 0}
              <div style="display:flex;gap:18px">
                <label class="radio"><input type="radio" checked={!sch.periodic} onchange={() => (sch.periodic = false)} /> One-time downloading</label>
                <label class="radio"><input type="radio" checked={sch.periodic} onchange={() => (sch.periodic = true)} /> Periodic synchronization</label>
              </div>
              <label class="check"><input type="checkbox" bind:checked={sch.start_on_startup} /> Start download on LID startup</label>
              <div style="display:flex;align-items:center;gap:8px">
                <label class="check" style="flex:1"><input type="checkbox" bind:checked={sch.start_enabled} /> Start download at</label>
                <input class="input mono" type="time" bind:value={sch.start_time} disabled={!sch.start_enabled} />
              </div>
              <div style="padding-left:24px;display:flex;flex-direction:column;gap:6px">
                <div style="display:flex;align-items:center;gap:8px">
                  <label class="radio"><input type="radio" checked={!sch.daily} disabled={!sch.start_enabled} onchange={() => (sch.daily = false)} /> Once at</label>
                  <input class="input mono" type="date" bind:value={date} disabled={!sch.start_enabled || sch.daily} />
                </div>
                <label class="radio"><input type="radio" checked={sch.daily} disabled={!sch.start_enabled} onchange={() => (sch.daily = true)} /> Daily</label>
                <div style="display:grid;grid-template-columns:repeat(3, 1fr);gap:4px 12px;padding-left:24px">
                  {#each DAYS as d, i}
                    <label class="check"><input type="checkbox" disabled={!sch.start_enabled || !sch.daily} checked={sch.days.includes(i)}
                      onchange={(e) => (sch.days = e.currentTarget.checked ? [...sch.days, i].sort() : sch.days.filter((x: number) => x !== i))} /> {d}</label>
                  {/each}
                </div>
              </div>
              {#if sch.periodic}
                <Row label="Start again every" width={140}>
                  <input class="input mono" type="number" min="0" max="999" style="width:80px" bind:value={sch.every_hours} /><span>hours</span>
                  <input class="input mono" type="number" min="0" max="59" style="width:80px" bind:value={sch.every_minutes} /><span>minutes</span>
                </Row>
              {/if}
              <div style="display:flex;align-items:center;gap:8px">
                <label class="check" style="flex:1"><input type="checkbox" bind:checked={sch.stop_enabled} /> Stop download at</label>
                <input class="input mono" type="time" bind:value={sch.stop_time} />
              </div>
              <div style="display:flex;align-items:center;gap:8px">
                <label class="check" style="flex:1"><input type="checkbox" bind:checked={sch.retries_enabled} /> Number of retries for each file if downloading failed</label>
                <input class="input mono" type="number" min="0" max="999" style="width:80px" bind:value={sch.retries} />
              </div>
              <label class="check"><input type="checkbox" bind:checked={openOn} /> Open the following file when done:</label>
              {#if openOn}
                <div style="display:flex;gap:8px;padding-left:24px"><input class="input mono" style="flex:1" bind:value={sch.open_when_done} /><button class="btn small" style="min-width:36px" onclick={chooseFile}>...</button></div>
              {/if}
              <label class="check"><input type="checkbox" bind:checked={sch.exit_when_done} /> Exit LID when done</label>
              <label class="check"><input type="checkbox" bind:checked={sch.shutdown_when_done} /> Turn off computer when done</label>
              {#if nextRun}<div class="accentBox"><Icon name="clock" size={16} {theme} />{nextRun}</div>{/if}
            {:else}
              <Row label="Download" width={70}><input class="input mono" type="number" min="1" max="64" style="width:80px" bind:value={sch.max_parallel} /><span>files at the same time</span></Row>
              <div style="display:flex;gap:8px">
                <div class="list2" style="flex:1;height:300px">
                  <div class="h"><span style="flex:1">FILE NAME</span><span style="width:90px">SIZE</span><span style="width:110px">STATUS</span></div>
                  {#each data.items as it (it.id)}
                    <!-- svelte-ignore a11y_click_events_have_key_events -->
                    <div class="r" class:sel={selItem === it.id} role="option" aria-selected={selItem === it.id} tabindex="-1" onclick={() => (selItem = it.id)}>
                      <span style="flex:1;display:flex;gap:6px;overflow:hidden"><Icon name={it.icon} size={16} {theme} /><span style="overflow:hidden;text-overflow:ellipsis;white-space:nowrap">{it.name}</span></span>
                      <span class="mono" style="width:90px">{it.size}</span>
                      <span style="width:110px">{#if it.status === 'Queued'}<span class="tpill muted">Queued</span>{:else}{it.status}{/if}</span>
                    </div>
                  {/each}
                </div>
                <div style="display:flex;flex-direction:column;gap:6px">
                  <button class="roundBtn" title="Move up" onclick={() => moveItem(-1)}><Icon name="up" size={14} {theme} /></button>
                  <button class="roundBtn" title="Move down" onclick={() => moveItem(1)}><Icon name="down-arrow" size={14} {theme} /></button>
                  <button class="roundBtn" title="Delete from queue" onclick={removeItem}><Icon name="remove" size={14} {theme} /></button>
                </div>
              </div>
            {/if}
          </div>
        </div>
      </div>
    </div>
    <div class="shell-footer">
      <button class="btn primary" onclick={startNow}><Icon name="play-white" size={14} {theme} /> Start now</button>
      <button class="btn" onclick={stopNow}>Stop</button>
      <span class="grow"></span>
      <button class="btn" onclick={apply}>Apply</button>
      <button class="btn" onclick={async () => { await apply(); close() }}>Close</button>
    </div>
  </div>
{/if}
{#if box}<Dialog {box} {theme} />{/if}
