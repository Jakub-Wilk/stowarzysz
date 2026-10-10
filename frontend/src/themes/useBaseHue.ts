import { useState, useSyncExternalStore } from 'react'

import type { ThemeId } from '@/themes'
import {
  applyBaseChroma,
  applyBaseColor,
  getStoredChroma,
  getStoredHue,
  subscribeColor,
} from '@/themes/color'
import { useTheme } from '@/themes/context'

/** The hue the active theme paints with when the user hasn't picked one (its `--base-hue`). */
function themeDefaultHue(): number {
  const hue = parseFloat(getComputedStyle(document.documentElement).getPropertyValue('--base-hue'))
  return Number.isFinite(hue) ? hue : 0
}

/**
 * A value remembered per theme: the stored one for `theme`, re-read when the theme changes. The
 * setter is what applies and saves it for that theme.
 */
function useThemeValue(theme: ThemeId, read: (theme: ThemeId) => number | null) {
  const [state, setState] = useState({ theme, value: read(theme) })
  // Adjusting state while rendering: the value belongs to the theme it was read for.
  if (state.theme !== theme) setState({ theme, value: read(theme) })
  return [state.value, (value: number | null) => setState({ theme, value })] as const
}

/**
 * The base hue of the active theme, kept in sync with `<html>` and localStorage. `custom` is false
 * while the theme's own default applies, in which case `hue` is that default.
 */
export function useBaseHue() {
  const { theme } = useTheme()
  const [stored, setStored] = useThemeValue(theme, getStoredHue)
  // The default is read from the DOM, which follows the theme only after the render that changed it.
  const defaultHue = useSyncExternalStore(subscribeColor, themeDefaultHue)

  const setHue = (next: number) => {
    applyBaseColor(theme, next)
    setStored(next)
  }
  const reset = () => {
    applyBaseColor(theme, null)
    setStored(null)
  }
  return {
    hue: stored ?? defaultHue,
    custom: stored !== null,
    setHue,
    reset,
  }
}

/** The saturation multiplier (1 = as designed) of the active theme, kept in sync like the hue. */
export function useBaseChroma() {
  const { theme } = useTheme()
  const [stored, setStored] = useThemeValue(theme, getStoredChroma)

  const setChroma = (next: number) => {
    applyBaseChroma(theme, next)
    setStored(next)
  }
  const reset = () => {
    applyBaseChroma(theme, null)
    setStored(null)
  }
  return { chroma: stored ?? 1, custom: stored !== null, setChroma, reset }
}
