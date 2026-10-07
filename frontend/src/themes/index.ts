import { getStoredHue, storeHue, syncThemeColor } from '@/themes/color'

/**
 * Add a theme: create `<id>.css` scoped to `[data-theme='<id>']`, import it in index.css, list it here.
 * A theme is styling only: derive its colors from `--base-hue` (see color.ts) unless they are part
 * of its identity (christmas).
 */
export const themes = [
  { id: 'royal', label: 'Royal' },
  { id: 'christmas', label: 'Boże Narodzenie' },
  { id: 'claymorphic', label: 'Glina' },
] as const

export type ThemeId = (typeof themes)[number]['id']

export const DEFAULT_THEME: ThemeId = 'royal'

const STORAGE_KEY = 'theme'
const MIGRATED_KEY = 'theme-v2'

function isThemeId(value: string | null): value is ThemeId {
  return themes.some((t) => t.id === value)
}

/** `default` and `royal` used to be two themes; they are now royal at hue 22 and 300. Runs once. */
function migrateLegacyTheme(): void {
  if (localStorage.getItem(MIGRATED_KEY)) return
  if (localStorage.getItem(STORAGE_KEY) === 'royal' && getStoredHue() === null) storeHue(300)
  localStorage.setItem(MIGRATED_KEY, '1')
}

export function getStoredTheme(): ThemeId {
  try {
    migrateLegacyTheme()
    const stored = localStorage.getItem(STORAGE_KEY)
    return isThemeId(stored) ? stored : DEFAULT_THEME
  } catch {
    return DEFAULT_THEME
  }
}

export function applyTheme(id: ThemeId): void {
  document.documentElement.dataset.theme = id
  syncThemeColor()
  try {
    localStorage.setItem(STORAGE_KEY, id)
  } catch {
    // Storage unavailable; the theme still applies for this session.
  }
}
