import { getStoredHue, storeHue, syncThemeColor } from '@/themes/color'

/**
 * Add a theme: create `<id>.css` scoped to `[data-theme='<id>']`, import it in index.css, list it here.
 * A theme is styling only: derive its colors from `--base-hue` (see color.ts) unless they are part
 * of its identity (christmas).
 */
export const themes = [
  { id: 'royal', label: 'Królewski' },
  { id: 'christmas', label: 'Boże Narodzenie' },
  { id: 'claymorphic', label: 'Miękki' },
] as const

export type ThemeId = (typeof themes)[number]['id']

export const DEFAULT_THEME: ThemeId = 'royal'

const STORAGE_KEY = 'theme'
const OVERRIDE_KEY = 'theme-override'
const MIGRATED_KEY = 'theme-v2'

function isThemeId(value: string | null): value is ThemeId {
  return themes.some((t) => t.id === value)
}

/** `default` and `royal` used to be two themes; they are now royal at its default hue and hue 265. Runs once. */
function migrateLegacyTheme(): void {
  if (localStorage.getItem(MIGRATED_KEY)) return
  if (localStorage.getItem(STORAGE_KEY) === 'royal' && getStoredHue() === null) storeHue(265)
  localStorage.setItem(MIGRATED_KEY, '1')
}

/** Christmas used to be saved as if the user had picked it; it is an override now. */
function migrateChristmas(): void {
  if (localStorage.getItem(STORAGE_KEY) !== 'christmas') return
  localStorage.setItem(STORAGE_KEY, DEFAULT_THEME)
  localStorage.setItem(OVERRIDE_KEY, 'christmas')
}

/** The theme the user picked. */
export function getStoredTheme(): ThemeId {
  try {
    migrateLegacyTheme()
    migrateChristmas()
    const stored = localStorage.getItem(STORAGE_KEY)
    return isThemeId(stored) ? stored : DEFAULT_THEME
  } catch {
    return DEFAULT_THEME
  }
}

/**
 * A theme forced over the user's pick by a running event (christmas during Secret Santa; see
 * `EventThemeSync`). Remembered so a reload paints it before the app knows whether it still applies.
 */
export function getStoredOverride(): ThemeId | null {
  try {
    migrateChristmas()
    const stored = localStorage.getItem(OVERRIDE_KEY)
    return isThemeId(stored) ? stored : null
  } catch {
    return null
  }
}

function store(key: string, value: string | null): void {
  try {
    if (value === null) localStorage.removeItem(key)
    else localStorage.setItem(key, value)
  } catch {
    // Storage unavailable; the theme still applies for this session.
  }
}

export const storeTheme = (id: ThemeId): void => store(STORAGE_KEY, id)
export const storeOverride = (id: ThemeId | null): void => store(OVERRIDE_KEY, id)

export function applyTheme(id: ThemeId): void {
  document.documentElement.dataset.theme = id
  syncThemeColor()
}

/**
 * A theme forced by the calendar: christmas all of December (local time), otherwise none. Takes
 * priority over an event override (`getStoredOverride`), so add new seasons here.
 */
export function getSeasonalTheme(now: Date = new Date()): ThemeId | null {
  return now.getMonth() === 11 ? 'christmas' : null
}

/** Themes the user can pick. Christmas applies by itself in December and during Secret Santa. */
export const selectableThemes = themes.filter((t) => t.id !== 'christmas')
