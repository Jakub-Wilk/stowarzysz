import { useMemo, useState, type ReactNode } from 'react'

import { applyBaseColor, getStoredHue } from '@/themes/color'
import { applyTheme, getStoredTheme, themes, type ThemeId } from '@/themes'
import { ThemeContext, type ThemeContextValue } from '@/themes/context'

export function ThemeProvider({ children }: { children: ReactNode }) {
  const [theme, setThemeState] = useState<ThemeId>(() => {
    const initial = getStoredTheme()
    applyBaseColor(getStoredHue(), false)
    applyTheme(initial) // before first paint, so there is no flash of the wrong theme
    return initial
  })

  const value = useMemo<ThemeContextValue>(
    () => ({
      theme,
      themes,
      setTheme: (id) => {
        applyTheme(id)
        setThemeState(id)
      },
    }),
    [theme],
  )

  return <ThemeContext.Provider value={value}>{children}</ThemeContext.Provider>
}
