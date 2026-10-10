import { useCallback, useLayoutEffect, useMemo, useState, type ReactNode } from 'react'

import { applyBaseChroma, applyBaseColor, getStoredChroma, getStoredHue } from '@/themes/color'
import {
  applyTheme,
  getSeasonalTheme,
  getStoredOverride,
  getStoredTheme,
  storeOverride,
  storeTheme,
  themes,
  type ThemeId,
} from '@/themes'
import { ThemeContext, type ThemeContextValue } from '@/themes/context'

export function ThemeProvider({ children }: { children: ReactNode }) {
  const [selected, setSelected] = useState<ThemeId>(getStoredTheme)
  const [override, setOverrideState] = useState<ThemeId | null>(getStoredOverride)
  // A seasonal theme beats an event override (a Secret Santa in October stays Halloween).
  // Checked on every render, so a long-open tab picks up a new season on its next update.
  const theme = getSeasonalTheme(undefined, selected) ?? override ?? selected

  // Layout effect: runs before first paint, so there is no flash of the wrong theme.
  // Each theme has its own hue and saturation, applied together with it.
  useLayoutEffect(() => {
    applyTheme(theme)
    applyBaseColor(theme, getStoredHue(theme), false)
    applyBaseChroma(theme, getStoredChroma(theme), false)
  }, [theme])

  const setTheme = useCallback((id: ThemeId) => {
    storeTheme(id)
    setSelected(id)
  }, [])
  const setOverride = useCallback((id: ThemeId | null) => {
    storeOverride(id)
    setOverrideState(id)
  }, [])

  const value = useMemo<ThemeContextValue>(
    () => ({ theme, selected, themes, setTheme, setOverride }),
    [theme, selected, setTheme, setOverride],
  )

  return <ThemeContext.Provider value={value}>{children}</ThemeContext.Provider>
}
