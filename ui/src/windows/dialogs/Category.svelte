<script lang="ts">
  // The category editor in its own window (Add category / Properties from the sidebar).
  import CategoryForm, { type Category } from './CategoryForm.svelte'
  import { call, close, followTheme, fit, params } from '../../lib/dialog'
  import type { ThemeName } from '../../theme/tokens'

  const name = params().get('name')
  let theme = $state<ThemeName>('ember')
  let cat = $state<Category | null | undefined>(undefined)
  followTheme((t) => (theme = t))
  call<any>('settings_get').then((s) => {
    cat = name ? s.categories.find((c: Category) => c.name === name) ?? null : null
    fit(640)
  })

  async function ok(c: Category) {
    await call('category_save', { old: name, category: c })
    close()
  }
</script>

{#if cat !== undefined}
  <CategoryForm category={cat} {theme} onok={ok} oncancel={close} />
{/if}
