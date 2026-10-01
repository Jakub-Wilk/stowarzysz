import { useEffect } from 'react'

import { SnowLayer } from '@/features/secretsanta/SnowLayer'
import { useSanta } from '@/features/secretsanta/hooks'
import { useTheme } from '@/themes/context'

/**
 * The whole app wears the Christmas theme while a Secret Santa event is active. The theme is
 * remembered by `applyTheme`, so a reload paints it before the status has been fetched.
 */
export function SantaThemeSync() {
  const { data } = useSanta()
  const { theme, setTheme } = useTheme()
  const wanted = data === undefined ? null : data.active ? 'christmas' : 'default'

  useEffect(() => {
    if (wanted !== null && wanted !== theme) setTheme(wanted)
  }, [wanted, theme, setTheme])

  return theme === 'christmas' ? <SnowLayer /> : null
}
