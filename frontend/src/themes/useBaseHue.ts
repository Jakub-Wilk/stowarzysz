import { useState } from 'react'

import { applyBaseChroma, applyBaseColor, getStoredChroma, getStoredHue } from '@/themes/color'
import { useTheme } from '@/themes/context'

/** The hue the active theme paints with when the user hasn't picked one (its `--base-hue`). */
function themeDefaultHue(): number {
  const hue = parseFloat(getComputedStyle(document.documentElement).getPropertyValue('--base-hue'))
  return Number.isFinite(hue) ? hue : 0
}

/**
 * The base hue in effect, kept in sync with `<html>` and localStorage. `custom` is false while the
 * active theme's own default applies, in which case `hue` is that default.
 */
export function useBaseHue() {
  // Re-read the default when the theme changes: each theme has its own.
  useTheme()
  const [stored, setStored] = useState<number | null>(getStoredHue)

  const setHue = (next: number) => {
    applyBaseColor(next)
    setStored(next)
  }
  const reset = () => {
    applyBaseColor(null)
    setStored(null)
  }
  return {
    hue: stored ?? themeDefaultHue(),
    custom: stored !== null,
    setHue,
    reset,
  }
}

/** The saturation multiplier (1 = as designed), kept in sync with `<html>` and localStorage. */
export function useBaseChroma() {
  const [stored, setStored] = useState<number | null>(getStoredChroma)

  const setChroma = (next: number) => {
    applyBaseChroma(next)
    setStored(next)
  }
  const reset = () => {
    applyBaseChroma(null)
    setStored(null)
  }
  return { chroma: stored ?? 1, custom: stored !== null, setChroma, reset }
}
