<script lang="ts">
  // A menu label with its "&" mnemonic underlined (shown while Alt is held).
  let { text }: { text: string } = $props()
  const parts = $derived.by(() => {
    const i = text.indexOf('&')
    if (i < 0 || i === text.length - 1) return { a: text, m: '', b: '' }
    return { a: text.slice(0, i), m: text[i + 1], b: text.slice(i + 2) }
  })
</script>

{parts.a}{#if parts.m}<u class="mn">{parts.m}</u>{/if}{parts.b}
