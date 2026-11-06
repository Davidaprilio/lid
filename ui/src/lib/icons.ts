// LID's "Signal" icon set (lider/ui/assets/icons), recoloured like icons.py: the ink colour follows the
// text colour of the icon, the blue accent becomes the theme's accent. Some icons keep fixed colours.
const files = import.meta.glob('../../../lider/ui/assets/icons/*.svg', { query: '?raw', import: 'default', eager: true }) as Record<string, string>

const SVG: Record<string, string> = {}
for (const [path, text] of Object.entries(files)) {
  SVG[path.split('/').pop()!.replace('.svg', '')] = text
}

const ALIASES: Record<string, string> = { download: 'folder-download', downloads: 'folder-download', 'folder-open': 'folder' }
/** Icons drawn in their own colours. */
const FIXED = new Set(['add-white', 'play-white', 'finished', 'shield-check', 'delete', 'shield-alert', 'app'])

export function hasIcon(name: string): boolean {
  return (ALIASES[name] ?? name) in SVG
}

/** The SVG markup at `size` px; `accent` replaces the set's blue. */
export function iconSvg(name: string, size: number, accent: string): string {
  const key = ALIASES[name] ?? name
  let svg = SVG[key] ?? SVG['folder'] ?? ''
  if (!FIXED.has(key)) {
    svg = svg.replaceAll('#1E293B', 'currentColor').replaceAll('#2563EB', accent)
  }
  return svg.replace(/width="24" height="24"/, `width="${size}" height="${size}"`)
}

export const ICON_NAMES = Object.keys(SVG).sort()
