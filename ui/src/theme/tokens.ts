// The three themes of LID (theme.py in 1.x), as CSS custom properties.
export type ThemeName = 'ember' | 'ember-dark' | 'dark'

const T: Record<string, [string, string, string]> = {
  //          ember      ember-dark  dark
  win: ['#F7F5F2', '#1A1714', '#1B1E24'],
  pane: ['#FFFFFF', '#141210', '#15181D'],
  alt: ['#FBFAF8', '#181512', '#181B21'],
  hdr: ['#F9F7F4', '#1D1915', '#1D2027'],
  line: ['#E7E2DA', '#2E2925', '#2B3039'],
  line2: ['#D8D1C6', '#40382F', '#3A404B'],
  tx: ['#2A2420', '#EDE6DF', '#E2E8F0'],
  mu: ['#6B625A', '#AEA399', '#A3ADBD'],
  fa: ['#9A9087', '#7C7168', '#6E7787'],
  ac: ['#EA580C', '#F97316', '#F97316'],
  acsel: ['#FDEEE4', '#3A2417', '#33261F'],
  actx: ['#C2410C', '#FDBA74', '#FDBA74'],
  ok: ['#0D9488', '#2DD4BF', '#2DD4BF'],
  bad: ['#DC2626', '#F87171', '#F87171'],
  track: ['#EFEAE3', '#2B2520', '#2A2F38'],
  input: ['#FFFFFF', '#100E0C', '#111318'],
  btn: ['#FFFFFF', '#221E1A', '#252932'],
  ink: ['#1E293B', '#E2E8F0', '#E2E8F0'],
  icon_ac: ['#EA580C', '#F97316', '#F97316'],
  title: ['#EEEAE4', '#221E1A', '#23262D'],
}

const INDEX: Record<ThemeName, number> = { ember: 0, 'ember-dark': 1, dark: 2 }

export const TITLES: Record<ThemeName, string> = { ember: 'Ember', 'ember-dark': 'Ember Dark', dark: 'Dark' }

export function token(theme: ThemeName, name: keyof typeof T): string {
  return T[name][INDEX[theme] ?? 0]
}

function rgba(hex: string, alpha: number): string {
  const n = parseInt(hex.slice(1), 16)
  return `rgba(${(n >> 16) & 255}, ${(n >> 8) & 255}, ${n & 255}, ${alpha})`
}

/** Put the theme's colours on :root (and the alpha variants the pills and badges use). */
export function applyTheme(theme: ThemeName) {
  const root = document.documentElement
  const i = INDEX[theme] ?? 0
  for (const [k, v] of Object.entries(T)) root.style.setProperty(`--${k.replace('_', '-')}`, v[i])
  const ac = T.ac[i], ok = T.ok[i], bad = T.bad[i], acsel = T.acsel[i]
  const vars: Record<string, string> = {
    'ok-34': rgba(ok, 34 / 255), 'ok-31': rgba(ok, 31 / 255), 'ok-14': rgba(ok, 0.14), 'ok-13': rgba(ok, 0.13),
    'ok-08': rgba(ok, 0.08), 'ok-35': rgba(ok, 0.35),
    'bad-30': rgba(bad, 30 / 255), 'bad-12': rgba(bad, 0.12), 'bad-07': rgba(bad, 0.07), 'bad-35': rgba(bad, 0.35),
    'ac-70': rgba(ac, 70 / 255), 'fa-70': rgba(T.fa[i], 70 / 255), 'acsel-150': rgba(acsel, 150 / 255),
  }
  for (const [k, v] of Object.entries(vars)) root.style.setProperty(`--${k}`, v)
  root.dataset.theme = theme
  root.style.colorScheme = theme === 'ember' ? 'light' : 'dark'
}
