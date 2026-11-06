<script lang="ts">
  let { text, tone = 'muted' }: { text: string; tone?: string } = $props()
</script>

<span class="tpill {tone}">{text}</span>
