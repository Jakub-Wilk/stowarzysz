import type { ThemeId } from '@/themes'

/**
 * The base color: one HSL hue (0-360) that themes derive their palette from via `--base-hue`
 * (see royal.css / claymorphic.css). Unset, each theme falls back to its own default hue in CSS.
 * Alongside it, `--base-chroma` is a multiplier on every derived color's chroma
 * (1 = as designed, 0 = grey). The user picks both in the account panel; from the console use
 * `setBaseColor(150)` or `setBaseColor('#2a9d8f')`, and reset with `setBaseColor()`.
 */
const STORAGE_KEY = 'base-hue'
const CHROMA_KEY = 'base-chroma'

/** Hue and saturation are remembered per theme: each theme's keys are these plus `:<theme id>`. */
export const hueKey = (theme: ThemeId): string => `${STORAGE_KEY}:${theme}`
export const chromaKey = (theme: ThemeId): string => `${CHROMA_KEY}:${theme}`

/** Before per-theme colors there was one hue and one saturation for every theme. */
export const LEGACY_KEYS = [STORAGE_KEY, CHROMA_KEY] as const

export const MAX_CHROMA = 1.5

export function normalizeHue(hue: number): number {
  return ((hue % 360) + 360) % 360
}

/** A hue angle, or a `#rgb` / `#rrggbb` color converted to its HSL hue. */
export function parseBaseColor(input: number | string): number {
  if (typeof input === 'number') {
    if (!Number.isFinite(input)) throw new RangeError('Hue must be a finite number')
    return normalizeHue(input)
  }
  const match = /^#?([0-9a-f]{3}|[0-9a-f]{6})$/i.exec(input.trim())
  if (!match) throw new RangeError(`Expected a hue (0-360) or a hex color, got "${input}"`)
  let hex = match[1]
  if (hex.length === 3) hex = [...hex].map((c) => c + c).join('')
  const [r, g, b] = [0, 2, 4].map((i) => {
    return parseInt(hex.slice(i, i + 2), 16) / 255
  })
  const max = Math.max(r, g, b)
  const d = max - Math.min(r, g, b)
  if (d === 0) return 0
  const h = max === r ? ((g - b) / d) % 6 : max === g ? (b - r) / d + 2 : (r - g) / d + 4
  return normalizeHue(h * 60)
}

export function getStoredHue(theme: ThemeId): number | null {
  try {
    const stored = localStorage.getItem(hueKey(theme))
    const hue = stored === null ? NaN : Number(stored)
    return Number.isFinite(hue) ? normalizeHue(hue) : null
  } catch {
    return null
  }
}

export function storeHue(theme: ThemeId, hue: number | null): void {
  try {
    if (hue === null) localStorage.removeItem(hueKey(theme))
    else localStorage.setItem(hueKey(theme), String(hue))
  } catch {
    // Storage unavailable; the color still applies for this session.
  }
}

export function getStoredChroma(theme: ThemeId): number | null {
  try {
    const stored = localStorage.getItem(chromaKey(theme))
    const chroma = stored === null ? NaN : Number(stored)
    return Number.isFinite(chroma) ? Math.min(Math.max(chroma, 0), MAX_CHROMA) : null
  } catch {
    return null
  }
}

export function storeChroma(theme: ThemeId, chroma: number | null): void {
  try {
    if (chroma === null) localStorage.removeItem(chromaKey(theme))
    else localStorage.setItem(chromaKey(theme), String(chroma))
  } catch {
    // Storage unavailable; the saturation still applies for this session.
  }
}

const listeners = new Set<() => void>()

/** For `useSyncExternalStore`: called whenever the theme, hue or saturation is applied. */
export function subscribeColor(listener: () => void): () => void {
  listeners.add(listener)
  return () => void listeners.delete(listener)
}

/** Keeps the browser chrome (address bar, PWA window) in step with the active theme and hue. */
export function syncThemeColor(): void {
  listeners.forEach((listener) => listener())
  const meta = document.querySelector<HTMLMetaElement>('meta[name="theme-color"]')
  const background = getComputedStyle(document.body).backgroundColor
  if (meta && background) meta.content = background
}

/** Applies the hue (or `null` to fall back to the theme's own) and remembers it for `theme`. */
export function applyBaseColor(theme: ThemeId, hue: number | null, persist = true): void {
  const root = document.documentElement
  if (hue === null) root.style.removeProperty('--base-hue')
  else root.style.setProperty('--base-hue', String(hue))
  if (persist) storeHue(theme, hue)
  syncThemeColor()
}

/** Applies the chroma multiplier (or `null` for the default of 1) and remembers it for `theme`. */
export function applyBaseChroma(theme: ThemeId, chroma: number | null, persist = true): void {
  const root = document.documentElement
  if (chroma === null) root.style.removeProperty('--base-chroma')
  else root.style.setProperty('--base-chroma', String(chroma))
  if (persist) storeChroma(theme, chroma)
  syncThemeColor()
}

/**
 * Console entry point, for the theme on screen: `setBaseColor(150)`, `setBaseColor('#2a9d8f')`,
 * `setBaseColor()` to reset.
 */
export function setBaseColor(input?: number | string): number | null {
  const hue = input === undefined ? null : parseBaseColor(input)
  applyBaseColor(document.documentElement.dataset.theme as ThemeId, hue)
  return hue
}

declare global {
  interface Window {
    setBaseColor: typeof setBaseColor
  }
}
