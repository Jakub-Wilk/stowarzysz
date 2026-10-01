import { createContext, useContext } from 'react'

import type { themes, ThemeId } from '@/themes'

export interface ThemeContextValue {
  theme: ThemeId
  setTheme: (id: ThemeId) => void
  themes: typeof themes
}

export const ThemeContext = createContext<ThemeContextValue | null>(null)

export function useTheme(): ThemeContextValue {
  const ctx = useContext(ThemeContext)
  if (!ctx) throw new Error('useTheme must be used within ThemeProvider')
  return ctx
}
