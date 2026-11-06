<script lang="ts">
  // Values as removable chips (1.x widgets.TagInput): Enter or "," adds, Backspace on empty removes
  // the last one, pasted text is split on commas and spaces.
  let { values = $bindable([]), placeholder = '', kind = 'extension', noun = 'types', disabled = false }: {
    values?: string[]; placeholder?: string; kind?: 'extension' | 'site'; noun?: string; disabled?: boolean
  } = $props()
  let text = $state('')
  let focused = $state(false)
  let field: HTMLInputElement | undefined = $state()

  const norm = (v: string) => (kind === 'extension' ? v.trim().replace(/^\.+/, '').toLowerCase() : v.trim().toLowerCase())

  export function commit() {
    const parts = text.split(/[,\s]+/).map(norm).filter(Boolean)
    if (parts.length) {
      const next = [...values]
      for (const p of parts) if (!next.includes(p)) next.push(p)
      values = next
    }
    text = ''
  }

  function key(e: KeyboardEvent) {
    if (e.key === 'Enter' || e.key === ',') { e.preventDefault(); e.stopPropagation(); commit() }
    else if (e.key === 'Backspace' && !text && values.length) values = values.slice(0, -1)
  }
</script>

<!-- Clicking anywhere in the frame focuses the field (a label does that). -->
<label class="tagInput" class:focused class:disabled>
  {#each values as v}
    <button class="tag" title="Remove {v}" {disabled} onclick={(e) => { e.stopPropagation(); values = values.filter((x) => x !== v) }}>
      {v}<svg width="11" height="11" viewBox="0 0 24 24"><path d="M6 6l12 12M18 6L6 18" stroke="currentColor" stroke-width="2.4" stroke-linecap="round"/></svg>
    </button>
  {/each}
  <input bind:this={field} bind:value={text} {placeholder} {disabled} style:min-width="{Math.max(120, placeholder.length * 7 + 24)}px"
    onkeydown={key} onfocus={() => (focused = true)} onblur={() => { focused = false; commit() }}
    onpaste={(e) => { e.preventDefault(); text += e.clipboardData?.getData('text') ?? ''; commit() }} />
</label>
<div class="tagmeta faint">
  <span>Enter or , adds · Backspace removes the last</span>
  <span style="flex:1"></span>
  <span>{values.length} {noun}</span>
  <button class="ghostBtn" {disabled} onclick={() => (values = [])}>Clear all</button>
</div>
