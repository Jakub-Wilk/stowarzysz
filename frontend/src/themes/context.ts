import { createContext, useContext } from 'react'

import type { themes, ThemeId } from '@/themes'

export interface ThemeContextValue {
  /** The theme in effect: the override if there is one, otherwise the user's pick. */
  theme: ThemeId
  /** The theme the user picked. */
  selected: ThemeId
  setTheme: (id: ThemeId) => void
  /** Force a theme over the user's pick, or `null` to go back to it. */
  setOverride: (id: ThemeId | null) => void
  themes: typeof themes
}

export const ThemeContext = createContext<ThemeContextValue | null>(null)

export function useTheme(): ThemeContextValue {
  const ctx = useContext(ThemeContext)
  if (!ctx) throw new Error('useTheme must be used within ThemeProvider')
  return ctx
}
