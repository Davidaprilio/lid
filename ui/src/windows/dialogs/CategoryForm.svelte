<script lang="ts">
  // The category editor (1.x options.CategoryDialog), used in its own window and over other dialogs.
  import { open as openDir } from '@tauri-apps/plugin-dialog'
  import Shell from '../../components/form/Shell.svelte'
  import Row from '../../components/form/Row.svelte'
  import TagInput from '../../components/form/TagInput.svelte'
  import { untrack } from 'svelte'
  import type { ThemeName } from '../../theme/tokens'

  export interface Category { name: string; extensions: string[]; save_dir: string; sites: string[]; remember_last: boolean }
  let { category, home, theme, onok, oncancel }: {
    category: Category | null; home: string; theme: ThemeName; onok: (c: Category) => void; oncancel: () => void
  } = $props()
  // The form edits a copy taken when it opens.
  const start = untrack(() => ({ category, home }))
  const isNew = start.category === null
  let name = $state(start.category?.name ?? '')
  let exts = $state<string[]>(start.category?.extensions ?? [])
  let useSites = $state((start.category?.sites.length ?? 0) > 0)
  let sites = $state<string[]>(start.category?.sites ?? [])
  let folder = $state(start.category?.save_dir ?? `${start.home}/Downloads/New`)
  let remember = $state(start.category?.remember_last ?? false)
  const lower = (n: string) => n.toLowerCase()
  const icon = $derived(isNew ? 'folder' : ['compressed', 'documents', 'music', 'programs', 'video'].includes(lower(start.category!.name)) ? `category-${lower(start.category!.name)}` : 'folder')
  let tags: TagInput | undefined = $state()
  let siteTags: TagInput | undefined = $state()

  async function browse() {
    const d = await openDir({ directory: true, title: 'Select a Folder', defaultPath: folder })
    if (typeof d === 'string') folder = d
  }

  function ok() {
    tags?.commit()
    siteTags?.commit()
    if (!name.trim() || !folder.trim()) return
    onok({ name: name.trim(), extensions: exts, save_dir: folder.trim(), sites: useSites ? sites : [], remember_last: remember })
  }
</script>

<Shell {theme} {icon} tile title="LID categories" subtitle={isNew ? 'New category' : `Edit the ${start.category!.name} category`}>
  <Row label="Category name" width={110}>
    <input class="input" bind:value={name} readonly={start.category?.name === 'General'} />
  </Row>
  <div>Automatically put in this category the following file types:</div>
  <TagInput bind:this={tags} bind:values={exts} placeholder="Add a type, e.g. mkv" kind="extension" noun="types" />
  <label class="check"><input type="checkbox" bind:checked={useSites} /> Automatically put in this category the files from these sites:</label>
  <TagInput bind:this={siteTags} bind:values={sites} placeholder="Add a site, e.g. *.example.com" kind="site" noun="sites" disabled={!useSites} />
  <div>Save future downloads of this category to:</div>
  <div style="display:flex;gap:8px"><input class="input mono" style="flex:1" bind:value={folder} /><button class="btn" onclick={browse}>Browse...</button></div>
  <label class="check"><input type="checkbox" bind:checked={remember} /> Remember last save path</label>
  {#snippet footer()}
    <span class="grow"></span>
    <button class="btn" onclick={oncancel}>Cancel</button>
    <button class="btn primary" onclick={ok}>OK</button>
  {/snippet}
</Shell>
