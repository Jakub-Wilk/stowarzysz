/** Add a theme: create `<id>.css` scoped to `[data-theme='<id>']`, import it in index.css, list it here. */
export const themes = [{ id: 'default', label: 'Default' }] as const

export type ThemeId = (typeof themes)[number]['id']

export const DEFAULT_THEME: ThemeId = 'default'

const STORAGE_KEY = 'theme'

function isThemeId(value: string | null): value is ThemeId {
  return themes.some((t) => t.id === value)
}

export function getStoredTheme(): ThemeId {
  try {
    const stored = localStorage.getItem(STORAGE_KEY)
    return isThemeId(stored) ? stored : DEFAULT_THEME
  } catch {
    return DEFAULT_THEME
  }
}

export function applyTheme(id: ThemeId): void {
  document.documentElement.dataset.theme = id
  try {
    localStorage.setItem(STORAGE_KEY, id)
  } catch {
    // Storage unavailable; the theme still applies for this session.
  }
}
