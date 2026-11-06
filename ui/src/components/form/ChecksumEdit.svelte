<script lang="ts">
  // The checksum field (1.x widgets.ChecksumEdit): algorithm, hash, detected-type pill, message line.
  import { invoke } from '@tauri-apps/api/core'

  const HINT = 'Optional — LID checks the file when the download finishes.'
  let { text = $bindable(''), algorithm = $bindable('auto'), hint = HINT, valid = $bindable(true), value = $bindable(''), disabled = false }: {
    text?: string; algorithm?: string; hint?: string; valid?: boolean; value?: string; disabled?: boolean
  } = $props()
  let field: HTMLInputElement | undefined = $state()
  export function focus() { field?.focus() }
  let title = $state('')
  let error = $state('')

  $effect(() => {
    const t = text, a = algorithm
    invoke<{ value: string; title: string; error: string }>('checksum_parse', { text: t, algorithm: a }).then((r) => {
      if (t !== text || a !== algorithm) return
      value = r.value
      title = r.title
      error = r.error
      valid = !r.error
    })
  })
</script>

<div style="display:flex;gap:8px;align-items:center">
  <select class="input" style="width:124px" bind:value={algorithm} {disabled}>
    <option value="auto">Auto-detect</option>
    <option value="md5">MD5</option>
    <option value="sha1">SHA-1</option>
    <option value="sha256">SHA-256</option>
    <option value="sha512">SHA-512</option>
  </select>
  <div style="position:relative;flex:1">
    <input class="input mono" style="width:100%" style:padding-right={title ? '86px' : null} placeholder="Paste a hash (optional)"
      bind:this={field} bind:value={text} aria-invalid={!valid} {disabled} />
    {#if title}<span class="tpill accent" style="position:absolute;right:6px;top:50%;transform:translateY(-50%)">{title}</span>{/if}
  </div>
</div>
<div class={error ? 'errorText' : 'faint'} style="font-size:0.92em">{error || hint}</div>
