import { chromaKey, hueKey, LEGACY_KEYS, syncThemeColor } from '@/themes/color'

/**
 * Add a theme: create `<id>.css` scoped to `[data-theme='<id>']`, import it in index.css, list it here.
 * A theme is styling only: derive its colors from `--base-hue` (see color.ts) unless they are part
 * of its identity (christmas). A theme may also set fonts (`@import` its @fontsource files in its
 * own CSS) and decoration, but set `font-family` on elements: `@theme inline` freezes `font-sans`.
 */
export const themes = [
  { id: 'royal', label: 'Królewski', tone: 'dark' },
  { id: 'christmas', label: 'Boże Narodzenie', tone: 'dark' },
  { id: 'newyear', label: 'Sylwester', tone: 'dark' },
  { id: 'halloween', label: 'Halloween', tone: 'dark' },
  { id: 'polska', label: 'Polska', tone: 'dark' },
  { id: 'easter', label: 'Wielkanoc', tone: 'light' },
  { id: 'birthday', label: 'Urodziny', tone: 'light' },
  { id: 'birthday-dusk', label: 'Urodziny po zmierzchu', tone: 'dark' },
  { id: 'capybara', label: 'Kapibara', tone: 'light' },
  { id: 'capybara-dusk', label: 'Kapibara o zmierzchu', tone: 'dark' },
  { id: 'claymorphic', label: 'Obły', tone: 'light' },
  { id: 'boring-dark', label: 'Nudny czarnuch', tone: 'dark' },
  { id: 'boring-light', label: 'Nudny białas', tone: 'light' },
  { id: 'casino', label: 'Kasyno', tone: 'dark' },
  { id: 'neon', label: 'Neon', tone: 'dark' },
  { id: 'terminal', label: 'Terminal', tone: 'dark' },
  { id: 'parchment', label: 'Pergamin', tone: 'light' },
  { id: 'gazetka', label: 'Gazetka', tone: 'light' },
  { id: 'gabinet', label: 'Gabinet', tone: 'dark' },
] as const

/** Whether a theme's surfaces are light or dark; the picker groups by it. */
export type ThemeTone = (typeof themes)[number]['tone']

export type ThemeId = (typeof themes)[number]['id']

export const DEFAULT_THEME: ThemeId = 'royal'

const STORAGE_KEY = 'theme'
const OVERRIDE_KEY = 'theme-override'
const MIGRATED_KEY = 'theme-v2'
const PER_THEME_KEY = 'color-per-theme'

function isThemeId(value: string | null): value is ThemeId {
  return themes.some((t) => t.id === value)
}

/** `default` and `royal` used to be two themes; they are now royal at its default hue and hue 265. Runs once. */
function migrateLegacyTheme(): void {
  if (localStorage.getItem(MIGRATED_KEY)) return
  if (
    localStorage.getItem(STORAGE_KEY) === 'royal' &&
    localStorage.getItem(LEGACY_KEYS[0]) === null
  ) {
    localStorage.setItem(LEGACY_KEYS[0], '265')
  }
  localStorage.setItem(MIGRATED_KEY, '1')
}

/** The one hue and saturation every theme used to share become the picked theme's own. Runs once. */
function migrateColorsPerTheme(): void {
  if (localStorage.getItem(PER_THEME_KEY)) return
  const stored = localStorage.getItem(STORAGE_KEY)
  const theme = isThemeId(stored) ? stored : DEFAULT_THEME
  const [hue, chroma] = LEGACY_KEYS.map((key) => localStorage.getItem(key))
  if (hue !== null) localStorage.setItem(hueKey(theme), hue)
  if (chroma !== null) localStorage.setItem(chromaKey(theme), chroma)
  LEGACY_KEYS.forEach((key) => localStorage.removeItem(key))
  localStorage.setItem(PER_THEME_KEY, '1')
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
    migrateColorsPerTheme()
    const stored = localStorage.getItem(STORAGE_KEY)
    return isThemeId(stored) ? stored : DEFAULT_THEME
  } catch {
    return DEFAULT_THEME
  }
}

/**
 * A theme forced over the user's pick by a running event (birthday themes; see
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

const JULY = 6
const OCTOBER = 9

/** New Year's Eve: 31 December. */
function isNewYearsEve(now: Date): boolean {
  return now.getMonth() === 11 && now.getDate() === 31
}

/** International Capybara Day: 10 July. */
function isCapybaraDay(now: Date): boolean {
  return now.getMonth() === JULY && now.getDate() === 10
}

/** Easter Sunday of `year` (the Gregorian "Anonymous" algorithm), at local midnight. */
function easterSunday(year: number): Date {
  const a = year % 19
  const b = Math.floor(year / 100)
  const c = year % 100
  const d = Math.floor(b / 4)
  const g = Math.floor((b - Math.floor((b + 8) / 25) + 1) / 3)
  const h = (19 * a + b - d - g + 15) % 30
  const l = (32 + 2 * (b % 4) + 2 * Math.floor(c / 4) - h - (c % 4)) % 7
  const m = Math.floor((a + 11 * h + 22 * l) / 451)
  const month = Math.floor((h + l - 7 * m + 114) / 31) - 1
  const day = ((h + l - 7 * m + 114) % 31) + 1
  return new Date(year, month, day)
}

/** Easter runs from Palm Sunday to Easter Monday (the day after Easter, `Śmigus-dyngus`). */
function isEaster(now: Date): boolean {
  const sunday = easterSunday(now.getFullYear())
  const start = new Date(sunday.getFullYear(), sunday.getMonth(), sunday.getDate() - 7)
  const end = new Date(sunday.getFullYear(), sunday.getMonth(), sunday.getDate() + 2)
  return now >= start && now < end
}

/** Halloween runs for the two weeks before it and on the day: 17-31 October. */
const HALLOWEEN_FIRST_DAY = 17

/**
 * The Polish theme's days only: 1-3 May (Labour Day, Flag Day, Constitution Day) and Independence
 * Day (11 November).
 */
function isPolishHoliday(now: Date): boolean {
  const [month, day] = [now.getMonth(), now.getDate()]
  return (month === 4 && day <= 3) || (month === 10 && day === 11)
}

/**
 * The capybara theme that matches the user's pick: the light one for a light theme, the dusk one
 * for a dark theme, and a capybara theme they picked themselves stays as it is.
 */
function capybaraFor(selected: ThemeId | undefined): ThemeId {
  if (selected === 'capybara' || selected === 'capybara-dusk') return selected
  return themes.find((t) => t.id === selected)?.tone === 'dark' ? 'capybara-dusk' : 'capybara'
}

/** The birthday theme that matches the user's pick: the light one for a light theme, dusk for a dark one. */
export function birthdayFor(selected: ThemeId | undefined): ThemeId {
  return themes.find((t) => t.id === selected)?.tone === 'dark' ? 'birthday-dusk' : 'birthday'
}

/**
 * A theme forced by the calendar, otherwise `null`: newyear on New Year's Eve (31 December, ahead
 * of christmas), capybara on 10 July (light or dusk to match
 * `selected`, the theme the user picked), easter from Palm Sunday to Easter Monday, polska on
 * 1-3 May and 11 November, christmas all of December, halloween from 17 October to Halloween
 * (local time). The birthday themes are not here: they follow the members' birthdays, which only the
 * server knows (`EventThemeSync`). Takes priority over an event override (`getStoredOverride`), so add new seasons
 * here.
 */
export function getSeasonalTheme(now: Date = new Date(), selected?: ThemeId): ThemeId | null {
  if (isNewYearsEve(now)) return 'newyear'
  if (isCapybaraDay(now)) return capybaraFor(selected)
  if (isEaster(now)) return 'easter'
  if (isPolishHoliday(now)) return 'polska'
  if (now.getMonth() === 11) return 'christmas'
  if (now.getMonth() === OCTOBER && now.getDate() >= HALLOWEEN_FIRST_DAY) return 'halloween'
  return null
}

/** Themes that only apply by themselves (a season or an event), so they are not in the picker. */
const AUTOMATIC_THEMES: ThemeId[] = [
  'christmas',
  'newyear',
  'halloween',
  'polska',
  'easter',
  'birthday',
  'birthday-dusk',
]

/**
 * Themes with their own fixed palette (the seasonal ones), not derived from `--base-hue`: the hue
 * and saturation sliders do nothing for them. The capybara themes are too but can also be picked.
 */
const FIXED_COLOR_THEMES: ThemeId[] = [...AUTOMATIC_THEMES, 'capybara', 'capybara-dusk']

export const hasFixedColors = (id: ThemeId): boolean => FIXED_COLOR_THEMES.includes(id)

/** Themes the user can pick. Christmas applies by itself in December. */
export const selectableThemes = themes.filter((t) => !AUTOMATIC_THEMES.includes(t.id))

/** The picker's groups, in order: Jasne, then Ciemne. */
export const themeGroups: { tone: ThemeTone; label: string; themes: typeof selectableThemes }[] = [
  { tone: 'light', label: 'Jasne', themes: selectableThemes.filter((t) => t.tone === 'light') },
  { tone: 'dark', label: 'Ciemne', themes: selectableThemes.filter((t) => t.tone === 'dark') },
]
