/**
 * The base color: one oklch hue (0-360) that themes derive their palette from via `--base-hue`
 * (see royal.css / claymorphic.css). Unset, each theme falls back to its own default hue in CSS.
 * Alongside it, `--base-chroma` is a multiplier on every derived color's chroma
 * (1 = as designed, 0 = grey). The user picks both in the account panel; from the console use
 * `setBaseColor(150)` or `setBaseColor('#2a9d8f')`, and reset with `setBaseColor()`.
 */
const STORAGE_KEY = 'base-hue'
const CHROMA_KEY = 'base-chroma'

export const MAX_CHROMA = 1.5

export function normalizeHue(hue: number): number {
  return ((hue % 360) + 360) % 360
}

/** A hue angle, or a `#rgb` / `#rrggbb` color converted to its oklch hue. */
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
    const c = parseInt(hex.slice(i, i + 2), 16) / 255
    return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4
  })
  const l = Math.cbrt(0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b)
  const m = Math.cbrt(0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b)
  const s = Math.cbrt(0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b)
  const a = 1.9779984951 * l - 2.428592205 * m + 0.4505937099 * s
  const bb = 0.0259040371 * l + 0.7827717662 * m - 0.808675766 * s
  return normalizeHue((Math.atan2(bb, a) * 180) / Math.PI)
}

export function getStoredHue(): number | null {
  try {
    const stored = localStorage.getItem(STORAGE_KEY)
    const hue = stored === null ? NaN : Number(stored)
    return Number.isFinite(hue) ? normalizeHue(hue) : null
  } catch {
    return null
  }
}

export function storeHue(hue: number | null): void {
  try {
    if (hue === null) localStorage.removeItem(STORAGE_KEY)
    else localStorage.setItem(STORAGE_KEY, String(hue))
  } catch {
    // Storage unavailable; the color still applies for this session.
  }
}

export function getStoredChroma(): number | null {
  try {
    const stored = localStorage.getItem(CHROMA_KEY)
    const chroma = stored === null ? NaN : Number(stored)
    return Number.isFinite(chroma) ? Math.min(Math.max(chroma, 0), MAX_CHROMA) : null
  } catch {
    return null
  }
}

export function storeChroma(chroma: number | null): void {
  try {
    if (chroma === null) localStorage.removeItem(CHROMA_KEY)
    else localStorage.setItem(CHROMA_KEY, String(chroma))
  } catch {
    // Storage unavailable; the saturation still applies for this session.
  }
}

/** Keeps the browser chrome (address bar, PWA window) in step with the active theme and hue. */
export function syncThemeColor(): void {
  const meta = document.querySelector<HTMLMetaElement>('meta[name="theme-color"]')
  const background = getComputedStyle(document.body).backgroundColor
  if (meta && background) meta.content = background
}

/** Applies the hue (or `null` to fall back to the theme's own) and remembers it. */
export function applyBaseColor(hue: number | null, persist = true): void {
  const root = document.documentElement
  if (hue === null) root.style.removeProperty('--base-hue')
  else root.style.setProperty('--base-hue', String(hue))
  if (persist) storeHue(hue)
  syncThemeColor()
}

/** Applies the chroma multiplier (or `null` for the default of 1) and remembers it. */
export function applyBaseChroma(chroma: number | null, persist = true): void {
  const root = document.documentElement
  if (chroma === null) root.style.removeProperty('--base-chroma')
  else root.style.setProperty('--base-chroma', String(chroma))
  if (persist) storeChroma(chroma)
  syncThemeColor()
}

/** Console entry point: `setBaseColor(150)`, `setBaseColor('#2a9d8f')`, `setBaseColor()` to reset. */
export function setBaseColor(input?: number | string): number | null {
  const hue = input === undefined ? null : parseBaseColor(input)
  applyBaseColor(hue)
  return hue
}

declare global {
  interface Window {
    setBaseColor: typeof setBaseColor
  }
}
