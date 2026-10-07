import { useEffect } from 'react'

import { SnowLayer } from '@/features/secretsanta/SnowLayer'
import { useSanta } from '@/features/secretsanta/hooks'
import { useTheme } from '@/themes/context'

/**
 * The whole app wears the Christmas theme while a Secret Santa event is active. The theme is
 * remembered by `applyTheme`, so a reload paints it before the status has been fetched. Any other
 * theme (e.g. claymorphic, picked by the user) is left alone.
 */
export function SantaThemeSync() {
  const { data } = useSanta()
  const { theme, setTheme } = useTheme()
  const managed = theme === 'royal' || theme === 'christmas'
  const wanted = data === undefined ? null : data.active ? 'christmas' : 'royal'

  useEffect(() => {
    if (managed && wanted !== null && wanted !== theme) setTheme(wanted)
  }, [managed, wanted, theme, setTheme])

  return theme === 'christmas' ? <SnowLayer /> : null
}
